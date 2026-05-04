use crate::coddy_contract::{
    decode_wire_request_payload, evaluate_assistance, resolve_voice_turn_intent, write_frame,
    AssessmentPolicy, CoddyRequest, CoddyResult, CoddyWireResult, ModelRef, ReplCommand, ReplEvent,
    ReplEventBroker, ReplEventEnvelope, ReplEventStreamJob, ReplEventSubscription, ReplEventsJob,
    ReplIntent, ReplMessage, ReplMode, ReplSession, ReplSessionSnapshot, ReplSessionSnapshotJob,
    ReplToolsJob, RequestedHelp, ScreenAssistMode, ToolStatus, VoiceTurnIntent,
};
use anyhow::Result;
use std::{future::Future, pin::Pin, time::Instant};
use tokio::net::UnixStream;
use tracing::{error, info, warn};
use visionclip_common::{AppConfig, JobResult};

use super::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedVoiceTurn {
    OpenApplication {
        transcript: String,
        app_name: String,
    },
    OpenWebsite {
        transcript: String,
        label: String,
        url: String,
    },
    SearchWeb {
        transcript: String,
        query: String,
    },
}

#[derive(Debug)]
pub(super) enum ReplCommandDispatch {
    Ask { text: String },
    VoiceTurn { transcript_override: Option<String> },
    Local(ReplCommand),
}

pub(super) type ReplJobFuture<'a> = Pin<Box<dyn Future<Output = Result<JobResult>> + Send + 'a>>;

pub(super) trait ReplNativeServices {
    fn sanitize_search_query(&self, query: &str) -> String;

    fn answer_repl_question<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        command_text: String,
        speak: bool,
    ) -> ReplJobFuture<'a>;

    fn search_web<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        query: String,
        speak: bool,
        total_started_at: Instant,
    ) -> ReplJobFuture<'a>;

    fn open_application<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        transcript: String,
        app_name: String,
        speak: bool,
    ) -> ReplJobFuture<'a>;

    fn open_url<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        transcript: String,
        label: String,
        url: String,
        speak: bool,
    ) -> ReplJobFuture<'a>;

    fn voice_search<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        transcript: String,
        query: String,
        speak: bool,
    ) -> ReplJobFuture<'a>;
}

pub(crate) struct ReplRuntimeState {
    session: ReplSession,
    events: ReplEventBroker,
}

impl ReplRuntimeState {
    pub(crate) fn new(config: &AppConfig) -> Self {
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
            super::unix_ms_now(),
        );
        let session = events.replay(session);

        Self { session, events }
    }

    pub(crate) fn record(&mut self, event: ReplEvent, run_id: Option<uuid::Uuid>) {
        let envelope = self.events.publish(event, run_id, super::unix_ms_now());
        self.session.apply_event(&envelope.event);
    }

    pub(crate) fn snapshot(&self) -> ReplSessionSnapshot {
        ReplSessionSnapshot {
            session: self.session.clone(),
            last_sequence: self.events.last_sequence(),
        }
    }

    pub(crate) fn events_after(&self, sequence: u64) -> (Vec<ReplEventEnvelope>, u64) {
        (
            self.events.events_after(sequence),
            self.events.last_sequence(),
        )
    }

    pub(crate) fn subscribe_after(&self, sequence: u64) -> ReplEventSubscription {
        self.events.subscribe_after(sequence)
    }
}

pub(super) async fn record_event(state: &AppState, event: ReplEvent, run_id: Option<uuid::Uuid>) {
    state.repl.lock().await.record(event, run_id);
}

pub(super) async fn append_message(
    state: &AppState,
    request_id: uuid::Uuid,
    role: &str,
    text: String,
) {
    record_event(
        state,
        ReplEvent::MessageAppended {
            message: ReplMessage {
                id: uuid::Uuid::new_v4(),
                role: role.to_string(),
                text,
            },
        },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_error(
    state: &AppState,
    request_id: uuid::Uuid,
    code: impl Into<String>,
    message: impl Into<String>,
) {
    record_event(
        state,
        ReplEvent::Error {
            code: code.into(),
            message: message.into(),
        },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_run_started(state: &AppState, request_id: uuid::Uuid) {
    record_event(
        state,
        ReplEvent::RunStarted { run_id: request_id },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_run_completed(state: &AppState, request_id: uuid::Uuid) {
    record_event(
        state,
        ReplEvent::RunCompleted { run_id: request_id },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_search_docs_intent(
    state: &AppState,
    request_id: uuid::Uuid,
    confidence: f32,
) {
    record_intent(state, request_id, ReplIntent::SearchDocs, confidence).await;
}

pub(super) async fn record_ask_technical_question_intent(
    state: &AppState,
    request_id: uuid::Uuid,
    confidence: f32,
) {
    record_intent(
        state,
        request_id,
        ReplIntent::AskTechnicalQuestion,
        confidence,
    )
    .await;
}

pub(super) async fn record_open_application_intent(
    state: &AppState,
    request_id: uuid::Uuid,
    confidence: f32,
) {
    record_intent(state, request_id, ReplIntent::OpenApplication, confidence).await;
}

pub(super) async fn record_open_website_intent(
    state: &AppState,
    request_id: uuid::Uuid,
    confidence: f32,
) {
    record_intent(state, request_id, ReplIntent::OpenWebsite, confidence).await;
}

async fn record_intent(
    state: &AppState,
    request_id: uuid::Uuid,
    intent: ReplIntent,
    confidence: f32,
) {
    record_event(
        state,
        ReplEvent::IntentDetected { intent, confidence },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_voice_transcript_final(
    state: &AppState,
    request_id: uuid::Uuid,
    text: String,
) {
    record_event(
        state,
        ReplEvent::VoiceTranscriptFinal { text },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_tool_started(state: &AppState, request_id: uuid::Uuid, name: &str) {
    record_event(
        state,
        ReplEvent::ToolStarted {
            name: name.to_string(),
        },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_tool_completed(
    state: &AppState,
    request_id: uuid::Uuid,
    name: &str,
    status: ToolStatus,
) {
    record_event(
        state,
        ReplEvent::ToolCompleted {
            name: name.to_string(),
            status,
        },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_search_started(
    state: &AppState,
    request_id: uuid::Uuid,
    query: &str,
    provider: &str,
) {
    record_event(
        state,
        ReplEvent::SearchStarted {
            query: query.to_string(),
            provider: provider.to_string(),
        },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_search_context_extracted(
    state: &AppState,
    request_id: uuid::Uuid,
    provider: &str,
    organic_results: usize,
    ai_overview_present: bool,
) {
    record_event(
        state,
        ReplEvent::SearchContextExtracted {
            provider: provider.to_string(),
            organic_results,
            ai_overview_present,
        },
        Some(request_id),
    )
    .await;
}

pub(super) async fn record_browser_query_response(
    state: &AppState,
    request_id: uuid::Uuid,
    query: &str,
    summary: &Option<String>,
) {
    append_message(
        state,
        request_id,
        "assistant",
        assistant_message_for_browser_query(query, summary),
    )
    .await;
}

fn assistant_message_for_browser_query(query: &str, summary: &Option<String>) -> String {
    summary
        .clone()
        .unwrap_or_else(|| browser_query_fallback_summary(query))
}

fn browser_query_fallback_summary(query: &str) -> String {
    format!(
        "Pesquisa: {query}\n\nLeitura inicial:\nA consulta foi aberta no navegador, mas o VisionClip não conseguiu extrair um resumo local útil desta resposta.\n\nPróximo passo:\nRevise a aba aberta e refine a busca com local, data, fonte ou tipo de resultado."
    )
}

pub(super) fn process_local_command(
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
        ReplCommand::CaptureAndExplain { mode, policy } => {
            process_capture_and_explain_command(repl, request_id, mode, policy)
        }
        ReplCommand::DismissConfirmation => {
            repl.record(ReplEvent::ConfirmationDismissed, Some(request_id));

            JobResult::ActionStatus {
                request_id,
                message: "Confirmação de policy dispensada.".to_string(),
                spoken: false,
            }
        }
        ReplCommand::Ask { .. } | ReplCommand::VoiceTurn { .. } => JobResult::Error {
            request_id,
            code: "unsupported_local_repl_command".to_string(),
            message: "Comando Coddy não é local e deve passar pelo pipeline completo.".to_string(),
        },
    }
}

pub(super) async fn process_local_command_for_state(
    state: &AppState,
    request_id: uuid::Uuid,
    command: ReplCommand,
) -> JobResult {
    let mut repl = state.repl.lock().await;
    process_local_command(&mut repl, request_id, command)
}

pub(super) fn tool_status_for_result(result: &Result<JobResult>) -> ToolStatus {
    if result.is_ok() {
        ToolStatus::Succeeded
    } else {
        ToolStatus::Failed
    }
}

pub(super) fn dispatch_command(command: ReplCommand) -> ReplCommandDispatch {
    match command {
        ReplCommand::Ask { text, .. } => ReplCommandDispatch::Ask { text },
        ReplCommand::VoiceTurn {
            transcript_override,
        } => ReplCommandDispatch::VoiceTurn {
            transcript_override,
        },
        command @ (ReplCommand::StopSpeaking
        | ReplCommand::StopActiveRun
        | ReplCommand::OpenUi { .. }
        | ReplCommand::SelectModel { .. }
        | ReplCommand::CaptureAndExplain { .. }
        | ReplCommand::DismissConfirmation) => ReplCommandDispatch::Local(command),
    }
}

async fn process_repl_command(
    state: &AppState,
    request_id: uuid::Uuid,
    command: ReplCommandDispatch,
    speak: bool,
    services: &impl ReplNativeServices,
) -> Result<JobResult> {
    let total_started_at = Instant::now();

    info!(
        request_id = %request_id,
        command = ?command,
        speak_requested = speak,
        "processing Coddy REPL command"
    );

    match command {
        ReplCommandDispatch::Ask { text } => {
            let command_text = text.trim().to_string();
            if command_text.is_empty() {
                record_error(
                    state,
                    request_id,
                    "empty_repl_query",
                    "Comando Coddy vazio.",
                )
                .await;
                return Ok(JobResult::Error {
                    request_id,
                    code: "empty_repl_query".to_string(),
                    message: "Comando Coddy vazio.".to_string(),
                });
            }

            record_run_started(state, request_id).await;
            append_message(state, request_id, "user", command_text.clone()).await;

            if let Some(query) = explicit_web_search_query(&command_text) {
                let query = services.sanitize_search_query(&query);
                if query.trim().is_empty() {
                    record_error(
                        state,
                        request_id,
                        "empty_search_query",
                        "Consulta de busca Coddy vazia.",
                    )
                    .await;
                    return Ok(JobResult::Error {
                        request_id,
                        code: "empty_search_query".to_string(),
                        message: "Consulta de busca Coddy vazia.".to_string(),
                    });
                }

                record_search_docs_intent(state, request_id, 0.9).await;
                let result = services
                    .search_web(state, request_id, query, speak, total_started_at)
                    .await?;
                if let JobResult::BrowserQuery { query, summary, .. } = &result {
                    record_browser_query_response(state, request_id, query, summary).await;
                }
                record_run_completed(state, request_id).await;

                return Ok(result);
            }

            record_ask_technical_question_intent(state, request_id, 0.84).await;
            let result = services
                .answer_repl_question(state, request_id, command_text, speak)
                .await?;
            if let JobResult::ClipboardText { text, .. } = &result {
                append_message(state, request_id, "assistant", text.clone()).await;
            }
            record_run_completed(state, request_id).await;

            Ok(result)
        }
        ReplCommandDispatch::VoiceTurn {
            transcript_override: Some(transcript),
        } => {
            record_voice_transcript_final(state, request_id, transcript.clone()).await;

            match resolve_voice_turn(&transcript) {
                Some(ResolvedVoiceTurn::OpenApplication {
                    transcript,
                    app_name,
                }) => {
                    record_open_application_intent(state, request_id, 0.9).await;
                    record_tool_started(state, request_id, "open_application").await;

                    let result = services
                        .open_application(state, request_id, transcript, app_name, speak)
                        .await;

                    record_tool_completed(
                        state,
                        request_id,
                        "open_application",
                        tool_status_for_result(&result),
                    )
                    .await;
                    result
                }
                Some(ResolvedVoiceTurn::OpenWebsite {
                    transcript,
                    label,
                    url,
                }) => {
                    record_open_website_intent(state, request_id, 0.9).await;
                    record_tool_started(state, request_id, "open_url").await;

                    let result = services
                        .open_url(state, request_id, transcript, label, url, speak)
                        .await;

                    record_tool_completed(
                        state,
                        request_id,
                        "open_url",
                        tool_status_for_result(&result),
                    )
                    .await;
                    result
                }
                Some(ResolvedVoiceTurn::SearchWeb { transcript, query }) => {
                    let query = services.sanitize_search_query(&query);
                    if query.trim().is_empty() {
                        record_error(
                            state,
                            request_id,
                            "empty_voice_transcript",
                            "Transcript de voz vazio.",
                        )
                        .await;
                        return Ok(JobResult::Error {
                            request_id,
                            code: "empty_voice_transcript".to_string(),
                            message: "Transcript de voz vazio.".to_string(),
                        });
                    }

                    record_run_started(state, request_id).await;
                    record_search_docs_intent(state, request_id, 0.72).await;
                    let result = services
                        .voice_search(state, request_id, transcript, query, speak)
                        .await;
                    if let Ok(JobResult::BrowserQuery { query, summary, .. }) = &result {
                        record_browser_query_response(state, request_id, query, summary).await;
                    }
                    record_run_completed(state, request_id).await;
                    result
                }
                None => {
                    record_error(
                        state,
                        request_id,
                        "empty_voice_transcript",
                        "Transcript de voz vazio.",
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
        ReplCommandDispatch::VoiceTurn {
            transcript_override: None,
        } => {
            if speak {
                warn!(
                    request_id = %request_id,
                    "daemon received a voice turn without transcript; the CLI should capture ASR before sending"
                );
            }
            record_error(
                state,
                request_id,
                "missing_voice_transcript",
                "Coddy não recebeu transcript de voz. Capture/transcreva no cliente antes de enviar.",
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
        ReplCommandDispatch::Local(command) => {
            Ok(process_local_command_for_state(state, request_id, command).await)
        }
    }
}

pub(super) fn explicit_web_search_query(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_lowercase();
    let prefixes = [
        "pesquise por ",
        "pesquise ",
        "pesquisar ",
        "procure por ",
        "procure ",
        "busque por ",
        "busque ",
        "buscar ",
        "google ",
        "google: ",
        "pesquisa: ",
        "search for ",
        "search ",
        "web search ",
        "look up ",
    ];

    prefixes.iter().find_map(|prefix| {
        lower.strip_prefix(prefix).and_then(|_| {
            let query = trimmed[prefix.len()..].trim();
            if query.is_empty() {
                None
            } else {
                Some(query.to_string())
            }
        })
    })
}

pub(super) fn resolve_voice_turn(transcript: &str) -> Option<ResolvedVoiceTurn> {
    match resolve_voice_turn_intent(transcript)? {
        VoiceTurnIntent::OpenApplication {
            transcript,
            app_name,
        } => Some(ResolvedVoiceTurn::OpenApplication {
            transcript,
            app_name,
        }),
        VoiceTurnIntent::OpenWebsite {
            transcript,
            label,
            url,
        } => Some(ResolvedVoiceTurn::OpenWebsite {
            transcript,
            label,
            url,
        }),
        VoiceTurnIntent::SearchWeb { transcript, query } => {
            Some(ResolvedVoiceTurn::SearchWeb { transcript, query })
        }
    }
}

fn process_capture_and_explain_command(
    repl: &mut ReplRuntimeState,
    request_id: uuid::Uuid,
    mode: ScreenAssistMode,
    policy: AssessmentPolicy,
) -> JobResult {
    let requested_help = requested_help_for_screen_assist_mode(mode);
    let decision = evaluate_assistance(policy, requested_help);
    repl.record(
        ReplEvent::PolicyEvaluated {
            policy,
            allowed: decision.allowed,
        },
        Some(request_id),
    );

    if decision.requires_confirmation {
        return JobResult::ActionStatus {
            request_id,
            message: format!(
                "Confirme a política de uso antes de analisar a tela: {}",
                decision.reason
            ),
            spoken: false,
        };
    }

    if !decision.allowed {
        repl.record(
            ReplEvent::Error {
                code: "assessment_policy_blocked".to_string(),
                message: decision.reason.clone(),
            },
            Some(request_id),
        );
        return JobResult::Error {
            request_id,
            code: "assessment_policy_blocked".to_string(),
            message: decision.reason,
        };
    }

    repl.record(
        ReplEvent::RunStarted { run_id: request_id },
        Some(request_id),
    );
    repl.record(
        ReplEvent::IntentDetected {
            intent: intent_for_screen_assist_mode(mode),
            confidence: 0.86,
        },
        Some(request_id),
    );
    repl.record(
        ReplEvent::ToolStarted {
            name: "capture_and_explain".to_string(),
        },
        Some(request_id),
    );
    repl.record(
        ReplEvent::ToolCompleted {
            name: "capture_and_explain".to_string(),
            status: ToolStatus::Cancelled,
        },
        Some(request_id),
    );
    repl.record(
        ReplEvent::RunCompleted { run_id: request_id },
        Some(request_id),
    );

    JobResult::ActionStatus {
        request_id,
        message: "CaptureAndExplain foi autorizado; o conector de captura de tela do Coddy ainda será ligado ao backend de visão.".to_string(),
        spoken: false,
    }
}

fn requested_help_for_screen_assist_mode(mode: ScreenAssistMode) -> RequestedHelp {
    match mode {
        ScreenAssistMode::ExplainVisibleScreen | ScreenAssistMode::SummarizeDocument => {
            RequestedHelp::ExplainConcept
        }
        ScreenAssistMode::ExplainCode | ScreenAssistMode::DebugError => RequestedHelp::DebugCode,
        ScreenAssistMode::MultipleChoice => RequestedHelp::SolveMultipleChoice,
    }
}

fn intent_for_screen_assist_mode(mode: ScreenAssistMode) -> ReplIntent {
    match mode {
        ScreenAssistMode::ExplainVisibleScreen | ScreenAssistMode::SummarizeDocument => {
            ReplIntent::ExplainScreen
        }
        ScreenAssistMode::ExplainCode => ReplIntent::ExplainCode,
        ScreenAssistMode::DebugError => ReplIntent::DebugCode,
        ScreenAssistMode::MultipleChoice => ReplIntent::MultipleChoiceAssist,
    }
}

pub(crate) fn decode_request(payload: &[u8]) -> Result<Option<CoddyRequest>> {
    decode_wire_request_payload(payload)
}

pub(crate) async fn write_result(stream: &mut UnixStream, result: CoddyResult) -> Result<()> {
    write_frame(stream, &CoddyWireResult::new(result)).await?;
    Ok(())
}

pub(super) async fn handle_connection(
    stream: &mut UnixStream,
    state: &AppState,
    request: CoddyRequest,
    services: &impl ReplNativeServices,
) -> Result<()> {
    let request_id = request.request_id();
    let request = match request {
        CoddyRequest::EventStream(job) => {
            return stream_repl_events(stream, state, job).await;
        }
        request => request,
    };

    let response = match process_request(state, request, services).await {
        Ok(result) => result,
        Err(error) => {
            error!(?error, "Coddy job processing failed");
            processing_error(request_id, &error)
        }
    };

    write_result(stream, response).await?;
    Ok(())
}

async fn process_request(
    state: &AppState,
    request: CoddyRequest,
    services: &impl ReplNativeServices,
) -> Result<CoddyResult> {
    match request {
        CoddyRequest::Command(job) => Ok(map_job_result(
            process_repl_command(
                state,
                job.request_id,
                dispatch_command(job.command),
                job.speak,
                services,
            )
            .await?,
        )),
        CoddyRequest::SessionSnapshot(job) => process_repl_session_snapshot(state, job).await,
        CoddyRequest::Events(job) => process_repl_events(state, job).await,
        CoddyRequest::Tools(job) => Ok(process_repl_tools(job).await),
        CoddyRequest::EventStream(job) => Ok(CoddyResult::Error {
            request_id: job.request_id,
            code: "invalid_repl_stream_dispatch".to_string(),
            message: "EventStream requires a persistent connection.".to_string(),
        }),
    }
}

async fn process_repl_session_snapshot(
    state: &AppState,
    job: ReplSessionSnapshotJob,
) -> Result<CoddyResult> {
    let repl = state.repl.lock().await;
    Ok(CoddyResult::ReplSessionSnapshot {
        request_id: job.request_id,
        snapshot: Box::new(repl.snapshot()),
    })
}

async fn process_repl_events(state: &AppState, job: ReplEventsJob) -> Result<CoddyResult> {
    let repl = state.repl.lock().await;
    let (events, last_sequence) = repl.events_after(job.after_sequence);
    Ok(CoddyResult::ReplEvents {
        request_id: job.request_id,
        events,
        last_sequence,
    })
}

async fn process_repl_tools(job: ReplToolsJob) -> CoddyResult {
    CoddyResult::ReplTools {
        request_id: job.request_id,
        tools: coddy_tool_names(),
    }
}

fn coddy_tool_names() -> Vec<String> {
    let mut tools = vec![
        "filesystem.apply_edit",
        "filesystem.list_files",
        "filesystem.preview_edit",
        "filesystem.read_file",
        "filesystem.search_files",
        "shell.run",
    ];
    tools.sort_unstable();
    tools.into_iter().map(str::to_string).collect()
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
        "starting direct Coddy REPL event stream"
    );

    while let Some(event) = subscription.next().await {
        let last_sequence = event.sequence;
        let response = CoddyResult::ReplEvents {
            request_id: job.request_id,
            events: vec![event],
            last_sequence,
        };

        if let Err(error) = write_result(stream, response).await {
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

fn processing_error(request_id: uuid::Uuid, error: &anyhow::Error) -> CoddyResult {
    CoddyResult::Error {
        request_id,
        code: "processing_error".into(),
        message: error.to_string(),
    }
}

fn map_job_result(result: JobResult) -> CoddyResult {
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
        JobResult::DocumentStatus {
            request_id,
            message,
            spoken,
            ..
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
        JobResult::Search(response) => CoddyResult::ActionStatus {
            request_id: uuid::Uuid::parse_str(&response.request_id)
                .unwrap_or_else(|_| uuid::Uuid::new_v4()),
            message: response
                .diagnostics
                .and_then(|diagnostics| diagnostics.message)
                .unwrap_or_else(|| format!("Search returned {} hits.", response.hits.len())),
            spoken: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coddy_contract::{
        encode_payload, CoddyWireRequest, ModelRole, ReplEventsJob, ReplSessionSnapshotJob,
        ReplToolsJob, SessionStatus,
    };
    use tokio::time::Duration;
    use visionclip_common::{HealthCheckJob, VisionRequest};

    #[test]
    fn coddy_wire_payload_decodes_before_legacy_fallback() {
        let request_id = uuid::Uuid::new_v4();
        let wire = CoddyWireRequest::new(CoddyRequest::Events(ReplEventsJob {
            request_id,
            after_sequence: 7,
        }));
        let payload = encode_payload(&wire).expect("encode coddy wire request");

        let decoded = decode_request(&payload)
            .expect("decode coddy wire request")
            .expect("coddy request");

        let CoddyRequest::Events(job) = decoded else {
            panic!("unexpected coddy request")
        };
        assert_eq!(job.request_id, request_id);
        assert_eq!(job.after_sequence, 7);
    }

    #[test]
    fn coddy_wire_payload_decodes_tools_request() {
        let request_id = uuid::Uuid::new_v4();
        let wire = CoddyWireRequest::new(CoddyRequest::Tools(ReplToolsJob { request_id }));
        let payload = encode_payload(&wire).expect("encode coddy tools request");

        let decoded = decode_request(&payload)
            .expect("decode coddy tools request")
            .expect("coddy request");

        let CoddyRequest::Tools(job) = decoded else {
            panic!("unexpected coddy request")
        };
        assert_eq!(job.request_id, request_id);
    }

    #[test]
    fn coddy_tools_catalog_is_stable_and_sorted() {
        assert_eq!(
            coddy_tool_names(),
            vec![
                "filesystem.apply_edit".to_string(),
                "filesystem.list_files".to_string(),
                "filesystem.preview_edit".to_string(),
                "filesystem.read_file".to_string(),
                "filesystem.search_files".to_string(),
                "shell.run".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn coddy_tools_request_returns_builtin_catalog() {
        let request_id = uuid::Uuid::new_v4();
        let result = process_repl_tools(ReplToolsJob { request_id }).await;

        assert!(matches!(
            result,
            CoddyResult::ReplTools { request_id: actual, tools }
                if actual == request_id && tools == coddy_tool_names()
        ));
    }

    #[test]
    fn legacy_payload_is_not_treated_as_coddy_wire_request() {
        let request_id = uuid::Uuid::new_v4();
        let legacy = VisionRequest::HealthCheck(HealthCheckJob { request_id });
        let payload = encode_payload(&legacy).expect("encode legacy request");

        assert!(decode_request(&payload)
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
        let payload = encode_payload(&wire).expect("encode coddy wire request");

        assert!(decode_request(&payload).is_err());
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
        assert_eq!(snapshot.session.status, SessionStatus::Idle);
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
        let model = ModelRef {
            provider: "ollama".to_string(),
            name: "qwen2.5:0.5b".to_string(),
        };

        let result = process_local_command(
            &mut runtime,
            request_id,
            ReplCommand::SelectModel {
                model: model.clone(),
                role: ModelRole::Chat,
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

        let result = process_local_command(
            &mut runtime,
            request_id,
            ReplCommand::OpenUi {
                mode: ReplMode::DesktopApp,
            },
        );

        assert!(matches!(result, JobResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.session.mode, ReplMode::DesktopApp);
    }

    #[test]
    fn repl_capture_and_explain_requires_policy_confirmation_when_unknown() {
        let mut runtime = ReplRuntimeState::new(&AppConfig::default());
        let request_id = uuid::Uuid::new_v4();

        let result = process_local_command(
            &mut runtime,
            request_id,
            ReplCommand::CaptureAndExplain {
                mode: ScreenAssistMode::ExplainCode,
                policy: AssessmentPolicy::UnknownAssessment,
            },
        );

        assert!(matches!(result, JobResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.session.policy, AssessmentPolicy::UnknownAssessment);
        assert_eq!(snapshot.session.status, SessionStatus::AwaitingConfirmation);
    }

    #[test]
    fn repl_dismiss_confirmation_returns_session_to_idle() {
        let mut runtime = ReplRuntimeState::new(&AppConfig::default());
        let request_id = uuid::Uuid::new_v4();

        process_local_command(
            &mut runtime,
            request_id,
            ReplCommand::CaptureAndExplain {
                mode: ScreenAssistMode::ExplainCode,
                policy: AssessmentPolicy::UnknownAssessment,
            },
        );
        let result =
            process_local_command(&mut runtime, request_id, ReplCommand::DismissConfirmation);

        assert!(matches!(result, JobResult::ActionStatus { .. }));
        assert_eq!(runtime.snapshot().session.status, SessionStatus::Idle);
    }

    #[test]
    fn repl_capture_and_explain_blocks_restricted_multiple_choice() {
        let mut runtime = ReplRuntimeState::new(&AppConfig::default());
        let request_id = uuid::Uuid::new_v4();

        let result = process_local_command(
            &mut runtime,
            request_id,
            ReplCommand::CaptureAndExplain {
                mode: ScreenAssistMode::MultipleChoice,
                policy: AssessmentPolicy::RestrictedAssessment,
            },
        );

        assert!(matches!(
            result,
            JobResult::Error {
                code,
                ..
            } if code == "assessment_policy_blocked"
        ));
        assert_eq!(runtime.snapshot().session.status, SessionStatus::Error);
    }

    #[test]
    fn repl_capture_and_explain_records_authorized_screen_assist_lifecycle() {
        let mut runtime = ReplRuntimeState::new(&AppConfig::default());
        let request_id = uuid::Uuid::new_v4();

        let result = process_local_command(
            &mut runtime,
            request_id,
            ReplCommand::CaptureAndExplain {
                mode: ScreenAssistMode::DebugError,
                policy: AssessmentPolicy::Practice,
            },
        );

        assert!(matches!(result, JobResult::ActionStatus { .. }));
        let (events, _) = runtime.events_after(1);
        assert!(matches!(
            events[0].event,
            ReplEvent::PolicyEvaluated { allowed: true, .. }
        ));
        assert!(matches!(events[1].event, ReplEvent::RunStarted { .. }));
        assert!(matches!(
            events[2].event,
            ReplEvent::IntentDetected {
                intent: ReplIntent::DebugCode,
                ..
            }
        ));
        assert!(matches!(events[3].event, ReplEvent::ToolStarted { .. }));
        assert!(matches!(
            events[4].event,
            ReplEvent::ToolCompleted {
                status: ToolStatus::Cancelled,
                ..
            }
        ));
        assert!(matches!(events[5].event, ReplEvent::RunCompleted { .. }));
        assert_eq!(runtime.snapshot().session.status, SessionStatus::Idle);
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

    #[test]
    fn repl_ask_keeps_plain_messages_in_agent_mode() {
        assert_eq!(explicit_web_search_query("olá"), None);
        assert_eq!(explicit_web_search_query("explique esse código"), None);
        assert_eq!(explicit_web_search_query("terminal"), None);
    }

    #[test]
    fn repl_ask_uses_web_search_only_for_explicit_search_prefixes() {
        assert_eq!(
            explicit_web_search_query("pesquise quando foi fundada a NASA").as_deref(),
            Some("quando foi fundada a NASA")
        );
        assert_eq!(
            explicit_web_search_query("search rust ownership").as_deref(),
            Some("rust ownership")
        );
        assert_eq!(
            explicit_web_search_query("Google: Quem foi Rousseau?").as_deref(),
            Some("Quem foi Rousseau?")
        );
    }

    #[test]
    fn voice_turn_resolution_stays_behind_daemon_bridge() {
        assert!(matches!(
            resolve_voice_turn("abra o firefox"),
            Some(ResolvedVoiceTurn::OpenApplication { app_name, .. }) if app_name == "firefox"
        ));
        assert!(matches!(
            resolve_voice_turn("abra o youtube"),
            Some(ResolvedVoiceTurn::OpenWebsite { label, url, .. })
                if label == "YouTube" && url == "https://www.youtube.com/"
        ));
        assert!(matches!(
            resolve_voice_turn("open you too"),
            Some(ResolvedVoiceTurn::OpenWebsite { label, url, .. })
                if label == "YouTube" && url == "https://www.youtube.com/"
        ));
        assert!(matches!(
            resolve_voice_turn("quem foi rousseau"),
            Some(ResolvedVoiceTurn::SearchWeb { query, .. }) if query == "quem foi rousseau"
        ));
    }

    #[test]
    fn browser_query_assistant_message_prefers_search_summary() {
        let summary = Some("Resumo estruturado da pesquisa.".to_string());

        assert_eq!(
            assistant_message_for_browser_query("Quem foi Rousseau?", &summary),
            "Resumo estruturado da pesquisa."
        );
    }

    #[test]
    fn browser_query_assistant_message_falls_back_when_search_summary_is_missing() {
        let message = assistant_message_for_browser_query("Quem foi Rousseau?", &None);

        assert!(message.contains("Pesquisa: Quem foi Rousseau?"));
        assert!(message.contains("Leitura inicial:"));
    }
}
