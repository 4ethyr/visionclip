mod capture;
mod doctor;
mod search_overlay;
mod voice;
mod voice_overlay;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;
use tokio::{
    net::UnixStream,
    process::Command,
    time::{sleep, Duration},
};
use tracing::{debug, info, warn};
use uuid::Uuid;
use visionclip_common::{
    read_message, write_assistant_status, write_message, AppConfig, ApplicationLaunchJob,
    AssistantLanguage, AssistantStatusKind, CaptureJob, DocumentAskJob, DocumentControlJob,
    DocumentControlKind, DocumentIngestJob, DocumentOpenJob, DocumentReadJob, DocumentSummarizeJob,
    DocumentTranslateJob, JobResult, SearchControlRequest, SearchHit, SearchMode, SearchRequest,
    SearchResponse, SessionType, UrlOpenJob, VisionRequest, VoiceSearchJob,
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
    voice_agent_dry_run: bool,

    #[arg(long, default_value_t = false)]
    wake_listener: bool,

    #[arg(long, hide = true, default_value_t = false)]
    wake_listener_once: bool,

    #[arg(long, default_value_t = false)]
    doctor: bool,

    #[arg(long, default_value_t = false)]
    voice_request: bool,

    #[arg(long, default_value_t = false)]
    voice_search: bool,

    #[arg(long)]
    voice_transcript: Option<String>,

    #[arg(long, default_value_t = false)]
    stop_speaking: bool,

    #[arg(long, hide = true, default_value_t = false)]
    voice_overlay_listening: bool,

    #[arg(long, hide = true, default_value_t = 4000)]
    voice_overlay_duration_ms: u64,

    #[arg(long, default_value_t = false)]
    search_overlay: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Document(DocumentCommand),
    #[command(subcommand)]
    Voice(VoiceCommand),
    Search(SearchCommand),
    Locate(LocateCommand),
    Grep(GrepCommand),
    #[command(subcommand)]
    Index(IndexCommand),
}

#[derive(Debug, Args)]
struct SearchCommand {
    query: String,
    #[arg(long, default_value_t = false)]
    semantic: bool,
    #[arg(long, default_value_t = false)]
    hybrid: bool,
    #[arg(long, default_value_t = 10)]
    limit: u16,
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct LocateCommand {
    filename: String,
    #[arg(long, default_value_t = 10)]
    limit: u16,
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct GrepCommand {
    query: String,
    root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    semantic: bool,
    #[arg(long, default_value_t = 20)]
    limit: u16,
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum IndexCommand {
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add {
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Remove {
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Rebuild {
        root: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Audit {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Pause {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Resume {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum VoiceCommand {
    Enroll {
        #[arg(long, default_value_t = 3)]
        samples: usize,
        #[arg(long, default_value = "default")]
        label: String,
    },
    Status,
    Clear,
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

    if cli.search_overlay {
        return search_overlay::run_search_overlay(&config);
    }

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

    if cli.stop_speaking {
        let interrupted = voice::interrupt_active_tts_playback().await;
        let _ = write_assistant_status(AssistantStatusKind::Idle, None, None);
        if interrupted > 0 {
            println!("Speech stopped.");
        } else {
            println!("No active VisionClip speech playback was found.");
        }
        return Ok(());
    }

    if cli.wake_listener {
        return run_wake_listener(&config, cli.speak, cli.wake_listener_once).await;
    }

    if let Some(command) = &cli.command {
        return run_command(&config, command, cli.speak).await;
    }

    if (cli.action.is_some() || cli.open_app.is_some() || cli.open_url.is_some())
        && (cli.voice_agent
            || cli.voice_agent_dry_run
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

    let resolved_voice_agent = if cli.action.is_none()
        && cli.open_app.is_none()
        && (cli.voice_agent || cli.voice_agent_dry_run)
    {
        Some(
            voice::resolve_voice_agent_command(&config.voice, cli.voice_transcript.as_deref())
                .await?,
        )
    } else {
        None
    };

    if cli.voice_agent_dry_run {
        if let Some(command) = &resolved_voice_agent {
            println!("{}", voice_agent_dry_run_report(command));
            return Ok(());
        }
    }

    if let Some(command) = &resolved_voice_agent {
        return run_voice_agent_command(&config, cli.speak, command).await;
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
        JobResult::Search(_) => {
            anyhow::bail!("daemon returned unexpected search response for capture request");
        }
    }

    Ok(())
}

async fn run_command(config: &AppConfig, command: &Commands, speak: bool) -> Result<()> {
    match command {
        Commands::Document(command) => run_document_command(config, command, speak).await,
        Commands::Voice(command) => run_voice_command(config, command).await,
        Commands::Search(command) => run_search_command(config, command).await,
        Commands::Locate(command) => run_locate_command(config, command).await,
        Commands::Grep(command) => run_grep_command(config, command).await,
        Commands::Index(command) => run_index_command(config, command).await,
    }
}

async fn run_voice_command(config: &AppConfig, command: &VoiceCommand) -> Result<()> {
    let profile_path = config.voice_profile_path()?;
    match command {
        VoiceCommand::Enroll { samples, label } => {
            let profile =
                voice::enroll_speaker_profile(&config.voice, &profile_path, *samples, label)
                    .await?;
            println!(
                "Perfil de voz salvo em {}\nlabel={}\nsamples={}\nthreshold={:.2}",
                profile_path.display(),
                profile.label,
                profile.sample_count,
                profile.threshold
            );
        }
        VoiceCommand::Status => {
            let status = voice::speaker_profile_status(&profile_path)?;
            if status.exists {
                println!(
                    "speaker_profile=ready\npath={}\nlabel={}\nsamples={}\nthreshold={:.2}\ncreated_at_ms={}",
                    status.path.display(),
                    status.label.as_deref().unwrap_or("default"),
                    status.sample_count,
                    status.threshold.unwrap_or_default(),
                    status.created_at_ms.unwrap_or_default()
                );
            } else {
                println!(
                    "speaker_profile=missing\npath={}\nrun=visionclip voice enroll --samples {}",
                    status.path.display(),
                    config.voice.speaker_verification_min_samples
                );
            }
        }
        VoiceCommand::Clear => {
            if voice::clear_speaker_profile(&profile_path)? {
                println!("Perfil de voz removido: {}", profile_path.display());
            } else {
                println!(
                    "Nenhum perfil de voz encontrado em {}",
                    profile_path.display()
                );
            }
        }
    }
    Ok(())
}

async fn run_wake_listener(config: &AppConfig, speak: bool, once: bool) -> Result<()> {
    if !config.voice.wake_word_enabled {
        anyhow::bail!(
            "wake listener is disabled; set [voice].wake_word_enabled = true or run the installer with --enable-wake-listener"
        );
    }

    info!(
        wake_record_duration_ms = config.voice.wake_record_duration_ms,
        wake_idle_sleep_ms = config.voice.wake_idle_sleep_ms,
        "VisionClip wake listener started"
    );

    let voice_profile_path = config.voice_profile_path()?;
    let speaker_gate_ready = config.voice.speaker_verification_enabled
        && voice::speaker_profile_exists(&voice_profile_path);
    if config.voice.speaker_verification_enabled && !speaker_gate_ready {
        warn!(
            path = %voice_profile_path.display(),
            "speaker verification is enabled but no usable voice profile exists; playback blocking remains active"
        );
    }

    let mut playback_blocked = false;
    let mut last_playback_block_log: Option<Instant> = None;
    let mut playback_clear_since: Option<Instant> = None;
    let playback_resume_grace = Duration::from_millis(2_500);

    loop {
        if wake_should_skip_for_playback(config, speaker_gate_ready).await {
            playback_clear_since = None;
            let should_log = last_playback_block_log
                .map(|logged_at| logged_at.elapsed() >= Duration::from_secs(60))
                .unwrap_or(true);
            if should_log {
                warn!(
                    "wake listener paused while system playback is active; passive `Key` is ignored until playback stops"
                );
                last_playback_block_log = Some(Instant::now());
            }
            if !playback_blocked {
                let _ = write_assistant_status(
                    AssistantStatusKind::Idle,
                    Some("wake_blocked_by_playback"),
                    None,
                );
                playback_blocked = true;
            }
            if once {
                break;
            }
            sleep(Duration::from_millis(
                config.voice.wake_idle_sleep_ms.max(1_000),
            ))
            .await;
            continue;
        }

        if playback_blocked {
            let clear_since = playback_clear_since.get_or_insert_with(Instant::now);
            if clear_since.elapsed() < playback_resume_grace {
                if once {
                    break;
                }
                sleep(Duration::from_millis(
                    config.voice.wake_idle_sleep_ms.max(1_000),
                ))
                .await;
                continue;
            }
            warn!("wake listener resumed after system playback stopped");
            playback_blocked = false;
            last_playback_block_log = None;
            playback_clear_since = None;
        }

        let _ = write_assistant_status(AssistantStatusKind::Listening, Some("wake_passive"), None);

        let speaker_profile = speaker_gate_ready.then_some(voice_profile_path.as_path());
        match voice::listen_for_wake_agent_activation(&config.voice, speaker_profile).await {
            Ok(Some(voice::WakeAgentActivation::Command(command))) => {
                let _ = write_assistant_status(
                    AssistantStatusKind::Listening,
                    Some("wake_word_detected"),
                    None,
                );
                if let Err(error) = run_voice_agent_command(config, speak, &command).await {
                    let _ = write_assistant_status(AssistantStatusKind::Idle, None, None);
                    warn!(?error, "wake listener failed to run detected command");
                }
            }
            Ok(Some(voice::WakeAgentActivation::AwaitingCommand { transcript })) => {
                info!(
                    transcript_chars = transcript.chars().count(),
                    "wake word detected; capturing follow-up command"
                );
                let _ = write_assistant_status(
                    AssistantStatusKind::Listening,
                    Some("wake_word_detected"),
                    None,
                );
                match voice::resolve_voice_agent_command(&config.voice, None).await {
                    Ok(command) => {
                        if let Err(error) = run_voice_agent_command(config, speak, &command).await {
                            let _ = write_assistant_status(AssistantStatusKind::Idle, None, None);
                            warn!(?error, "wake listener failed to run follow-up command");
                        }
                    }
                    Err(error) => {
                        let _ = write_assistant_status(AssistantStatusKind::Idle, None, None);
                        warn!(?error, "wake listener could not resolve follow-up command");
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                if is_empty_voice_transcript_error(&error) {
                    debug!("wake listener polling turn did not produce a transcript");
                } else {
                    warn!(?error, "wake listener polling turn failed");
                    let _ = write_assistant_status(AssistantStatusKind::Idle, None, None);
                }
            }
        }

        if once {
            break;
        }

        sleep(Duration::from_millis(
            config.voice.wake_idle_sleep_ms.max(1_000),
        ))
        .await;
    }

    Ok(())
}

async fn wake_should_skip_for_playback(config: &AppConfig, speaker_gate_ready: bool) -> bool {
    if !config.voice.wake_block_during_playback {
        return false;
    }
    if speaker_gate_ready {
        return false;
    }

    match Command::new("pactl")
        .args(["list", "sink-inputs"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            pactl_sink_inputs_have_active_playback(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => {
            debug!(
                status = ?output.status.code(),
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "wake playback gate could not inspect sink inputs; skipping wake polling"
            );
            true
        }
        Err(error) => {
            debug!(
                ?error,
                "wake playback gate could not execute pactl; skipping wake polling"
            );
            true
        }
    }
}

fn pactl_sink_inputs_have_active_playback(output: &str) -> bool {
    output
        .split("\nSink Input #")
        .filter(|block| !block.trim().is_empty())
        .any(|block| {
            let mut saw_playback_status = false;
            for line in block.lines().map(str::trim) {
                if let Some(state) = line.strip_prefix("State:") {
                    saw_playback_status = true;
                    if state.trim().eq_ignore_ascii_case("RUNNING") {
                        return true;
                    }
                }
                if let Some(corked) = line.strip_prefix("Corked:") {
                    saw_playback_status = true;
                    if corked.trim().eq_ignore_ascii_case("no") {
                        return true;
                    }
                }
                if line == "pulse.corked = \"false\"" {
                    return true;
                }
                if line == "pulse.corked = \"true\"" {
                    saw_playback_status = true;
                }
            }

            !saw_playback_status
        })
}

fn is_empty_voice_transcript_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("voice transcript is empty")
        || message.contains("transcript is empty")
        || message.contains("produced no transcript")
        || message.contains("no usable transcript")
        || message.contains("wake audio below speech threshold")
        || message.contains("wake speaker verification rejected sample")
}

async fn run_search_command(config: &AppConfig, command: &SearchCommand) -> Result<()> {
    let mode = if command.hybrid {
        SearchMode::Hybrid
    } else if command.semantic {
        SearchMode::Semantic
    } else {
        SearchMode::Auto
    };
    run_search_request(
        config,
        SearchRequest {
            request_id: Uuid::new_v4().to_string(),
            query: command.query.clone(),
            mode,
            root_hint: None,
            limit: command.limit,
            include_snippets: true,
            include_ocr: true,
            include_semantic: command.semantic || command.hybrid,
        },
        command.json,
    )
    .await
}

async fn run_locate_command(config: &AppConfig, command: &LocateCommand) -> Result<()> {
    run_search_request(
        config,
        SearchRequest {
            request_id: Uuid::new_v4().to_string(),
            query: command.filename.clone(),
            mode: SearchMode::Locate,
            root_hint: None,
            limit: command.limit,
            include_snippets: false,
            include_ocr: false,
            include_semantic: false,
        },
        command.json,
    )
    .await
}

async fn run_grep_command(config: &AppConfig, command: &GrepCommand) -> Result<()> {
    run_search_request(
        config,
        SearchRequest {
            request_id: Uuid::new_v4().to_string(),
            query: command.query.clone(),
            mode: if command.semantic {
                SearchMode::Hybrid
            } else {
                SearchMode::Grep
            },
            root_hint: command.root.as_ref().map(|path| {
                path.canonicalize()
                    .unwrap_or_else(|_| path.clone())
                    .display()
                    .to_string()
            }),
            limit: command.limit,
            include_snippets: true,
            include_ocr: false,
            include_semantic: command.semantic,
        },
        command.json,
    )
    .await
}

async fn run_search_request(
    config: &AppConfig,
    request: SearchRequest,
    json_output: bool,
) -> Result<()> {
    let response = send_request(config, VisionRequest::Search(request)).await?;
    match response {
        JobResult::Search(response) => print_search_response(&response, json_output),
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ActionStatus { .. }
        | JobResult::ClipboardText { .. }
        | JobResult::BrowserQuery { .. }
        | JobResult::DocumentStatus { .. } => {
            anyhow::bail!("daemon returned unexpected response for search request");
        }
    }
}

async fn run_index_command(config: &AppConfig, command: &IndexCommand) -> Result<()> {
    let request_id = Uuid::new_v4().to_string();
    let (request, json_output) = match command {
        IndexCommand::Status { json } => (SearchControlRequest::Status { request_id }, *json),
        IndexCommand::Add { path, json } => (
            SearchControlRequest::AddRoot {
                request_id,
                path: path.display().to_string(),
            },
            *json,
        ),
        IndexCommand::Remove { path, json } => (
            SearchControlRequest::RemoveRoot {
                request_id,
                path: path.display().to_string(),
            },
            *json,
        ),
        IndexCommand::Rebuild { root, json } => (
            SearchControlRequest::Rebuild {
                request_id,
                root: root.as_ref().map(|path| path.display().to_string()),
            },
            *json,
        ),
        IndexCommand::Audit { json } => (SearchControlRequest::Audit { request_id }, *json),
        IndexCommand::Pause { json } => (SearchControlRequest::Pause { request_id }, *json),
        IndexCommand::Resume { json } => (SearchControlRequest::Resume { request_id }, *json),
    };

    let response = send_request(config, VisionRequest::SearchControl(request)).await?;
    match response {
        JobResult::Search(response) => print_search_response(&response, json_output),
        JobResult::ActionStatus { message, .. } => {
            if json_output {
                println!("{}", serde_json::json!({ "message": message }));
            } else {
                println!("{}", message);
            }
            Ok(())
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ClipboardText { .. }
        | JobResult::BrowserQuery { .. }
        | JobResult::DocumentStatus { .. } => {
            anyhow::bail!("daemon returned unexpected response for index command");
        }
    }
}

fn print_search_response(response: &SearchResponse, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }

    if let Some(message) = response
        .diagnostics
        .as_ref()
        .and_then(|diagnostics| diagnostics.message.as_deref())
    {
        println!("{}", message);
    }

    if response.hits.is_empty() {
        if response
            .diagnostics
            .as_ref()
            .and_then(|diagnostics| diagnostics.message.as_ref())
            .is_none()
        {
            println!("No local results.");
        }
        return Ok(());
    }

    for (index, hit) in response.hits.iter().enumerate() {
        print_search_hit(index + 1, hit);
    }
    Ok(())
}

fn print_search_hit(index: usize, hit: &SearchHit) {
    let size = hit
        .size_bytes
        .map(format_size)
        .unwrap_or_else(|| "-".to_string());
    println!(
        "{index}. {}  [{} | score {:.1} | {}]",
        hit.title, hit.kind, hit.score, size
    );
    println!("   {}", hit.path);
    if let Some(snippet) = &hit.snippet {
        println!("   {}", snippet);
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1}GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes / KB)
    } else {
        format!("{}B", bytes as u64)
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
        JobResult::Search(_) => {
            anyhow::bail!("daemon returned unexpected search response for document command");
        }
    }

    Ok(())
}

async fn run_voice_agent_command(
    config: &AppConfig,
    speak: bool,
    command: &voice::VoiceAgentCommand,
) -> Result<()> {
    match command {
        voice::VoiceAgentCommand::OpenApplication {
            transcript,
            language,
            app_name,
        } => run_open_application(config, speak, app_name, Some(transcript), Some(*language)).await,
        voice::VoiceAgentCommand::OpenUrl {
            transcript,
            language,
            label,
            url,
        } => run_open_url(config, speak, label, url, Some(transcript), Some(*language)).await,
        voice::VoiceAgentCommand::OpenDocument {
            transcript,
            language,
            query,
        } => run_open_document(config, speak, query, Some(transcript), Some(*language)).await,
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
            run_voice_search(config, speak, &voice_search).await
        }
    }
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

fn voice_agent_dry_run_report(command: &voice::VoiceAgentCommand) -> String {
    match command {
        voice::VoiceAgentCommand::OpenApplication {
            transcript,
            language,
            app_name,
        } => format!(
            "voice_agent_dry_run\nintent=open_application\nlanguage={}\ntranscript={}\napp_name={}",
            language.tts_language_code(),
            transcript,
            app_name
        ),
        voice::VoiceAgentCommand::OpenDocument {
            transcript,
            language,
            query,
        } => format!(
            "voice_agent_dry_run\nintent=open_document\nlanguage={}\ntranscript={}\nquery={}",
            language.tts_language_code(),
            transcript,
            query
        ),
        voice::VoiceAgentCommand::OpenUrl {
            transcript,
            language,
            label,
            url,
        } => format!(
            "voice_agent_dry_run\nintent=open_url\nlanguage={}\ntranscript={}\nlabel={}\nurl={}",
            language.tts_language_code(),
            transcript,
            label,
            url
        ),
        voice::VoiceAgentCommand::SearchWeb {
            transcript,
            language,
            query,
        } => format!(
            "voice_agent_dry_run\nintent=search_web\nlanguage={}\ntranscript={}\nquery={}",
            language.tts_language_code(),
            transcript,
            query
        ),
    }
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
        | JobResult::DocumentStatus { .. }
        | JobResult::Search(_) => {
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
        | JobResult::DocumentStatus { .. }
        | JobResult::Search(_) => {
            anyhow::bail!("daemon returned unexpected response for open url");
        }
    }

    Ok(())
}

async fn run_open_document(
    config: &AppConfig,
    speak: bool,
    query: &str,
    transcript: Option<&str>,
    input_language: Option<AssistantLanguage>,
) -> Result<()> {
    let request_id = Uuid::new_v4();
    let total_started_at = Instant::now();
    let socket_path = config.socket_path()?;

    info!(
        request_id = %request_id,
        query,
        transcript = ?transcript,
        input_language = input_language.map(|language| language.tts_language_code()),
        speak,
        "open document request started"
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

    let request = VisionRequest::OpenDocument(DocumentOpenJob {
        request_id,
        transcript: transcript.map(str::to_string),
        input_language: input_language.or_else(|| transcript.map(AssistantLanguage::detect)),
        query: query.to_string(),
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
                "open document response received"
            );
            println!("{}", message);
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ClipboardText { .. }
        | JobResult::BrowserQuery { .. }
        | JobResult::DocumentStatus { .. }
        | JobResult::Search(_) => {
            anyhow::bail!("daemon returned unexpected response for open document");
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
            println!(
                "{}",
                localized_voice_search_opened_message(voice_search.language, &query)
            );
            if let Some(summary) = summary {
                println!(
                    "\n{}:\n{}",
                    localized_voice_search_summary_heading(voice_search.language),
                    summary
                );
            }
        }
        JobResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}");
        }
        JobResult::ClipboardText { .. } => {
            anyhow::bail!("daemon returned unexpected clipboard response for voice search");
        }
        JobResult::ActionStatus { .. }
        | JobResult::DocumentStatus { .. }
        | JobResult::Search(_) => {
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

fn localized_voice_search_opened_message(language: AssistantLanguage, query: &str) -> String {
    match language {
        AssistantLanguage::PortugueseBrazil => {
            format!("Consulta por voz aberta no navegador: {query}")
        }
        AssistantLanguage::English => format!("Voice search opened in the browser: {query}"),
        AssistantLanguage::Chinese => format!("已在浏览器中打开语音搜索：{query}"),
        AssistantLanguage::Spanish => format!("Búsqueda por voz abierta en el navegador: {query}"),
        AssistantLanguage::Russian => format!("Голосовой поиск открыт в браузере: {query}"),
        AssistantLanguage::Japanese => format!("音声検索をブラウザで開きました: {query}"),
        AssistantLanguage::Korean => format!("음성 검색을 브라우저에서 열었습니다: {query}"),
        AssistantLanguage::Hindi => format!("वॉइस खोज ब्राउज़र में खोल दी गई: {query}"),
    }
}

fn localized_voice_search_summary_heading(language: AssistantLanguage) -> &'static str {
    match language {
        AssistantLanguage::PortugueseBrazil => "Resumo inicial da pesquisa",
        AssistantLanguage::English => "Initial search summary",
        AssistantLanguage::Chinese => "初始搜索摘要",
        AssistantLanguage::Spanish => "Resumen inicial de la búsqueda",
        AssistantLanguage::Russian => "Первичное резюме поиска",
        AssistantLanguage::Japanese => "検索の初期要約",
        AssistantLanguage::Korean => "초기 검색 요약",
        AssistantLanguage::Hindi => "प्रारंभिक खोज सारांश",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localizes_voice_search_cli_messages() {
        assert_eq!(
            localized_voice_search_opened_message(AssistantLanguage::English, "Rust async"),
            "Voice search opened in the browser: Rust async"
        );
        assert_eq!(
            localized_voice_search_summary_heading(AssistantLanguage::Chinese),
            "初始搜索摘要"
        );
        assert_eq!(
            localized_voice_search_opened_message(AssistantLanguage::PortugueseBrazil, "Rust"),
            "Consulta por voz aberta no navegador: Rust"
        );
    }

    #[test]
    fn voice_agent_dry_run_reports_intent_language_and_slots() {
        let report = voice_agent_dry_run_report(&voice::VoiceAgentCommand::OpenDocument {
            transcript: "Open the book, Programming TypeScript.".into(),
            language: AssistantLanguage::English,
            query: "Programming TypeScript".into(),
        });

        assert!(report.contains("intent=open_document"));
        assert!(report.contains("language=en"));
        assert!(report.contains("query=Programming TypeScript"));

        let report = voice_agent_dry_run_report(&voice::VoiceAgentCommand::OpenUrl {
            transcript: "打开油管".into(),
            language: AssistantLanguage::Chinese,
            label: "YouTube".into(),
            url: "https://www.youtube.com/".into(),
        });

        assert!(report.contains("intent=open_url"));
        assert!(report.contains("language=zh"));
        assert!(report.contains("label=YouTube"));
    }

    #[test]
    fn detects_active_pactl_sink_input_for_wake_gate() {
        let output = r#"
Sink Input #93
    Driver: PipeWire
    Owner Module: n/a
    State: RUNNING
    Properties:
        application.name = "Firefox"
"#;

        assert!(pactl_sink_inputs_have_active_playback(output));
    }

    #[test]
    fn detects_pipewire_corked_no_sink_input_for_wake_gate() {
        let output = r#"
Sink Input #3836
    Driver: PipeWire
    Corked: no
    Properties:
        application.name = "Google Chrome"
        pulse.corked = "false"
"#;

        assert!(pactl_sink_inputs_have_active_playback(output));
    }

    #[test]
    fn ignores_inactive_pactl_sink_input_for_wake_gate() {
        let output = r#"
Sink Input #94
    Driver: PipeWire
    State: CORKED
    Properties:
        application.name = "VisionClip"
"#;

        assert!(!pactl_sink_inputs_have_active_playback(output));
        assert!(!pactl_sink_inputs_have_active_playback(""));
    }

    #[test]
    fn ignores_pipewire_corked_yes_sink_input_for_wake_gate() {
        let output = r#"
Sink Input #96
    Driver: PipeWire
    Corked: yes
    Properties:
        application.name = "Paused Player"
        pulse.corked = "true"
"#;

        assert!(!pactl_sink_inputs_have_active_playback(output));
    }

    #[test]
    fn treats_unknown_pactl_sink_input_state_as_active_for_wake_gate() {
        let output = r#"
Sink Input #95
    Driver: PipeWire
    Properties:
        application.name = "Unknown Player"
"#;

        assert!(pactl_sink_inputs_have_active_playback(output));
    }
}
