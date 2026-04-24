mod capture;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::UnixStream;
use tracing::info;
use uuid::Uuid;
use visionclip_common::{
    read_message, write_message, Action, AppConfig, CaptureJob, JobResult, SessionType,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| config.general.log_level.clone()),
        )
        .init();

    let action_string = cli
        .action
        .clone()
        .unwrap_or_else(|| config.general.default_action.clone());
    let action: Action = action_string.parse().map_err(anyhow::Error::msg)?;
    let session_type = detect_session_type();
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();

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

    let daemon_roundtrip_started_at = Instant::now();
    write_message(&mut stream, &job).await?;
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
