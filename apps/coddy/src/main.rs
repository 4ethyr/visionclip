mod shortcut;
mod voice_overlay;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{env, ffi::OsString, process::Stdio};
use tokio::net::UnixStream;
use tokio::process::Command as TokioCommand;
use tracing::{info, warn};
use uuid::Uuid;
use visionclip_common::{
    read_message, write_message, AppConfig, ContextPolicy, JobResult, ReplCommand, ReplCommandJob,
    ReplEventsJob, ReplSessionSnapshotJob, VisionRequest,
};

#[derive(Debug, Parser)]
#[command(name = "coddy")]
#[command(about = "Backend CLI do Coddy REPL")]
struct Cli {
    #[arg(long, global = true, default_value_t = false)]
    speak: bool,

    #[arg(long, hide = true, default_value_t = false)]
    voice_overlay_listening: bool,

    #[arg(long, hide = true, default_value_t = 4000)]
    voice_overlay_duration_ms: u64,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ask {
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    Voice {
        #[arg(long)]
        transcript: Option<String>,

        #[arg(long, default_value_t = false)]
        overlay: bool,
    },
    Shortcuts {
        #[command(subcommand)]
        command: ShortcutCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ShortcutCommand {
    Test,
    Install {
        #[arg(long, default_value = "Shift+CapsLk")]
        binding: String,

        #[arg(long)]
        coddy_bin: Option<std::path::PathBuf>,

        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    Snapshot,
    Events {
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
}

#[derive(Debug, Subcommand)]
enum DoctorCommand {
    Shortcuts,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.voice_overlay_listening {
        return voice_overlay::run_listening_overlay(cli.voice_overlay_duration_ms);
    }

    let config = AppConfig::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| config.general.log_level.clone()))
        .init();

    match cli.command {
        Some(Command::Ask { text }) => {
            let text = join_command_text(text);
            let result = send_repl_command(
                &config,
                ReplCommand::Ask {
                    text,
                    context_policy: ContextPolicy::NoScreen,
                },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::Voice {
            transcript,
            overlay,
        }) => {
            let (_lock, _overlay) = if overlay {
                (
                    Some(acquire_voice_shortcut_lock(&config)?),
                    start_listening_overlay(config.voice.record_duration_ms),
                )
            } else {
                (None, None)
            };
            let transcript = match normalize_transcript_override(transcript) {
                Some(transcript) => transcript,
                None => visionclip_voice_input::capture_and_transcribe(&config.voice).await?,
            };
            info!(
                chars = transcript.chars().count(),
                "Coddy voice transcript resolved"
            );
            let result = send_repl_command(
                &config,
                ReplCommand::VoiceTurn {
                    transcript_override: Some(transcript),
                },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::Shortcuts {
            command: ShortcutCommand::Test,
        }) => run_shortcuts_test(&config).await,
        Some(Command::Shortcuts {
            command:
                ShortcutCommand::Install {
                    binding,
                    coddy_bin,
                    dry_run,
                },
        }) => run_shortcuts_install(binding, coddy_bin, dry_run),
        Some(Command::Session {
            command: SessionCommand::Snapshot,
        }) => run_session_snapshot(&config).await,
        Some(Command::Session {
            command: SessionCommand::Events { after },
        }) => run_session_events(&config, after).await,
        Some(Command::Doctor {
            command: DoctorCommand::Shortcuts,
        }) => run_shortcuts_doctor(&config).await,
        None => {
            println!("Use `coddy ask`, `coddy voice`, `coddy session snapshot`, `coddy shortcuts test` ou `coddy doctor shortcuts`.");
            Ok(())
        }
    }
}

async fn send_repl_command(
    config: &AppConfig,
    command: ReplCommand,
    speak: bool,
) -> Result<JobResult> {
    let request_id = Uuid::new_v4();
    let socket_path = config.socket_path()?;

    info!(
        request_id = %request_id,
        socket = %socket_path.display(),
        ?command,
        speak,
        "sending Coddy REPL command"
    );

    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;
    let request = VisionRequest::ReplCommand(ReplCommandJob {
        request_id,
        command,
        speak,
    });

    write_message(&mut stream, &request).await?;
    Ok(read_message(&mut stream).await?)
}

async fn run_shortcuts_doctor(config: &AppConfig) -> Result<()> {
    let environment = shortcut::ShortcutEnvironment::detect(config)?;
    print!("{environment}");
    let status = shortcut::GnomeShortcutStatus::detect(&shortcut::default_wrapper_path()?);
    print!("{status}");
    environment.validate_for_shortcut()?;
    Ok(())
}

async fn run_shortcuts_test(config: &AppConfig) -> Result<()> {
    let environment = shortcut::ShortcutEnvironment::detect(config)?;
    print!("{environment}");
    environment.validate_for_shortcut()?;
    let lock = shortcut::VoiceShortcutLock::acquire(environment.lock_path()?)?;
    println!("lock_acquired: {}", lock.path().display());

    let result = send_repl_command(config, ReplCommand::StopSpeaking, false).await?;
    print_job_result(result)?;
    println!("shortcut_test: ok");
    Ok(())
}

async fn run_session_snapshot(config: &AppConfig) -> Result<()> {
    let request_id = Uuid::new_v4();
    let socket_path = config.socket_path()?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;

    write_message(
        &mut stream,
        &VisionRequest::ReplSessionSnapshot(ReplSessionSnapshotJob { request_id }),
    )
    .await?;

    match read_message(&mut stream).await? {
        JobResult::ReplSessionSnapshot { snapshot, .. } => {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            Ok(())
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}")
        }
        _ => anyhow::bail!("daemon returned unexpected response for REPL session snapshot"),
    }
}

async fn run_session_events(config: &AppConfig, after_sequence: u64) -> Result<()> {
    let request_id = Uuid::new_v4();
    let socket_path = config.socket_path()?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;

    write_message(
        &mut stream,
        &VisionRequest::ReplEvents(ReplEventsJob {
            request_id,
            after_sequence,
        }),
    )
    .await?;

    match read_message(&mut stream).await? {
        JobResult::ReplEvents {
            events,
            last_sequence,
            ..
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "last_sequence": last_sequence,
                    "events": events,
                }))?
            );
            Ok(())
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}")
        }
        _ => anyhow::bail!("daemon returned unexpected response for REPL session events"),
    }
}

fn acquire_voice_shortcut_lock(config: &AppConfig) -> Result<shortcut::VoiceShortcutLock> {
    let environment = shortcut::ShortcutEnvironment::detect(config)?;
    environment.validate_for_shortcut()?;
    shortcut::VoiceShortcutLock::acquire(environment.lock_path()?)
}

fn start_listening_overlay(duration_ms: u64) -> Option<OverlayGuard> {
    if !voice_overlay::is_overlay_available() {
        warn!("coddy was built without the `gtk-overlay` feature; skipping voice overlay");
        return None;
    }
    if env::var_os("WAYLAND_DISPLAY").is_none() && env::var_os("DISPLAY").is_none() {
        warn!("no graphical display available; skipping voice overlay");
        return None;
    }

    let current_exe = env::current_exe().ok()?;
    let mut child = TokioCommand::new(current_exe);
    child
        .args(overlay_cli_args(duration_ms))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    match child.spawn() {
        Ok(child) => Some(OverlayGuard { child: Some(child) }),
        Err(error) => {
            warn!(?error, "failed to spawn Coddy listening overlay");
            None
        }
    }
}

fn overlay_cli_args(duration_ms: u64) -> Vec<OsString> {
    vec![
        OsString::from("--voice-overlay-listening"),
        OsString::from("--voice-overlay-duration-ms"),
        OsString::from(duration_ms.max(300).to_string()),
    ]
}

struct OverlayGuard {
    child: Option<tokio::process::Child>,
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}

fn run_shortcuts_install(
    binding: String,
    coddy_bin: Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let coddy_bin = match coddy_bin {
        Some(path) => path,
        None => env::current_exe().context("failed to resolve current coddy binary")?,
    };
    let plan = shortcut::ShortcutInstallPlan::new(binding, coddy_bin)?;

    shortcut::install_gnome_shortcut(&plan, dry_run)?;

    println!("Coddy shortcut configured.");
    println!("Binding: {}", plan.resolved_binding);
    println!("Command: {}", plan.wrapper_path.display());
    if dry_run {
        println!("Dry-run: no files or GNOME settings were changed.");
    }
    Ok(())
}

fn print_job_result(result: JobResult) -> Result<()> {
    match result {
        JobResult::ClipboardText { text, .. } => {
            println!("{text}");
            Ok(())
        }
        JobResult::BrowserQuery { query, summary, .. } => {
            println!("Pesquisa: {query}");
            if let Some(summary) = summary {
                println!("\n{summary}");
            }
            Ok(())
        }
        JobResult::ActionStatus { message, .. } => {
            println!("{message}");
            Ok(())
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}")
        }
        JobResult::ReplSessionSnapshot { snapshot, .. } => {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            Ok(())
        }
        JobResult::ReplEvents {
            events,
            last_sequence,
            ..
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "last_sequence": last_sequence,
                    "events": events,
                }))?
            );
            Ok(())
        }
    }
}

fn join_command_text(text: Vec<String>) -> String {
    text.join(" ").trim().to_string()
}

fn normalize_transcript_override(transcript: Option<String>) -> Option<String> {
    transcript
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_trailing_text_arguments() {
        assert_eq!(
            join_command_text(vec!["quem".into(), "foi".into(), "rousseau?".into()]),
            "quem foi rousseau?"
        );
    }

    #[test]
    fn overlay_cli_args_match_hidden_overlay_command() {
        assert_eq!(
            overlay_cli_args(250).into_iter().collect::<Vec<_>>(),
            vec![
                OsString::from("--voice-overlay-listening"),
                OsString::from("--voice-overlay-duration-ms"),
                OsString::from("300"),
            ]
        );
    }

    #[test]
    fn empty_voice_transcript_override_is_ignored() {
        assert_eq!(normalize_transcript_override(Some("  ".into())), None);
        assert_eq!(
            normalize_transcript_override(Some("  Quem foi Rousseau? ".into())),
            Some("Quem foi Rousseau?".into())
        );
    }
}
