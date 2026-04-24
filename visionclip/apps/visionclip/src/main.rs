use anyhow::{Context, Result};
use clap::Parser;
use std::{fs, path::PathBuf, process::Command};
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

    let image_bytes = if let Some(path) = cli.image.as_ref() {
        fs::read(path).with_context(|| format!("failed to read image at {}", path.display()))?
    } else if let Some(command) = cli.capture_command.as_ref() {
        capture_from_command(command)?
    } else {
        anyhow::bail!("provide --image <path> or --capture-command <command>");
    };

    let socket_path = config.socket_path()?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;

    let job = CaptureJob {
        request_id: Uuid::new_v4(),
        action,
        mime_type: "image/png".to_string(),
        image_bytes,
        session_type: detect_session_type(),
        speak: cli.speak,
        source_app: cli.source_app,
    };

    write_message(&mut stream, &job).await?;
    let response: JobResult = read_message(&mut stream).await?;

    match response {
        JobResult::ClipboardText { text, spoken, .. } => {
            info!(spoken, "clipboard response received");
            println!("Resultado copiado para o clipboard:\n{}", text);
        }
        JobResult::BrowserQuery { query, .. } => {
            println!("Consulta aberta no navegador: {}", query);
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
    }

    Ok(())
}

fn capture_from_command(command: &str) -> Result<Vec<u8>> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .with_context(|| format!("failed to run capture command `{command}`"))?;

    if !output.status.success() {
        anyhow::bail!(
            "capture command failed with status {}",
            output.status.code().unwrap_or_default()
        );
    }

    Ok(output.stdout)
}

fn detect_session_type() -> SessionType {
    match std::env::var("XDG_SESSION_TYPE") {
        Ok(value) if value.eq_ignore_ascii_case("wayland") => SessionType::Wayland,
        Ok(value) if value.eq_ignore_ascii_case("x11") => SessionType::X11,
        _ => SessionType::Unknown,
    }
}
