mod linux_apps;
mod rendered_search;
mod search;

use crate::linux_apps::open_application;
use crate::search::{GoogleSearchClient, SearchEnrichment};
use anyhow::{Context, Result};
use coddy_ipc::{
    decode_payload, decode_wire_request_payload, read_frame_payload, write_frame, CoddyRequest,
    CoddyResult, CoddyWireResult, ReplCommandJob, ReplEventStreamJob, ReplEventsJob,
    ReplSessionSnapshotJob,
};
use std::{
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use visionclip_common::{
    resolve_voice_turn_intent, write_message, Action, AppConfig, ApplicationLaunchJob, CaptureJob,
    HealthCheckJob, JobResult, ModelRef, ReplCommand, ReplEvent, ReplEventBroker,
    ReplEventEnvelope, ReplEventSubscription, ReplIntent, ReplMessage, ReplMode, ReplSession,
    ToolStatus, UrlOpenJob, VisionRequest, VoiceSearchJob, VoiceTurnIntent,
};
use visionclip_infer::{
    postprocess::{sanitize_for_speech, sanitize_output},
    InferenceBackend, InferenceInput, OllamaBackend,
};
use visionclip_output::{notify, open_search_query, open_url, ClipboardOwner};
use visionclip_tts::PiperHttpClient;

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::load()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| config.general.log_level.clone()),
        )
        .init();

    let socket_path = config.socket_path()?;
    cleanup_existing_socket(&socket_path)?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind socket at {}", socket_path.display()))?;

    info!(socket = %socket_path.display(), "visionclip-daemon listening");

    let state = Arc::new(AppState {
        config: config.clone(),
        clipboard: ClipboardOwner::new().context("failed to initialize clipboard owner")?,
        infer: OllamaBackend::new(config.infer.clone()),
        search: if config.search.enabled {
            Some(GoogleSearchClient::new(config.search.clone())?)
        } else {
            None
        },
        piper: if config.audio.enabled {
            Some(PiperHttpClient::new(config.audio.clone()))
        } else {
            None
        },
        tts_gate: TtsPlaybackGate::default(),
        repl: Mutex::new(ReplRuntimeState::new(&config)),
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, state).await {
                error!(?error, "failed to process launcher request");
            }
        });
    }
}

struct AppState {
    config: AppConfig,
    clipboard: ClipboardOwner,
    infer: OllamaBackend,
    search: Option<GoogleSearchClient>,
    piper: Option<PiperHttpClient>,
    tts_gate: TtsPlaybackGate,
    repl: Mutex<ReplRuntimeState>,
}

struct ReplRuntimeState {
    session: ReplSession,
    events: ReplEventBroker,
}

impl ReplRuntimeState {
    fn new(config: &AppConfig) -> Self {
        let session = ReplSession::new(
            ReplMode::FloatingTerminal,
            ModelRef {
                provider: config.infer.backend.clone(),
                name: config.infer.model.clone(),
            },
        );
        let mut events = ReplEventBroker::new(session.id, 256);
        events.publish(
            ReplEvent::SessionStarted {
                session_id: session.id,
            },
            None,
            unix_ms_now(),
        );
        let session = events.replay(session);

        Self { session, events }
    }

    fn record(&mut self, event: ReplEvent, run_id: Option<uuid::Uuid>) {
        let envelope = self.events.publish(event, run_id, unix_ms_now());
        self.session.apply_event(&envelope.event);
    }

    fn snapshot(&self) -> visionclip_common::ReplSessionSnapshot {
        visionclip_common::ReplSessionSnapshot {
            session: self.session.clone(),
            last_sequence: self.events.last_sequence(),
        }
    }

    fn events_after(&self, sequence: u64) -> (Vec<ReplEventEnvelope>, u64) {
        (
            self.events.events_after(sequence),
            self.events.last_sequence(),
        )
    }

    fn subscribe_after(&self, sequence: u64) -> ReplEventSubscription {
        self.events.subscribe_after(sequence)
    }
}

#[derive(Clone, Default)]
struct TtsPlaybackGate {
    lock: Arc<Mutex<()>>,
}

impl TtsPlaybackGate {
    async fn run<T>(&self, operation: impl Future<Output = T>) -> T {
        let _guard = self.lock.lock().await;
        operation.await
    }
}

async fn handle_connection(mut stream: UnixStream, state: Arc<AppState>) -> Result<()> {
    let payload = read_frame_payload(&mut stream).await?;

    if let Some(request) = decode_coddy_wire_request(&payload)? {
        return handle_coddy_connection(&mut stream, &state, request).await;
    }

    let request: VisionRequest = decode_payload(&payload)?;
    let request = match request {
        VisionRequest::ReplEventStream(job) => {
            return stream_repl_events(&mut stream, &state, job).await;
        }
        request => request,
    };

    let response = match process_request(&state, request).await {
        Ok(result) => result,
        Err(error) => {
            error!(?error, "job processing failed");
            JobResult::Error {
                request_id: uuid::Uuid::new_v4(),
                code: "processing_error".into(),
                message: error.to_string(),
            }
        }
    };

    write_message(&mut stream, &response).await?;
    Ok(())
}

fn decode_coddy_wire_request(payload: &[u8]) -> Result<Option<CoddyRequest>> {
    Ok(decode_wire_request_payload(payload)?)
}

async fn handle_coddy_connection(
    stream: &mut UnixStream,
    state: &AppState,
    request: CoddyRequest,
) -> Result<()> {
    let request_id = request.request_id();
    let request = match request {
        CoddyRequest::EventStream(job) => {
            return stream_coddy_repl_events(stream, state, job).await;
        }
        request => request,
    };

    let response = match process_coddy_request(state, request).await {
        Ok(result) => result,
        Err(error) => {
            error!(?error, "Coddy job processing failed");
            CoddyResult::Error {
                request_id,
                code: "processing_error".into(),
                message: error.to_string(),
            }
        }
    };

    write_frame(stream, &CoddyWireResult::new(response)).await?;
    Ok(())
}

async fn process_request(state: &AppState, request: VisionRequest) -> Result<JobResult> {
    match request {
        VisionRequest::Capture(job) => process_job(state, job).await,
        VisionRequest::VoiceSearch(job) => process_voice_search(state, job).await,
        VisionRequest::OpenApplication(job) => process_open_application(state, job).await,
        VisionRequest::OpenUrl(job) => process_open_url(state, job).await,
        VisionRequest::ReplCommand(job) => process_repl_command(state, job).await,
        VisionRequest::ReplSessionSnapshot(job) => process_repl_session_snapshot(state, job).await,
        VisionRequest::ReplEvents(job) => process_repl_events(state, job).await,
        VisionRequest::ReplEventStream(job) => Ok(JobResult::Error {
            request_id: job.request_id,
            code: "invalid_repl_stream_dispatch".to_string(),
            message: "ReplEventStream requires a persistent connection.".to_string(),
        }),
        VisionRequest::HealthCheck(job) => process_health_check(job).await,
    }
}

async fn process_coddy_request(state: &AppState, request: CoddyRequest) -> Result<CoddyResult> {
    let result = match request {
        CoddyRequest::Command(job) => process_repl_command(state, job).await?,
        CoddyRequest::SessionSnapshot(job) => process_repl_session_snapshot(state, job).await?,
        CoddyRequest::Events(job) => process_repl_events(state, job).await?,
        CoddyRequest::EventStream(job) => JobResult::Error {
            request_id: job.request_id,
            code: "invalid_repl_stream_dispatch".to_string(),
            message: "EventStream requires a persistent connection.".to_string(),
        },
    };

    Ok(map_job_result_to_coddy(result))
}

fn map_job_result_to_coddy(result: JobResult) -> CoddyResult {
    match result {
        JobResult::ClipboardText {
            request_id,
            text,
            spoken,
        } => CoddyResult::Text {
            request_id,
            text,
            spoken,
        },
        JobResult::BrowserQuery {
            request_id,
            query,
            summary,
            spoken,
        } => CoddyResult::BrowserQuery {
            request_id,
            query,
            summary,
            spoken,
        },
        JobResult::ActionStatus {
            request_id,
            message,
            spoken,
        } => CoddyResult::ActionStatus {
            request_id,
            message,
            spoken,
        },
        JobResult::Error {
            request_id,
            code,
            message,
        } => CoddyResult::Error {
            request_id,
            code,
            message,
        },
        JobResult::ReplSessionSnapshot {
            request_id,
            snapshot,
        } => CoddyResult::ReplSessionSnapshot {
            request_id,
            snapshot,
        },
        JobResult::ReplEvents {
            request_id,
            events,
            last_sequence,
        } => CoddyResult::ReplEvents {
            request_id,
            events,
            last_sequence,
        },
    }
}

async fn process_health_check(job: HealthCheckJob) -> Result<JobResult> {
    Ok(JobResult::ActionStatus {
        request_id: job.request_id,
        message: "VisionClip daemon ativo.".to_string(),
        spoken: false,
    })
}

async fn process_repl_session_snapshot(
    state: &AppState,
    job: ReplSessionSnapshotJob,
) -> Result<JobResult> {
    let repl = state.repl.lock().await;
    Ok(JobResult::ReplSessionSnapshot {
        request_id: job.request_id,
        snapshot: Box::new(repl.snapshot()),
    })
}

async fn process_repl_events(state: &AppState, job: ReplEventsJob) -> Result<JobResult> {
    let repl = state.repl.lock().await;
    let (events, last_sequence) = repl.events_after(job.after_sequence);
    Ok(JobResult::ReplEvents {
        request_id: job.request_id,
        events,
        last_sequence,
    })
}

async fn stream_repl_events(
    stream: &mut UnixStream,
    state: &AppState,
    job: ReplEventStreamJob,
) -> Result<()> {
    let mut subscription = {
        let repl = state.repl.lock().await;
        repl.subscribe_after(job.after_sequence)
    };

    info!(
        request_id = %job.request_id,
        after_sequence = job.after_sequence,
        "starting Coddy REPL event stream"
    );

    while let Some(event) = subscription.next().await {
        let last_sequence = event.sequence;
        let response = JobResult::ReplEvents {
            request_id: job.request_id,
            events: vec![event],
            last_sequence,
        };

        if let Err(error) = write_message(stream, &response).await {
            info!(
                request_id = %job.request_id,
                ?error,
                "Coddy REPL event stream closed"
            );
            return Ok(());
        }
    }

    Ok(())
}

async fn stream_coddy_repl_events(
    stream: &mut UnixStream,
    state: &AppState,
    job: ReplEventStreamJob,
) -> Result<()> {
    let mut subscription = {
        let repl = state.repl.lock().await;
        repl.subscribe_after(job.after_sequence)
    };

    info!(
        request_id = %job.request_id,
        after_sequence = job.after_sequence,
        "starting direct Coddy REPL event stream"
    );

    while let Some(event) = subscription.next().await {
        let last_sequence = event.sequence;
        let response = CoddyWireResult::new(CoddyResult::ReplEvents {
            request_id: job.request_id,
            events: vec![event],
            last_sequence,
        });

        if let Err(error) = write_frame(stream, &response).await {
            info!(
                request_id = %job.request_id,
                ?error,
                "direct Coddy REPL event stream closed"
            );
            return Ok(());
        }
    }

    Ok(())
}

async fn record_repl_event(state: &AppState, event: ReplEvent, run_id: Option<uuid::Uuid>) {
    state.repl.lock().await.record(event, run_id);
}

async fn process_open_application(
    state: &AppState,
    job: ApplicationLaunchJob,
) -> Result<JobResult> {
    let request_id = job.request_id;
    let total_started_at = Instant::now();
    let app_name = job.app_name.trim();
    let speak_requested = state
        .config
        .action_should_speak("OpenApplication", job.speak);

    info!(
        request_id = %request_id,
        transcript = ?job.transcript,
        app_name,
        speak_requested,
        "processing open application job"
    );

    let result = open_application(app_name)?;
    let message = result.message;
    let (tts_enqueue_ms, spoken) = enqueue_tts(
        state.piper.as_ref(),
        &state.tts_gate,
        request_id,
        "OpenApplication",
        &message,
        None,
        speak_requested,
    );

    let _ = notify("VisionClip", &message);

    info!(
        request_id = %request_id,
        app_name,
        resolved_app = %result.resolved_app,
        spoken,
        tts_enqueue_ms,
        total_ms = elapsed_ms(total_started_at),
        "open application job completed"
    );

    Ok(JobResult::ActionStatus {
        request_id,
        message,
        spoken,
    })
}

async fn process_open_url(state: &AppState, job: UrlOpenJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let total_started_at = Instant::now();
    let label = job.label.trim();
    let url = job.url.trim();
    let speak_requested = state.config.action_should_speak("OpenUrl", job.speak)
        || state
            .config
            .action_should_speak("OpenApplication", job.speak);

    validate_browser_url(url)?;

    info!(
        request_id = %request_id,
        transcript = ?job.transcript,
        label,
        url,
        speak_requested,
        "processing open url job"
    );

    open_url(url)?;
    let message = if label.is_empty() {
        "Abrindo o site.".to_string()
    } else {
        format!("Abrindo {label}.")
    };
    let (tts_enqueue_ms, spoken) = enqueue_tts(
        state.piper.as_ref(),
        &state.tts_gate,
        request_id,
        "OpenUrl",
        &message,
        None,
        speak_requested,
    );

    let _ = notify("VisionClip", &message);

    info!(
        request_id = %request_id,
        label,
        url,
        spoken,
        tts_enqueue_ms,
        total_ms = elapsed_ms(total_started_at),
        "open url job completed"
    );

    Ok(JobResult::ActionStatus {
        request_id,
        message,
        spoken,
    })
}

async fn process_repl_command(state: &AppState, job: ReplCommandJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let total_started_at = Instant::now();

    info!(
        request_id = %request_id,
        command = ?job.command,
        speak_requested = job.speak,
        "processing Coddy REPL command"
    );

    match job.command {
        ReplCommand::Ask { text, .. } => {
            let query = sanitize_output(&Action::SearchWeb, &text);
            if query.trim().is_empty() {
                record_repl_event(
                    state,
                    ReplEvent::Error {
                        code: "empty_repl_query".to_string(),
                        message: "Comando Coddy vazio.".to_string(),
                    },
                    Some(request_id),
                )
                .await;
                return Ok(JobResult::Error {
                    request_id,
                    code: "empty_repl_query".to_string(),
                    message: "Comando Coddy vazio.".to_string(),
                });
            }

            record_repl_event(
                state,
                ReplEvent::RunStarted { run_id: request_id },
                Some(request_id),
            )
            .await;
            record_repl_event(
                state,
                ReplEvent::MessageAppended {
                    message: ReplMessage {
                        id: uuid::Uuid::new_v4(),
                        role: "user".to_string(),
                        text,
                    },
                },
                Some(request_id),
            )
            .await;
            record_repl_event(
                state,
                ReplEvent::IntentDetected {
                    intent: ReplIntent::SearchDocs,
                    confidence: 0.72,
                },
                Some(request_id),
            )
            .await;

            let speak_requested = state
                .config
                .action_should_speak(Action::SearchWeb.as_str(), job.speak);
            let search_result =
                execute_search_query(state, request_id, &query, speak_requested, total_started_at)
                    .await?;
            record_repl_event(
                state,
                ReplEvent::RunCompleted { run_id: request_id },
                Some(request_id),
            )
            .await;

            Ok(JobResult::BrowserQuery {
                request_id,
                query,
                summary: search_result.summary,
                spoken: search_result.spoken,
            })
        }
        ReplCommand::VoiceTurn {
            transcript_override: Some(transcript),
        } => {
            record_repl_event(
                state,
                ReplEvent::VoiceTranscriptFinal {
                    text: transcript.clone(),
                },
                Some(request_id),
            )
            .await;

            match resolve_voice_turn_intent(&transcript) {
                Some(VoiceTurnIntent::OpenApplication {
                    transcript,
                    app_name,
                }) => {
                    record_repl_event(
                        state,
                        ReplEvent::IntentDetected {
                            intent: ReplIntent::OpenApplication,
                            confidence: 0.9,
                        },
                        Some(request_id),
                    )
                    .await;
                    record_repl_event(
                        state,
                        ReplEvent::ToolStarted {
                            name: "open_application".to_string(),
                        },
                        Some(request_id),
                    )
                    .await;

                    let result = process_open_application(
                        state,
                        ApplicationLaunchJob {
                            request_id,
                            transcript: Some(transcript),
                            app_name,
                            speak: job.speak,
                        },
                    )
                    .await;

                    record_repl_event(
                        state,
                        ReplEvent::ToolCompleted {
                            name: "open_application".to_string(),
                            status: tool_status_for_result(&result),
                        },
                        Some(request_id),
                    )
                    .await;
                    result
                }
                Some(VoiceTurnIntent::OpenWebsite {
                    transcript,
                    label,
                    url,
                }) => {
                    record_repl_event(
                        state,
                        ReplEvent::IntentDetected {
                            intent: ReplIntent::OpenWebsite,
                            confidence: 0.9,
                        },
                        Some(request_id),
                    )
                    .await;
                    record_repl_event(
                        state,
                        ReplEvent::ToolStarted {
                            name: "open_url".to_string(),
                        },
                        Some(request_id),
                    )
                    .await;

                    let result = process_open_url(
                        state,
                        UrlOpenJob {
                            request_id,
                            transcript: Some(transcript),
                            label,
                            url,
                            speak: job.speak,
                        },
                    )
                    .await;

                    record_repl_event(
                        state,
                        ReplEvent::ToolCompleted {
                            name: "open_url".to_string(),
                            status: tool_status_for_result(&result),
                        },
                        Some(request_id),
                    )
                    .await;
                    result
                }
                Some(VoiceTurnIntent::SearchWeb { transcript, query }) => {
                    let query = sanitize_output(&Action::SearchWeb, &query);
                    if query.trim().is_empty() {
                        record_repl_event(
                            state,
                            ReplEvent::Error {
                                code: "empty_voice_transcript".to_string(),
                                message: "Transcript de voz vazio.".to_string(),
                            },
                            Some(request_id),
                        )
                        .await;
                        return Ok(JobResult::Error {
                            request_id,
                            code: "empty_voice_transcript".to_string(),
                            message: "Transcript de voz vazio.".to_string(),
                        });
                    }

                    record_repl_event(
                        state,
                        ReplEvent::RunStarted { run_id: request_id },
                        Some(request_id),
                    )
                    .await;
                    record_repl_event(
                        state,
                        ReplEvent::IntentDetected {
                            intent: ReplIntent::SearchDocs,
                            confidence: 0.72,
                        },
                        Some(request_id),
                    )
                    .await;
                    let result = process_voice_search(
                        state,
                        VoiceSearchJob {
                            request_id,
                            transcript,
                            query,
                            speak: job.speak,
                        },
                    )
                    .await;
                    record_repl_event(
                        state,
                        ReplEvent::RunCompleted { run_id: request_id },
                        Some(request_id),
                    )
                    .await;
                    result
                }
                None => {
                    record_repl_event(
                        state,
                        ReplEvent::Error {
                            code: "empty_voice_transcript".to_string(),
                            message: "Transcript de voz vazio.".to_string(),
                        },
                        Some(request_id),
                    )
                    .await;
                    Ok(JobResult::Error {
                        request_id,
                        code: "empty_voice_transcript".to_string(),
                        message: "Transcript de voz vazio.".to_string(),
                    })
                }
            }
        }
        ReplCommand::VoiceTurn {
            transcript_override: None,
        } => {
            if job.speak {
                warn!(
                    request_id = %request_id,
                    "daemon received a voice turn without transcript; the CLI should capture ASR before sending"
                );
            }
            record_repl_event(
                state,
                ReplEvent::Error {
                    code: "missing_voice_transcript".to_string(),
                    message:
                        "Coddy não recebeu transcript de voz. Capture/transcreva no cliente antes de enviar."
                            .to_string(),
                },
                Some(request_id),
            )
            .await;
            Ok(JobResult::Error {
                request_id,
                code: "missing_voice_transcript".to_string(),
                message:
                    "Coddy não recebeu transcript de voz. Capture/transcreva no cliente antes de enviar."
                        .to_string(),
            })
        }
        command @ (ReplCommand::StopSpeaking
        | ReplCommand::StopActiveRun
        | ReplCommand::OpenUi { .. }
        | ReplCommand::SelectModel { .. }
        | ReplCommand::CaptureAndExplain { .. }) => {
            Ok(process_repl_local_command(state, request_id, command).await)
        }
    }
}

async fn process_repl_local_command(
    state: &AppState,
    request_id: uuid::Uuid,
    command: ReplCommand,
) -> JobResult {
    let mut repl = state.repl.lock().await;
    process_repl_local_command_locked(&mut repl, request_id, command)
}

fn process_repl_local_command_locked(
    repl: &mut ReplRuntimeState,
    request_id: uuid::Uuid,
    command: ReplCommand,
) -> JobResult {
    match command {
        ReplCommand::StopSpeaking | ReplCommand::StopActiveRun => JobResult::ActionStatus {
            request_id,
            message: "Comando Coddy recebido; cancelamento cooperativo será tratado pelo broker."
                .to_string(),
            spoken: false,
        },
        ReplCommand::OpenUi { mode } => {
            repl.record(ReplEvent::OverlayShown { mode }, Some(request_id));

            JobResult::ActionStatus {
                request_id,
                message: format!("Modo Coddy atualizado para {mode:?}."),
                spoken: false,
            }
        }
        ReplCommand::SelectModel { model, role } => {
            repl.record(
                ReplEvent::ModelSelected {
                    model: model.clone(),
                    role,
                },
                Some(request_id),
            );

            JobResult::ActionStatus {
                request_id,
                message: format!(
                    "Modelo Coddy atualizado para {} ({role:?}, provider {}).",
                    model.name, model.provider
                ),
                spoken: false,
            }
        }
        ReplCommand::CaptureAndExplain { .. } => JobResult::ActionStatus {
            request_id,
            message: "Comando Coddy reconhecido, mas ainda não implementado no daemon.".to_string(),
            spoken: false,
        },
        ReplCommand::Ask { .. } | ReplCommand::VoiceTurn { .. } => JobResult::Error {
            request_id,
            code: "unsupported_local_repl_command".to_string(),
            message: "Comando Coddy não é local e deve passar pelo pipeline completo.".to_string(),
        },
    }
}

fn validate_browser_url(url: &str) -> Result<()> {
    if url.contains(char::is_whitespace) {
        anyhow::bail!("refusing to open URL with whitespace");
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        anyhow::bail!("refusing to open non-http URL");
    }
    Ok(())
}

fn tool_status_for_result(result: &Result<JobResult>) -> ToolStatus {
    if result.is_ok() {
        ToolStatus::Succeeded
    } else {
        ToolStatus::Failed
    }
}

async fn process_job(state: &AppState, job: CaptureJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let action_name = job.action.as_str();
    let total_started_at = Instant::now();
    info!(
        request_id = %request_id,
        action = action_name,
        image_bytes = job.image_bytes.len(),
        speak_requested = job.speak,
        "processing capture job"
    );

    let ocr_started_at = Instant::now();
    let mut ocr_ms = 0_u64;
    let mut ocr_chars = 0_usize;
    let mut ocr_text = None;
    let use_dedicated_ocr = state.infer.has_ocr_model();
    if use_dedicated_ocr {
        let ocr_action = match job.action {
            Action::ExtractCode => Action::ExtractCode,
            _ => Action::CopyText,
        };

        match state
            .infer
            .infer_with_ocr_model(
                request_id.to_string(),
                ocr_action,
                job.image_bytes.clone(),
                job.mime_type.clone(),
            )
            .await
        {
            Ok(output) => {
                ocr_ms = elapsed_ms(ocr_started_at);
                let extracted = output.text.trim().to_string();
                if !extracted.is_empty() {
                    ocr_chars = extracted.chars().count();
                    ocr_text = Some(extracted);
                }
            }
            Err(error) => {
                ocr_ms = elapsed_ms(ocr_started_at);
                warn!(
                    ?error,
                    request_id = %request_id,
                    action = action_name,
                    "dedicated OCR stage failed; falling back to primary inference path"
                );
            }
        }
    }

    let infer_started_at = Instant::now();
    let mut inference_mode = "primary_image";
    let inference = match job.action {
        Action::CopyText | Action::ExtractCode => {
            if let Some(text) = ocr_text.clone() {
                inference_mode = "ocr_only";
                visionclip_infer::backend::InferenceOutput { text }
            } else {
                state
                    .infer
                    .infer(InferenceInput {
                        request_id: request_id.to_string(),
                        action: job.action.clone(),
                        source_app: job.source_app.clone(),
                        image_bytes: job.image_bytes.clone(),
                        mime_type: job.mime_type.clone(),
                    })
                    .await?
            }
        }
        _ => {
            if let Some(text) = ocr_text.clone().filter(|value| value.chars().count() >= 12) {
                inference_mode = "ocr_text_to_reasoning";
                match state
                    .infer
                    .infer_from_text(
                        request_id.to_string(),
                        job.action.clone(),
                        job.source_app.clone(),
                        text,
                    )
                    .await
                {
                    Ok(output) if !output.text.trim().is_empty() => output,
                    Ok(_) => {
                        warn!(
                            request_id = %request_id,
                            action = action_name,
                            "OCR text to reasoning returned empty content; falling back to primary image inference"
                        );
                        inference_mode = "primary_image_fallback";
                        state
                            .infer
                            .infer(InferenceInput {
                                request_id: request_id.to_string(),
                                action: job.action.clone(),
                                source_app: job.source_app.clone(),
                                image_bytes: job.image_bytes.clone(),
                                mime_type: job.mime_type.clone(),
                            })
                            .await?
                    }
                    Err(error) => {
                        warn!(
                            ?error,
                            request_id = %request_id,
                            action = action_name,
                            "OCR text to reasoning failed; falling back to primary image inference"
                        );
                        inference_mode = "primary_image_fallback";
                        state
                            .infer
                            .infer(InferenceInput {
                                request_id: request_id.to_string(),
                                action: job.action.clone(),
                                source_app: job.source_app.clone(),
                                image_bytes: job.image_bytes.clone(),
                                mime_type: job.mime_type.clone(),
                            })
                            .await?
                    }
                }
            } else {
                state
                    .infer
                    .infer(InferenceInput {
                        request_id: request_id.to_string(),
                        action: job.action.clone(),
                        source_app: job.source_app.clone(),
                        image_bytes: job.image_bytes.clone(),
                        mime_type: job.mime_type.clone(),
                    })
                    .await?
            }
        }
    };
    let infer_ms = elapsed_ms(infer_started_at);

    let sanitize_started_at = Instant::now();
    let cleaned = sanitize_output(&job.action, &inference.text);
    let sanitize_ms = elapsed_ms(sanitize_started_at);
    let output_chars = cleaned.chars().count();
    let speak_requested = state
        .config
        .action_should_speak(job.action.as_str(), job.speak);

    match job.action {
        Action::SearchWeb => {
            let search_result = execute_search_query(
                state,
                request_id,
                &cleaned,
                speak_requested,
                total_started_at,
            )
            .await?;

            info!(
                request_id = %request_id,
                action = action_name,
                inference_mode,
                ocr_ms,
                ocr_chars,
                infer_ms,
                sanitize_ms,
                output_chars,
                search_fetch_ms = search_result.search_fetch_ms,
                search_result_count = search_result.search_result_count,
                ai_overview_chars = search_result.ai_overview_chars,
                search_summary_chars = search_result
                    .summary
                    .as_ref()
                    .map(|value| value.chars().count())
                    .unwrap_or_default(),
                output_ms = search_result.output_ms,
                speak_requested,
                spoken = search_result.spoken,
                tts_enqueue_ms = search_result.tts_enqueue_ms,
                total_ms = elapsed_ms(total_started_at),
                "capture job completed"
            );

            Ok(JobResult::BrowserQuery {
                request_id,
                query: cleaned,
                summary: search_result.summary,
                spoken: search_result.spoken,
            })
        }
        _ => {
            let output_started_at = Instant::now();
            if cleaned.trim().is_empty() {
                let _ = notify(
                    "VisionClip",
                    "Nenhum resultado textual útil foi retornado para esta captura.",
                );
            } else {
                state.clipboard.set_text(&cleaned)?;
                let _ = notify(
                    "VisionClip",
                    "Resultado copiado para a área de transferência.",
                );
            }
            let output_ms = elapsed_ms(output_started_at);
            let speech_text = sanitize_for_speech(&job.action, &cleaned);
            let (tts_enqueue_ms, spoken) = enqueue_tts(
                state.piper.as_ref(),
                &state.tts_gate,
                request_id,
                action_name,
                &speech_text,
                Some(tts_fallback_message(&job.action)),
                speak_requested,
            );

            info!(
                request_id = %request_id,
                action = action_name,
                inference_mode,
                ocr_ms,
                ocr_chars,
                infer_ms,
                sanitize_ms,
                output_chars,
                output_ms,
                speak_requested,
                spoken,
                tts_enqueue_ms,
                total_ms = elapsed_ms(total_started_at),
                "capture job completed"
            );

            Ok(JobResult::ClipboardText {
                request_id,
                text: cleaned,
                spoken,
            })
        }
    }
}

async fn process_voice_search(state: &AppState, job: VoiceSearchJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let action_name = Action::SearchWeb.as_str();
    let total_started_at = Instant::now();
    let cleaned = sanitize_output(&Action::SearchWeb, &job.query);

    info!(
        request_id = %request_id,
        action = action_name,
        transcript = %job.transcript,
        query = %cleaned,
        speak_requested = job.speak,
        "processing voice search job"
    );

    let speak_requested = state.config.action_should_speak(action_name, job.speak);
    let search_result = execute_search_query(
        state,
        request_id,
        &cleaned,
        speak_requested,
        total_started_at,
    )
    .await?;

    info!(
        request_id = %request_id,
        action = action_name,
        search_fetch_ms = search_result.search_fetch_ms,
        search_result_count = search_result.search_result_count,
        ai_overview_chars = search_result.ai_overview_chars,
        search_summary_chars = search_result
            .summary
            .as_ref()
            .map(|value| value.chars().count())
            .unwrap_or_default(),
        output_ms = search_result.output_ms,
        speak_requested,
        spoken = search_result.spoken,
        tts_enqueue_ms = search_result.tts_enqueue_ms,
        total_ms = elapsed_ms(total_started_at),
        "voice search job completed"
    );

    Ok(JobResult::BrowserQuery {
        request_id,
        query: cleaned,
        summary: search_result.summary,
        spoken: search_result.spoken,
    })
}

struct SearchExecution {
    summary: Option<String>,
    spoken: bool,
    search_fetch_ms: u64,
    search_result_count: usize,
    ai_overview_chars: usize,
    output_ms: u64,
    tts_enqueue_ms: u64,
}

async fn execute_search_query(
    state: &AppState,
    request_id: uuid::Uuid,
    query: &str,
    speak_requested: bool,
    total_started_at: Instant,
) -> Result<SearchExecution> {
    let mut search_fetch_ms = 0_u64;
    let mut search_summary = None;
    let mut search_spoken_text = None;
    let mut search_result_count = 0_usize;
    let mut ai_overview_chars = 0_usize;
    let mut search_blocked = false;

    record_repl_event(
        state,
        ReplEvent::SearchStarted {
            query: query.to_string(),
            provider: "google".to_string(),
        },
        Some(request_id),
    )
    .await;

    if let Some(search) = &state.search {
        let search_started_at = Instant::now();
        match search.search(query).await {
            Ok(enrichment) => {
                search_fetch_ms = elapsed_ms(search_started_at);
                search_result_count = enrichment.snippets.len();
                ai_overview_chars = enrichment
                    .ai_overview
                    .as_ref()
                    .map(|value| value.chars().count())
                    .unwrap_or_default();
                if let Some(answer) = generate_google_ai_overview_answer(
                    &state.infer,
                    request_id,
                    query,
                    &enrichment,
                    "Visão Geral por IA extraída do Google Search",
                )
                .await
                {
                    search_spoken_text = Some(answer.clone());
                    search_summary = Some(clipboard_text_for_google_ai_answer(
                        query,
                        &answer,
                        &enrichment,
                        "Visão Geral por IA extraída do Google Search",
                    ));
                } else {
                    search_spoken_text = enrichment.spoken_text(query);
                    search_summary = enrichment.clipboard_text(query);
                }
            }
            Err(error) => {
                search_fetch_ms = elapsed_ms(search_started_at);
                search_blocked = search::is_google_challenge_page(&error.to_string());
                warn!(?error, request_id = %request_id, query = %query, search_fetch_ms, "failed to enrich search query");
            }
        }
    }

    record_repl_event(
        state,
        ReplEvent::SearchContextExtracted {
            provider: if state.search.is_some() {
                "google".to_string()
            } else {
                "disabled".to_string()
            },
            organic_results: search_result_count,
            ai_overview_present: ai_overview_chars > 0,
        },
        Some(request_id),
    )
    .await;

    if search_summary.is_none() {
        search_summary = Some(search_browser_fallback_summary(query, search_blocked));
    }
    if search_spoken_text.is_none() {
        search_spoken_text = Some(search_browser_fallback_speech(query, search_blocked));
    }

    let output_started_at = Instant::now();
    if let Some(summary) = &search_summary {
        state.clipboard.set_text(summary)?;
    }
    if state.config.search.open_browser {
        open_search_query(query)?;
        if ai_overview_chars == 0 {
            spawn_rendered_ai_overview_listener(state, request_id, query, speak_requested);
        }
    }
    let _ = notify(
        "VisionClip",
        if search_result_count > 0 || ai_overview_chars > 0 {
            "Pesquisa aberta e resumo inicial copiado."
        } else if search_blocked {
            "Pesquisa aberta. O Google bloqueou a coleta local de resultados nesta sessão."
        } else {
            "Pesquisa aberta no navegador com resumo inicial de fallback."
        },
    );
    let output_ms = elapsed_ms(output_started_at);
    let tts_text = sanitize_for_speech(
        &Action::SearchWeb,
        search_spoken_text
            .as_deref()
            .unwrap_or("Pesquisa aberta no navegador para aprofundar o tema."),
    );
    let (tts_enqueue_ms, spoken) = enqueue_tts(
        state.piper.as_ref(),
        &state.tts_gate,
        request_id,
        Action::SearchWeb.as_str(),
        &tts_text,
        None,
        speak_requested,
    );

    info!(
        request_id = %request_id,
        query = %query,
        search_fetch_ms,
        output_ms,
        speak_requested,
        spoken,
        total_ms = elapsed_ms(total_started_at),
        "search query executed"
    );

    Ok(SearchExecution {
        summary: search_summary,
        spoken,
        search_fetch_ms,
        search_result_count,
        ai_overview_chars,
        output_ms,
        tts_enqueue_ms,
    })
}

fn spawn_rendered_ai_overview_listener(
    state: &AppState,
    request_id: uuid::Uuid,
    query: &str,
    speak_requested: bool,
) {
    if !state.config.search.rendered_ai_overview_listener {
        return;
    }

    let job = rendered_search::RenderedSearchJob {
        request_id,
        query: query.to_string(),
        search: state.config.search.clone(),
        infer: state.infer.clone(),
    };
    let piper = state.piper.clone();
    let tts_gate = state.tts_gate.clone();

    tokio::spawn(async move {
        let started_at = Instant::now();
        match rendered_search::wait_for_rendered_ai_overview(&job).await {
            Ok(Some(result)) => {
                let grounded_answer = generate_google_ai_overview_answer(
                    &job.infer,
                    job.request_id,
                    &job.query,
                    &result.enrichment,
                    "Visão Geral por IA renderizada no Google Search",
                )
                .await;

                let summary = if let Some(answer) = grounded_answer.as_ref() {
                    Some(clipboard_text_for_google_ai_answer(
                        &job.query,
                        answer,
                        &result.enrichment,
                        "Visão Geral por IA renderizada no Google Search",
                    ))
                } else {
                    result.enrichment.clipboard_text(&job.query)
                };

                if let Some(summary) = summary {
                    if let Err(error) =
                        ClipboardOwner::new().and_then(|clipboard| clipboard.set_text(&summary))
                    {
                        warn!(
                            ?error,
                            request_id = %job.request_id,
                            "failed to copy rendered AI overview summary"
                        );
                    }
                }

                let speech_text = grounded_answer
                    .or_else(|| result.enrichment.spoken_text(&job.query))
                    .unwrap_or_else(|| "Encontrei a visão geral criada por IA do Google, mas não consegui gerar uma resposta confiável com esse trecho.".to_string());
                let speech_text = sanitize_for_speech(&Action::SearchWeb, &speech_text);
                let (tts_enqueue_ms, spoken) = enqueue_tts(
                    piper.as_ref(),
                    &tts_gate,
                    job.request_id,
                    "SearchWebRenderedOverview",
                    &speech_text,
                    None,
                    speak_requested,
                );
                let _ = notify(
                    "VisionClip",
                    "Visão geral criada por IA detectada na página renderizada.",
                );

                info!(
                    request_id = %job.request_id,
                    query = %job.query,
                    attempts = result.attempts,
                    ocr_chars = result.ocr_chars,
                    ai_overview_chars = result
                        .enrichment
                        .ai_overview
                        .as_ref()
                        .map(|value| value.chars().count())
                        .unwrap_or_default(),
                    capture_backend = result.capture_backend,
                    spoken,
                    tts_enqueue_ms,
                    total_ms = elapsed_ms(started_at),
                    "rendered AI overview listener completed"
                );
            }
            Ok(None) => {
                debug_rendered_ai_overview_not_found(job.request_id, &job.query, started_at);
            }
            Err(error) => {
                warn!(
                    ?error,
                    request_id = %job.request_id,
                    query = %job.query,
                    total_ms = elapsed_ms(started_at),
                    "rendered AI overview listener failed"
                );
            }
        }
    });
}

async fn generate_google_ai_overview_answer(
    infer: &OllamaBackend,
    request_id: uuid::Uuid,
    query: &str,
    enrichment: &SearchEnrichment,
    source_label: &str,
) -> Option<String> {
    let overview = enrichment
        .ai_overview
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let supporting_sources = supporting_sources_text(enrichment);

    match infer
        .answer_search_from_context(
            format!("{request_id}-google-ai-overview-answer"),
            query,
            source_label,
            overview,
            &supporting_sources,
        )
        .await
    {
        Ok(output) => {
            let answer = sanitize_output(&Action::Explain, &output.text);
            if answer.trim().is_empty() {
                warn!(
                    request_id = %request_id,
                    query = %query,
                    "grounded Google AI overview answer was empty"
                );
                None
            } else {
                Some(answer)
            }
        }
        Err(error) => {
            warn!(
                ?error,
                request_id = %request_id,
                query = %query,
                "failed to generate grounded Google AI overview answer"
            );
            None
        }
    }
}

fn clipboard_text_for_google_ai_answer(
    query: &str,
    answer: &str,
    enrichment: &SearchEnrichment,
    source_label: &str,
) -> String {
    let mut sections = vec![
        format!("Pesquisa: {query}"),
        format!(
            "Resposta baseada na Visão Geral por IA do Google:\n{}",
            answer.trim()
        ),
    ];

    if let Some(overview) = enrichment.ai_overview.as_ref() {
        sections.push(format!(
            "Contexto extraído de {source_label}:\n{}",
            overview.trim()
        ));
    }

    let sources = supporting_sources_text(enrichment);
    if !sources.is_empty() {
        sections.push(format!("Fontes orgânicas complementares:\n{sources}"));
    }

    sections.join("\n\n")
}

fn supporting_sources_text(enrichment: &SearchEnrichment) -> String {
    enrichment
        .snippets
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, item)| {
            if item.snippet.trim().is_empty() {
                format!("{}. {} ({})", index + 1, item.title, item.domain)
            } else {
                format!(
                    "{}. {} ({}) - {}",
                    index + 1,
                    item.title,
                    item.domain,
                    item.snippet
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn debug_rendered_ai_overview_not_found(request_id: uuid::Uuid, query: &str, started_at: Instant) {
    info!(
        request_id = %request_id,
        query = %query,
        total_ms = elapsed_ms(started_at),
        "rendered AI overview listener finished without visible AI overview"
    );
}

fn search_browser_fallback_summary(query: &str, search_blocked: bool) -> String {
    let leitura_inicial = if search_blocked {
        "A consulta foi aberta no navegador, mas o Google bloqueou a coleta local de resultados nesta sessão."
    } else {
        "A consulta foi aberta no navegador, mas o VisionClip não conseguiu extrair um resumo local útil desta resposta."
    };

    format!(
        "Pesquisa: {query}\n\nLeitura inicial:\n{leitura_inicial}\n\nPróximo passo:\nRevise a aba aberta e refine a busca com local, data, fonte ou tipo de resultado."
    )
}

fn search_browser_fallback_speech(query: &str, search_blocked: bool) -> String {
    if search_blocked {
        format!(
            "Pesquisa aberta no navegador para {query}. O Google bloqueou a coleta local de resultados nesta sessão."
        )
    } else {
        format!(
            "Pesquisa aberta no navegador para {query}. Revise a aba aberta para aprofundar o tema."
        )
    }
}

fn enqueue_tts(
    piper: Option<&PiperHttpClient>,
    tts_gate: &TtsPlaybackGate,
    request_id: uuid::Uuid,
    action_name: &str,
    text: &str,
    fallback_text: Option<&str>,
    requested: bool,
) -> (u64, bool) {
    if !requested {
        return (0, false);
    }

    let Some(piper) = piper.cloned() else {
        return (0, false);
    };

    let text = if text.trim().is_empty() {
        let Some(fallback) = fallback_text.filter(|value| !value.trim().is_empty()) else {
            warn!(request_id = %request_id, action = %action_name, "skipping TTS for empty text");
            return (0, false);
        };
        fallback.trim().to_string()
    } else {
        text.trim().to_string()
    };

    let enqueue_started_at = Instant::now();
    let action_name = action_name.to_string();
    let tts_gate = tts_gate.clone();

    tokio::spawn(async move {
        tts_gate
            .run(async move {
                let tts_started_at = Instant::now();
                match piper.synthesize(&text, None).await {
                    Ok(wav) => {
                        let tts_synthesize_ms = elapsed_ms(tts_started_at);
                        let playback_started_at = Instant::now();
                        let piper_for_playback = piper.clone();
                        match tokio::task::spawn_blocking(move || piper_for_playback.play_wav(&wav))
                            .await
                        {
                            Ok(Ok(())) => {
                                info!(
                                    request_id = %request_id,
                                    action = %action_name,
                                    output_chars = text.chars().count(),
                                    tts_synthesize_ms,
                                    tts_playback_ms = elapsed_ms(playback_started_at),
                                    "background TTS completed"
                                );
                            }
                            Ok(Err(error)) => {
                                warn!(?error, request_id = %request_id, action = %action_name, "failed to play synthesized audio");
                            }
                            Err(error) => {
                                warn!(?error, request_id = %request_id, action = %action_name, "TTS playback task failed");
                            }
                        }
                    }
                    Err(error) => {
                        warn!(?error, request_id = %request_id, action = %action_name, "failed to synthesize audio");
                    }
                }
            })
            .await;
    });

    (elapsed_ms(enqueue_started_at), true)
}

fn tts_fallback_message(action: &Action) -> &'static str {
    match action {
        Action::TranslatePtBr => "Não foi possível realizar a tradução para esta captura.",
        Action::Explain => "Não foi possível gerar uma explicação útil para esta captura.",
        Action::CopyText => "Não foi possível extrair texto desta captura.",
        Action::ExtractCode => "Não foi possível extrair código desta captura.",
        Action::SearchWeb => "Pesquisa aberta no navegador para aprofundar o tema.",
    }
}

fn cleanup_existing_socket(socket_path: &PathBuf) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .with_context(|| format!("failed to remove stale socket {}", socket_path.display()))?;
    }
    Ok(())
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use coddy_ipc::CoddyWireRequest;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::time::{sleep, Duration};

    #[test]
    fn browser_url_validation_allows_http_urls_only() {
        assert!(validate_browser_url("https://www.youtube.com/").is_ok());
        assert!(validate_browser_url("http://example.com").is_ok());
        assert!(validate_browser_url("file:///etc/passwd").is_err());
        assert!(validate_browser_url("https://example.com/a b").is_err());
    }

    #[test]
    fn coddy_wire_payload_decodes_before_legacy_fallback() {
        let request_id = uuid::Uuid::new_v4();
        let wire = CoddyWireRequest::new(CoddyRequest::Events(ReplEventsJob {
            request_id,
            after_sequence: 7,
        }));
        let payload = coddy_ipc::encode_payload(&wire).expect("encode coddy wire request");

        let decoded = decode_coddy_wire_request(&payload)
            .expect("decode coddy wire request")
            .expect("coddy request");

        let CoddyRequest::Events(job) = decoded else {
            panic!("unexpected coddy request")
        };
        assert_eq!(job.request_id, request_id);
        assert_eq!(job.after_sequence, 7);
    }

    #[test]
    fn legacy_payload_is_not_treated_as_coddy_wire_request() {
        let request_id = uuid::Uuid::new_v4();
        let legacy = VisionRequest::HealthCheck(HealthCheckJob { request_id });
        let payload = coddy_ipc::encode_payload(&legacy).expect("encode legacy request");

        assert!(decode_coddy_wire_request(&payload)
            .expect("decode legacy fallback")
            .is_none());
    }

    #[test]
    fn coddy_wire_payload_rejects_incompatible_version() {
        let mut wire =
            CoddyWireRequest::new(CoddyRequest::SessionSnapshot(ReplSessionSnapshotJob {
                request_id: uuid::Uuid::new_v4(),
            }));
        wire.protocol_version += 1;
        let payload = coddy_ipc::encode_payload(&wire).expect("encode coddy wire request");

        assert!(decode_coddy_wire_request(&payload).is_err());
    }

    #[test]
    fn repl_runtime_snapshot_records_reduced_events() {
        let config = AppConfig::default();
        let mut runtime = ReplRuntimeState::new(&config);
        let run_id = uuid::Uuid::new_v4();

        runtime.record(ReplEvent::RunStarted { run_id }, Some(run_id));
        runtime.record(
            ReplEvent::SearchStarted {
                query: "Quem foi Rousseau?".to_string(),
                provider: "google".to_string(),
            },
            Some(run_id),
        );
        runtime.record(ReplEvent::RunCompleted { run_id }, Some(run_id));

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.last_sequence, 4);
        assert_eq!(snapshot.session.active_run, None);
        assert_eq!(
            snapshot.session.status,
            visionclip_common::SessionStatus::Idle
        );
    }

    #[test]
    fn repl_runtime_returns_incremental_events_after_sequence() {
        let config = AppConfig::default();
        let mut runtime = ReplRuntimeState::new(&config);
        let run_id = uuid::Uuid::new_v4();

        runtime.record(ReplEvent::RunStarted { run_id }, Some(run_id));
        runtime.record(ReplEvent::RunCompleted { run_id }, Some(run_id));

        let (events, last_sequence) = runtime.events_after(1);

        assert_eq!(last_sequence, 3);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, 2);
        assert_eq!(events[1].sequence, 3);
    }

    #[tokio::test]
    async fn repl_runtime_subscription_receives_live_events_without_polling() {
        let config = AppConfig::default();
        let mut runtime = ReplRuntimeState::new(&config);
        let start_sequence = runtime.snapshot().last_sequence;
        let run_id = uuid::Uuid::new_v4();
        let mut subscription = runtime.subscribe_after(start_sequence);

        runtime.record(ReplEvent::RunStarted { run_id }, Some(run_id));

        let event = tokio::time::timeout(Duration::from_millis(100), subscription.next())
            .await
            .expect("live event before timeout")
            .expect("open subscription");

        assert_eq!(event.sequence, start_sequence + 1);
        assert!(matches!(event.event, ReplEvent::RunStarted { .. }));
    }

    #[test]
    fn repl_select_model_updates_chat_model_in_snapshot() {
        let mut runtime = ReplRuntimeState::new(&AppConfig::default());
        let request_id = uuid::Uuid::new_v4();
        let model = visionclip_common::ModelRef {
            provider: "ollama".to_string(),
            name: "qwen2.5:0.5b".to_string(),
        };

        let result = process_repl_local_command_locked(
            &mut runtime,
            request_id,
            ReplCommand::SelectModel {
                model: model.clone(),
                role: visionclip_common::ModelRole::Chat,
            },
        );

        assert!(matches!(result, JobResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.session.selected_model, model);
    }

    #[test]
    fn repl_open_ui_updates_session_mode_in_snapshot() {
        let mut runtime = ReplRuntimeState::new(&AppConfig::default());
        let request_id = uuid::Uuid::new_v4();

        let result = process_repl_local_command_locked(
            &mut runtime,
            request_id,
            ReplCommand::OpenUi {
                mode: visionclip_common::ReplMode::DesktopApp,
            },
        );

        assert!(matches!(result, JobResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        assert_eq!(
            snapshot.session.mode,
            visionclip_common::ReplMode::DesktopApp
        );
    }

    #[test]
    fn tool_status_maps_result_state() {
        let ok: Result<JobResult> = Ok(JobResult::ActionStatus {
            request_id: uuid::Uuid::new_v4(),
            message: "ok".to_string(),
            spoken: false,
        });
        let error: Result<JobResult> = Err(anyhow::anyhow!("boom"));

        assert_eq!(tool_status_for_result(&ok), ToolStatus::Succeeded);
        assert_eq!(tool_status_for_result(&error), ToolStatus::Failed);
    }

    #[tokio::test]
    async fn tts_playback_gate_serializes_concurrent_jobs() {
        let gate = TtsPlaybackGate::default();
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let first = spawn_gate_probe(gate.clone(), Arc::clone(&active), Arc::clone(&max_active));
        let second = spawn_gate_probe(gate, Arc::clone(&active), Arc::clone(&max_active));

        first.await.unwrap();
        second.await.unwrap();

        assert_eq!(max_active.load(Ordering::SeqCst), 1);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    fn spawn_gate_probe(
        gate: TtsPlaybackGate,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            gate.run(async move {
                let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now_active, Ordering::SeqCst);
                sleep(Duration::from_millis(50)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            })
            .await;
        })
    }
}
