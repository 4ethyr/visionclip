mod linux_apps;
mod rendered_search;
mod search;

use crate::linux_apps::open_application;
use crate::search::{GoogleSearchClient, SearchEnrichment};
use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc, time::Instant};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info, warn};
use visionclip_common::{
    read_message, write_message, Action, AppConfig, ApplicationLaunchJob, CaptureJob,
    HealthCheckJob, JobResult, VisionRequest, VoiceSearchJob,
};
use visionclip_infer::{
    postprocess::{sanitize_for_speech, sanitize_output},
    InferenceBackend, InferenceInput, OllamaBackend,
};
use visionclip_output::{notify, open_search_query, ClipboardOwner};
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
}

async fn handle_connection(mut stream: UnixStream, state: Arc<AppState>) -> Result<()> {
    let request: VisionRequest = read_message(&mut stream).await?;

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

async fn process_request(state: &AppState, request: VisionRequest) -> Result<JobResult> {
    match request {
        VisionRequest::Capture(job) => process_job(state, job).await,
        VisionRequest::VoiceSearch(job) => process_voice_search(state, job).await,
        VisionRequest::OpenApplication(job) => process_open_application(state, job).await,
        VisionRequest::HealthCheck(job) => process_health_check(job).await,
    }
}

async fn process_health_check(job: HealthCheckJob) -> Result<JobResult> {
    Ok(JobResult::ActionStatus {
        request_id: job.request_id,
        message: "VisionClip daemon ativo.".to_string(),
        spoken: false,
    })
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

    tokio::spawn(async move {
        let tts_started_at = Instant::now();
        match piper.synthesize(&text, None).await {
            Ok(wav) => {
                let tts_synthesize_ms = elapsed_ms(tts_started_at);
                let dispatch_started_at = Instant::now();
                if let Err(error) = piper.play_wav_detached(&wav) {
                    warn!(?error, request_id = %request_id, action = %action_name, "failed to dispatch synthesized audio");
                    return;
                }

                info!(
                    request_id = %request_id,
                    action = %action_name,
                    output_chars = text.chars().count(),
                    tts_synthesize_ms,
                    tts_dispatch_ms = elapsed_ms(dispatch_started_at),
                    "background TTS dispatched"
                );
            }
            Err(error) => {
                warn!(?error, request_id = %request_id, action = %action_name, "failed to synthesize audio");
            }
        }
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
