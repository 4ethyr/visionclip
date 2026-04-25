mod capture;
mod doctor;
mod voice;
mod voice_overlay;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::UnixStream;
use tracing::{info, warn};
use uuid::Uuid;
use visionclip_common::{
    read_message, write_message, AppConfig, ApplicationLaunchJob, CaptureJob, JobResult,
    SessionType, VisionRequest, VoiceSearchJob,
};

#[derive(Debug, Parser)]
#[command(name = "visionclip")]
#[command(about = "CLI do VisionClip para enviar capturas ao daemon")]
struct Cli {
    #[arg(long)]
    action: Option<String>,

    #[arg(long)]
    image: Option<PathBuf>,

    #[arg(long)]
    capture_command: Option<String>,

    #[arg(long, default_value_t = false)]
    speak: bool,

    #[arg(long)]
    source_app: Option<String>,

    #[arg(long)]
    open_app: Option<String>,

    #[arg(long, default_value_t = false)]
    voice_agent: bool,

    #[arg(long, default_value_t = false)]
    doctor: bool,

    #[arg(long, default_value_t = false)]
    voice_request: bool,

    #[arg(long, default_value_t = false)]
    voice_search: bool,

    #[arg(long)]
    voice_transcript: Option<String>,

    #[arg(long, hide = true, default_value_t = false)]
    voice_overlay_listening: bool,

    #[arg(long, hide = true, default_value_t = 4000)]
    voice_overlay_duration_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.voice_overlay_listening {
        return voice_overlay::run_listening_overlay(cli.voice_overlay_duration_ms);
    }

    let config = AppConfig::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| config.general.log_level.clone()),
        )
        .init();

    if cli.doctor {
        let healthy = doctor::run(&config).await?;
        if !healthy {
            std::process::exit(1);
        }
        return Ok(());
    }

    if (cli.action.is_some() || cli.open_app.is_some())
        && (cli.voice_agent
            || cli.voice_request
            || cli.voice_search
            || cli.voice_transcript.is_some())
    {
        warn!("voice inputs were ignored because an explicit action was provided");
    }

    if let Some(app_name) = &cli.open_app {
        return run_open_application(&config, cli.speak, app_name, None).await;
    }

    let resolved_voice_agent = if cli.action.is_none() && cli.open_app.is_none() && cli.voice_agent
    {
        Some(
            voice::resolve_voice_agent_command(&config.voice, cli.voice_transcript.as_deref())
                .await?,
        )
    } else {
        None
    };

    if let Some(command) = &resolved_voice_agent {
        match command {
            voice::VoiceAgentCommand::OpenApplication {
                transcript,
                app_name,
            } => {
                return run_open_application(&config, cli.speak, app_name, Some(transcript)).await;
            }
            voice::VoiceAgentCommand::SearchWeb { transcript, query } => {
                let voice_search = voice::VoiceSearch {
                    transcript: transcript.clone(),
                    query: query.clone(),
                };
                return run_voice_search(&config, cli.speak, &voice_search).await;
            }
        }
    }

    let resolved_voice_request = if cli.action.is_none() && cli.voice_request {
        Some(voice::resolve_voice_request(&config.voice, cli.voice_transcript.as_deref()).await?)
    } else {
        None
    };
    let resolved_voice_search = if cli.action.is_none() && cli.voice_search {
        Some(voice::resolve_voice_search(&config.voice, cli.voice_transcript.as_deref()).await?)
    } else {
        None
    };

    if let Some(voice_search) = &resolved_voice_search {
        return run_voice_search(&config, cli.speak, voice_search).await;
    }

    let action = if let Some(action_string) = cli.action.clone() {
        action_string.parse().map_err(anyhow::Error::msg)?
    } else if let Some(voice_request) = &resolved_voice_request {
        voice_request.action.clone()
    } else {
        config
            .general
            .default_action
            .parse()
            .map_err(anyhow::Error::msg)?
    };
    let session_type = detect_session_type();
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();

    if let Some(voice_request) = &resolved_voice_request {
        info!(
            request_id = %request_id,
            transcript = %voice_request.transcript,
            resolved_action = voice_request.action.as_str(),
            "voice request resolved"
        );
    }

    info!(
        request_id = %request_id,
        action = action.as_str(),
        speak = cli.speak,
        session_type = ?session_type,
        "visionclip request started"
    );

    let capture_started_at = Instant::now();
    let image_bytes = capture::load_image_bytes(
        cli.image.as_ref(),
        cli.capture_command.as_deref(),
        &config,
        session_type.clone(),
    )
    .await?;
    let capture_ms = elapsed_ms(capture_started_at);

    info!(
        request_id = %request_id,
        action = action.as_str(),
        capture_ms,
        image_bytes = image_bytes.len(),
        "capture completed"
    );

    let socket_path = config.socket_path()?;
    let connect_started_at = Instant::now();
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;
    let connect_ms = elapsed_ms(connect_started_at);

    info!(
        request_id = %request_id,
        connect_ms,
        socket = %socket_path.display(),
        "daemon socket connected"
    );

    let job = CaptureJob {
        request_id,
        action,
        mime_type: "image/png".to_string(),
        image_bytes,
        session_type,
        speak: cli.speak,
        source_app: cli.source_app,
    };
    let request = VisionRequest::Capture(job);

    let daemon_roundtrip_started_at = Instant::now();
    write_message(&mut stream, &request).await?;
    let response: JobResult = read_message(&mut stream).await?;
    let daemon_roundtrip_ms = elapsed_ms(daemon_roundtrip_started_at);

    match response {
        JobResult::ClipboardText { text, spoken, .. } => {
            info!(
                request_id = %request_id,
                spoken,
                daemon_roundtrip_ms,
                total_ms = elapsed_ms(total_started_at),
                "clipboard response received"
            );
            println!("Resultado copiado para o clipboard:\n{}", text);
        }
        JobResult::BrowserQuery {
            query,
            summary,
            spoken,
            ..
        } => {
            info!(
                request_id = %request_id,
                spoken,
                daemon_roundtrip_ms,
                total_ms = elapsed_ms(total_started_at),
                "browser query response received"
            );
            println!("Consulta aberta no navegador: {}", query);
            if let Some(summary) = summary {
                println!("\nResumo inicial da pesquisa:\n{}", summary);
            }
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ActionStatus {
            message, spoken, ..
        } => {
            info!(
                request_id = %request_id,
                spoken,
                daemon_roundtrip_ms,
                total_ms = elapsed_ms(total_started_at),
                "action status response received"
            );
            println!("{}", message);
        }
    }

    Ok(())
}

async fn run_open_application(
    config: &AppConfig,
    speak: bool,
    app_name: &str,
    transcript: Option<&str>,
) -> Result<()> {
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();
    let socket_path = config.socket_path()?;

    info!(
        request_id = %request_id,
        transcript,
        app_name,
        speak,
        "open application request started"
    );

    let connect_started_at = Instant::now();
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;
    let connect_ms = elapsed_ms(connect_started_at);

    info!(
        request_id = %request_id,
        connect_ms,
        socket = %socket_path.display(),
        "daemon socket connected"
    );

    let request = VisionRequest::OpenApplication(ApplicationLaunchJob {
        request_id,
        transcript: transcript.map(str::to_string),
        app_name: app_name.to_string(),
        speak,
    });

    let daemon_roundtrip_started_at = Instant::now();
    write_message(&mut stream, &request).await?;
    let response: JobResult = read_message(&mut stream).await?;
    let daemon_roundtrip_ms = elapsed_ms(daemon_roundtrip_started_at);

    match response {
        JobResult::ActionStatus {
            message, spoken, ..
        } => {
            info!(
                request_id = %request_id,
                spoken,
                daemon_roundtrip_ms,
                total_ms = elapsed_ms(total_started_at),
                "open application response received"
            );
            println!("{}", message);
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ClipboardText { .. } | JobResult::BrowserQuery { .. } => {
            anyhow::bail!("daemon returned unexpected response for open application");
        }
    }

    Ok(())
}

async fn run_voice_search(
    config: &AppConfig,
    speak: bool,
    voice_search: &voice::VoiceSearch,
) -> Result<()> {
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();
    let socket_path = config.socket_path()?;

    info!(
        request_id = %request_id,
        transcript = %voice_search.transcript,
        query = %voice_search.query,
        speak,
        "voice search request started"
    );

    let connect_started_at = Instant::now();
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;
    let connect_ms = elapsed_ms(connect_started_at);

    info!(
        request_id = %request_id,
        connect_ms,
        socket = %socket_path.display(),
        "daemon socket connected"
    );

    let request = VisionRequest::VoiceSearch(VoiceSearchJob {
        request_id,
        transcript: voice_search.transcript.clone(),
        query: voice_search.query.clone(),
        speak,
    });

    let daemon_roundtrip_started_at = Instant::now();
    write_message(&mut stream, &request).await?;
    let response: JobResult = read_message(&mut stream).await?;
    let daemon_roundtrip_ms = elapsed_ms(daemon_roundtrip_started_at);

    match response {
        JobResult::BrowserQuery {
            query,
            summary,
            spoken,
            ..
        } => {
            info!(
                request_id = %request_id,
                spoken,
                daemon_roundtrip_ms,
                total_ms = elapsed_ms(total_started_at),
                "voice browser query response received"
            );
            println!("Consulta por voz aberta no navegador: {}", query);
            if let Some(summary) = summary {
                println!("\nResumo inicial da pesquisa:\n{}", summary);
            }
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ClipboardText { .. } => {
            anyhow::bail!("daemon returned unexpected clipboard response for voice search");
        }
        JobResult::ActionStatus { .. } => {
            anyhow::bail!("daemon returned unexpected action status for voice search");
        }
    }

    Ok(())
}

fn detect_session_type() -> SessionType {
    match std::env::var("XDG_SESSION_TYPE") {
        Ok(value) if value.eq_ignore_ascii_case("wayland") => SessionType::Wayland,
        Ok(value) if value.eq_ignore_ascii_case("x11") => SessionType::X11,
        _ => SessionType::Unknown,
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
}
