mod capture;
mod doctor;
mod voice;
mod voice_overlay;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::UnixStream;
use tracing::{info, warn};
use uuid::Uuid;
use visionclip_common::{
    read_message, write_message, AppConfig, ApplicationLaunchJob, AssistantLanguage, CaptureJob,
    DocumentAskJob, DocumentControlJob, DocumentControlKind, DocumentIngestJob, DocumentReadJob,
    DocumentSummarizeJob, DocumentTranslateJob, JobResult, SessionType, UrlOpenJob, VisionRequest,
    VoiceSearchJob,
};

#[derive(Debug, Parser)]
#[command(name = "visionclip")]
#[command(about = "CLI do VisionClip para enviar capturas ao daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

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

    #[arg(long)]
    open_url: Option<String>,

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

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Document(DocumentCommand),
}

#[derive(Debug, Subcommand)]
enum DocumentCommand {
    Ingest {
        path: PathBuf,
    },
    Translate {
        document_id: String,
        #[arg(long, default_value = "pt-BR")]
        target_lang: String,
    },
    Read {
        document_id: String,
        #[arg(long, default_value = "pt-BR")]
        target_lang: String,
    },
    Ask {
        document_id: String,
        question: String,
    },
    Summarize {
        document_id: String,
    },
    Pause {
        reading_session_id: String,
    },
    Resume {
        reading_session_id: String,
    },
    Stop {
        reading_session_id: String,
    },
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

    if let Some(command) = &cli.command {
        return run_command(&config, command, cli.speak).await;
    }

    if (cli.action.is_some() || cli.open_app.is_some() || cli.open_url.is_some())
        && (cli.voice_agent
            || cli.voice_request
            || cli.voice_search
            || cli.voice_transcript.is_some())
    {
        warn!("voice inputs were ignored because an explicit action was provided");
    }

    if let Some(app_name) = &cli.open_app {
        return run_open_application(&config, cli.speak, app_name, None, None).await;
    }

    if let Some(url) = &cli.open_url {
        return run_open_url(&config, cli.speak, url, url, None, None).await;
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
                language,
                app_name,
            } => {
                return run_open_application(
                    &config,
                    cli.speak,
                    app_name,
                    Some(transcript),
                    Some(*language),
                )
                .await;
            }
            voice::VoiceAgentCommand::OpenUrl {
                transcript,
                language,
                label,
                url,
            } => {
                return run_open_url(
                    &config,
                    cli.speak,
                    label,
                    url,
                    Some(transcript),
                    Some(*language),
                )
                .await;
            }
            voice::VoiceAgentCommand::SearchWeb {
                transcript,
                language,
                query,
            } => {
                let voice_search = voice::VoiceSearch {
                    transcript: transcript.clone(),
                    language: *language,
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
            input_language = voice_request.language.tts_language_code(),
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
        transcript: resolved_voice_request
            .as_ref()
            .map(|voice_request| voice_request.transcript.clone()),
        input_language: resolved_voice_request
            .as_ref()
            .map(|voice_request| voice_request.language),
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
        JobResult::DocumentStatus { .. } => {
            anyhow::bail!("daemon returned unexpected document response for capture request");
        }
    }

    Ok(())
}

async fn run_command(config: &AppConfig, command: &Commands, speak: bool) -> Result<()> {
    match command {
        Commands::Document(command) => run_document_command(config, command, speak).await,
    }
}

async fn run_document_command(
    config: &AppConfig,
    command: &DocumentCommand,
    speak: bool,
) -> Result<()> {
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();
    let request = match command {
        DocumentCommand::Ingest { path } => VisionRequest::DocumentIngest(DocumentIngestJob {
            request_id,
            path: path.clone(),
        }),
        DocumentCommand::Translate {
            document_id,
            target_lang,
        } => VisionRequest::DocumentTranslate(DocumentTranslateJob {
            request_id,
            document_id: document_id.clone(),
            target_language: target_lang.clone(),
        }),
        DocumentCommand::Read {
            document_id,
            target_lang,
        } => VisionRequest::DocumentRead(DocumentReadJob {
            request_id,
            document_id: document_id.clone(),
            target_language: target_lang.clone(),
        }),
        DocumentCommand::Ask {
            document_id,
            question,
        } => VisionRequest::DocumentAsk(DocumentAskJob {
            request_id,
            document_id: document_id.clone(),
            question: question.clone(),
            speak,
        }),
        DocumentCommand::Summarize { document_id } => {
            VisionRequest::DocumentSummarize(DocumentSummarizeJob {
                request_id,
                document_id: document_id.clone(),
                speak,
            })
        }
        DocumentCommand::Pause { reading_session_id } => {
            VisionRequest::DocumentControl(DocumentControlJob {
                request_id,
                reading_session_id: reading_session_id.clone(),
                control: DocumentControlKind::Pause,
            })
        }
        DocumentCommand::Resume { reading_session_id } => {
            VisionRequest::DocumentControl(DocumentControlJob {
                request_id,
                reading_session_id: reading_session_id.clone(),
                control: DocumentControlKind::Resume,
            })
        }
        DocumentCommand::Stop { reading_session_id } => {
            VisionRequest::DocumentControl(DocumentControlJob {
                request_id,
                reading_session_id: reading_session_id.clone(),
                control: DocumentControlKind::Stop,
            })
        }
    };

    info!(request_id = %request_id, "document command request started");
    let response = send_request(config, request).await?;

    match response {
        JobResult::DocumentStatus {
            document_id,
            reading_session_id,
            chunks,
            message,
            spoken,
            ..
        } => {
            info!(
                request_id = %request_id,
                spoken,
                total_ms = elapsed_ms(total_started_at),
                "document command response received"
            );
            println!("{}", message);
            if let Some(document_id) = document_id {
                println!("document_id: {}", document_id);
            }
            if let Some(reading_session_id) = reading_session_id {
                println!("reading_session_id: {}", reading_session_id);
            }
            if let Some(chunks) = chunks {
                println!("chunks: {}", chunks);
            }
        }
        JobResult::ClipboardText { text, spoken, .. } => {
            info!(
                request_id = %request_id,
                spoken,
                total_ms = elapsed_ms(total_started_at),
                "document text response received"
            );
            println!("Resultado copiado para o clipboard:\n{}", text);
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ActionStatus { message, .. } => {
            println!("{}", message);
        }
        JobResult::BrowserQuery { .. } => {
            anyhow::bail!("daemon returned unexpected browser query for document command");
        }
    }

    Ok(())
}

async fn send_request(config: &AppConfig, request: VisionRequest) -> Result<JobResult> {
    let socket_path = config.socket_path()?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;
    write_message(&mut stream, &request).await?;
    Ok(read_message(&mut stream).await?)
}

async fn run_open_application(
    config: &AppConfig,
    speak: bool,
    app_name: &str,
    transcript: Option<&str>,
    input_language: Option<AssistantLanguage>,
) -> Result<()> {
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();
    let socket_path = config.socket_path()?;

    info!(
        request_id = %request_id,
        transcript,
        input_language = input_language.map(|language| language.tts_language_code()),
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
        input_language: input_language.or_else(|| transcript.map(AssistantLanguage::detect)),
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
        JobResult::ClipboardText { .. }
        | JobResult::BrowserQuery { .. }
        | JobResult::DocumentStatus { .. } => {
            anyhow::bail!("daemon returned unexpected response for open application");
        }
    }

    Ok(())
}

async fn run_open_url(
    config: &AppConfig,
    speak: bool,
    label: &str,
    url: &str,
    transcript: Option<&str>,
    input_language: Option<AssistantLanguage>,
) -> Result<()> {
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();
    let socket_path = config.socket_path()?;

    info!(
        request_id = %request_id,
        transcript,
        input_language = input_language.map(|language| language.tts_language_code()),
        label,
        url,
        speak,
        "open url request started"
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

    let request = VisionRequest::OpenUrl(UrlOpenJob {
        request_id,
        transcript: transcript.map(str::to_string),
        input_language: input_language.or_else(|| transcript.map(AssistantLanguage::detect)),
        label: label.to_string(),
        url: url.to_string(),
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
                "open url response received"
            );
            println!("{}", message);
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ClipboardText { .. }
        | JobResult::BrowserQuery { .. }
        | JobResult::DocumentStatus { .. } => {
            anyhow::bail!("daemon returned unexpected response for open url");
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
        input_language = voice_search.language.tts_language_code(),
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
        input_language: Some(voice_search.language),
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
        JobResult::ActionStatus { .. } | JobResult::DocumentStatus { .. } => {
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
