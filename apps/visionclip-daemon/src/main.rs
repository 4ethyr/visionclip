#[cfg(feature = "coddy-protocol")]
mod coddy_bridge;
#[cfg(feature = "coddy-protocol")]
mod coddy_contract;
mod linux_apps;
mod rendered_search;
mod search;

use crate::linux_apps::open_application;
use crate::search::{GoogleSearchClient, SearchEnrichment};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(feature = "coddy-protocol")]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{HashMap, HashSet},
    fs,
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use visionclip_common::{
    decode_message_payload, read_message_payload, redact_for_audit, write_message, Action,
    AppConfig, ApplicationLaunchJob, AuditLog, CaptureJob, DocumentAskJob, DocumentControlJob,
    DocumentControlKind, DocumentIngestJob, DocumentReadJob, DocumentSummarizeJob,
    DocumentTranslateJob, HealthCheckJob, JobResult, PermissionEngine, PolicyDecision, PolicyInput,
    RiskContext, RiskLevel, SessionId, SessionManager, ToolCall, ToolRegistry, UrlOpenJob,
    VisionRequest, VoiceSearchJob,
};
use visionclip_documents::{
    AudioChunk, AudioSink, ChunkerConfig, DocumentChunk, DocumentRuntime, IngestedDocument,
    ReadingProgress, ReadingProgressStore, ReadingSession, ReadingStatus,
    TranslatedReadingPipeline, TranslatedUnit, TranslationProvider, TranslationRequest,
    TtsProvider, TtsRequest,
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
    let document_store = match DocumentStore::load(&config) {
        Ok(store) => store,
        Err(error) => {
            warn!(
                ?error,
                "failed to load persisted document store; starting with an empty store"
            );
            DocumentStore::new(&config)?
        }
    };

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
        tools: ToolRegistry::builtin(),
        permission_engine: PermissionEngine::default(),
        audit_log: AuditLog::default(),
        sessions: Mutex::new(SessionManager::default()),
        documents: Arc::new(Mutex::new(document_store)),
        #[cfg(feature = "coddy-protocol")]
        repl: Mutex::new(coddy_bridge::ReplRuntimeState::new(&config)),
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
    tools: ToolRegistry,
    permission_engine: PermissionEngine,
    audit_log: AuditLog,
    sessions: Mutex<SessionManager>,
    documents: Arc<Mutex<DocumentStore>>,
    #[cfg(feature = "coddy-protocol")]
    repl: Mutex<coddy_bridge::ReplRuntimeState>,
}

struct DocumentStore {
    runtime: DocumentRuntime,
    storage_path: PathBuf,
    documents: HashMap<String, IngestedDocument>,
    reading_sessions: HashMap<String, ReadingSession>,
    progress: HashMap<String, ReadingProgress>,
    translations: HashMap<String, Vec<TranslatedUnit>>,
    embeddings: HashMap<String, Vec<DocumentChunkEmbedding>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DocumentStoreSnapshot {
    version: u32,
    documents: HashMap<String, IngestedDocument>,
    reading_sessions: HashMap<String, ReadingSession>,
    progress: HashMap<String, ReadingProgress>,
    translations: HashMap<String, Vec<TranslatedUnit>>,
    #[serde(default)]
    embeddings: HashMap<String, Vec<DocumentChunkEmbedding>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DocumentChunkEmbedding {
    chunk_id: String,
    chunk_index: usize,
    vector: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
struct DocumentContextLimits {
    max_chunks: usize,
    max_chars: usize,
}

impl DocumentStore {
    fn new(config: &AppConfig) -> Result<Self> {
        Ok(Self::empty(config, config.documents_store_path()?))
    }

    fn load(config: &AppConfig) -> Result<Self> {
        Self::load_from_path(config, config.documents_store_path()?)
    }

    fn load_from_path(config: &AppConfig, storage_path: PathBuf) -> Result<Self> {
        let mut store = Self::empty(config, storage_path.clone());
        if !storage_path.exists() {
            return Ok(store);
        }

        let raw = fs::read_to_string(&storage_path)
            .with_context(|| format!("failed to read {}", storage_path.display()))?;
        if raw.trim().is_empty() {
            return Ok(store);
        }

        let snapshot: DocumentStoreSnapshot = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", storage_path.display()))?;
        if snapshot.version != 1 {
            anyhow::bail!(
                "unsupported document store version {} in {}",
                snapshot.version,
                storage_path.display()
            );
        }

        store.documents = snapshot.documents;
        store.reading_sessions = snapshot.reading_sessions;
        store.progress = snapshot.progress;
        store.translations = snapshot.translations;
        store.embeddings = snapshot.embeddings;
        Ok(store)
    }

    fn empty(config: &AppConfig, storage_path: PathBuf) -> Self {
        Self {
            runtime: DocumentRuntime::new(ChunkerConfig {
                target_chars: config.documents.chunk_chars,
                overlap_chars: config.documents.chunk_overlap_chars,
            }),
            storage_path,
            documents: HashMap::new(),
            reading_sessions: HashMap::new(),
            progress: HashMap::new(),
            translations: HashMap::new(),
            embeddings: HashMap::new(),
        }
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let snapshot = DocumentStoreSnapshot {
            version: 1,
            documents: self.documents.clone(),
            reading_sessions: self.reading_sessions.clone(),
            progress: self.progress.clone(),
            translations: self.translations.clone(),
            embeddings: self.embeddings.clone(),
        };
        let encoded = serde_json::to_vec_pretty(&snapshot)?;
        let tmp_path = self.storage_path.with_extension("json.tmp");
        fs::write(&tmp_path, encoded)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &self.storage_path).with_context(|| {
            format!(
                "failed to replace {} with {}",
                self.storage_path.display(),
                tmp_path.display()
            )
        })?;
        Ok(())
    }
}

#[derive(Clone)]
struct OllamaDocumentTranslator {
    infer: OllamaBackend,
    request_id: uuid::Uuid,
}

#[async_trait]
impl TranslationProvider for OllamaDocumentTranslator {
    async fn translate(&self, request: TranslationRequest) -> Result<String> {
        ensure_supported_document_target_language(&request.target_language)?;
        let output = self
            .infer
            .infer_from_text(
                format!(
                    "{}-document-translate-{}",
                    self.request_id, request.chunk_index
                ),
                Action::TranslatePtBr,
                Some("document".to_string()),
                request.source_text,
            )
            .await?;
        Ok(sanitize_output(&Action::TranslatePtBr, &output.text))
    }
}

#[derive(Clone)]
struct PiperDocumentTts {
    piper: PiperHttpClient,
}

#[async_trait]
impl TtsProvider for PiperDocumentTts {
    async fn synthesize(&self, request: TtsRequest) -> Result<Vec<u8>> {
        self.piper
            .synthesize(&request.text, request.voice_id.as_deref())
            .await
    }
}

#[derive(Clone)]
struct PiperDocumentAudioSink {
    piper: PiperHttpClient,
    tts_gate: TtsPlaybackGate,
}

#[async_trait]
impl AudioSink for PiperDocumentAudioSink {
    async fn play(&self, chunk: AudioChunk) -> Result<()> {
        let piper = self.piper.clone();
        self.tts_gate
            .run(async move {
                tokio::task::spawn_blocking(move || piper.play_wav(&chunk.bytes))
                    .await
                    .context("document audio playback task failed")?
            })
            .await
    }
}

#[derive(Clone)]
struct DaemonReadingProgressStore {
    documents: Arc<Mutex<DocumentStore>>,
}

#[async_trait]
impl ReadingProgressStore for DaemonReadingProgressStore {
    async fn save_progress(&self, progress: ReadingProgress) -> Result<()> {
        let mut documents = self.documents.lock().await;
        if let Some(session) = documents.reading_sessions.get_mut(&progress.session_id) {
            session.current_chunk_index = progress.current_chunk_index;
            session.status = progress.status;
        }
        documents
            .progress
            .insert(progress.session_id.clone(), progress);
        documents.persist()?;
        Ok(())
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
    let payload = read_message_payload(&mut stream).await?;

    #[cfg(feature = "coddy-protocol")]
    {
        if let Some(request) = coddy_bridge::decode_request(&payload)? {
            let services = DaemonReplNativeServices;
            return coddy_bridge::handle_connection(&mut stream, &state, request, &services).await;
        }
    }

    let request: VisionRequest = decode_message_payload(&payload)?;

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
        VisionRequest::OpenUrl(job) => process_open_url(state, job).await,
        VisionRequest::HealthCheck(job) => process_health_check(job).await,
        VisionRequest::DocumentIngest(job) => process_document_ingest(state, job).await,
        VisionRequest::DocumentTranslate(job) => process_document_translate(state, job).await,
        VisionRequest::DocumentRead(job) => process_document_read(state, job).await,
        VisionRequest::DocumentControl(job) => process_document_control(state, job).await,
        VisionRequest::DocumentAsk(job) => process_document_ask(state, job).await,
        VisionRequest::DocumentSummarize(job) => process_document_summarize(state, job).await,
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
    let session_id = ensure_request_session(state, request_id).await;

    info!(
        request_id = %request_id,
        transcript = ?job.transcript,
        app_name,
        speak_requested,
        "processing open application job"
    );

    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-open-application"),
            "open_application",
            json!({"app_name": app_name}),
        ),
        RiskContext::user_initiated(),
    )?;

    let result = match open_application(app_name) {
        Ok(result) => result,
        Err(error) => {
            record_tool_failed(
                state,
                &session_id,
                &authorized_tool.name,
                authorized_tool.risk_level,
                &error.to_string(),
            );
            return Err(error);
        }
    };
    let message = result.message;
    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({"resolved_app": result.resolved_app, "message": message}),
    );
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
    let session_id = ensure_request_session(state, request_id).await;

    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-open-url"),
            "open_url",
            json!({"url": url, "label": label}),
        ),
        RiskContext::user_initiated(),
    )?;

    if let Err(error) = validate_browser_url(url) {
        record_tool_failed(
            state,
            &session_id,
            &authorized_tool.name,
            authorized_tool.risk_level,
            &error.to_string(),
        );
        return Err(error);
    }

    info!(
        request_id = %request_id,
        transcript = ?job.transcript,
        label,
        url,
        speak_requested,
        "processing open url job"
    );

    if let Err(error) = open_url(url) {
        record_tool_failed(
            state,
            &session_id,
            &authorized_tool.name,
            authorized_tool.risk_level,
            &error.to_string(),
        );
        return Err(error);
    }
    let message = if label.is_empty() {
        "Abrindo o site.".to_string()
    } else {
        format!("Abrindo {label}.")
    };
    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({"url": url, "label": label, "message": message}),
    );
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

async fn process_document_ingest(state: &AppState, job: DocumentIngestJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let session_id = ensure_request_session(state, request_id).await;
    let path_text = job.path.display().to_string();
    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-document-ingest"),
            "ingest_document",
            json!({"path": path_text}),
        ),
        RiskContext::user_initiated(),
    )?;

    let runtime = {
        let documents = state.documents.lock().await;
        documents.runtime.clone()
    };
    let ingested = runtime.ingest_path(&job.path)?;
    let document_id = ingested.document.id.as_str().to_string();
    let title = ingested.document.title.clone();
    let chunks = ingested.chunks.len();
    let generated_embeddings =
        generate_document_embeddings(state, request_id, &document_id, &ingested.chunks).await;
    let embeddings_status = if generated_embeddings.is_some() {
        "stored"
    } else {
        "not_stored"
    };

    {
        let mut documents = state.documents.lock().await;
        documents.documents.insert(document_id.clone(), ingested);
        if let Some(embeddings) = generated_embeddings {
            documents.embeddings.insert(document_id.clone(), embeddings);
        }
        documents.persist()?;
    }

    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({
            "document_id": document_id,
            "title": title,
            "chunks": chunks,
            "embeddings": embeddings_status,
        }),
    );
    state.audit_log.record_tool_event(
        "document.ingested",
        Some(session_id),
        "ingest_document",
        RiskLevel::Level2,
        "ingested",
        json!({
            "document_id": document_id,
            "title": title,
            "chunks": chunks,
            "embeddings": embeddings_status,
        }),
    );

    Ok(JobResult::DocumentStatus {
        request_id,
        document_id: Some(document_id),
        reading_session_id: None,
        chunks: Some(chunks),
        message: format!("Documento ingerido: {title}."),
        spoken: false,
    })
}

async fn process_document_translate(
    state: &AppState,
    job: DocumentTranslateJob,
) -> Result<JobResult> {
    let request_id = job.request_id;
    let target_language = normalize_document_target_language(&job.target_language)?;
    let session_id = ensure_request_session(state, request_id).await;
    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-document-translate"),
            "translate_document",
            json!({"document_id": &job.document_id, "target_language": &target_language}),
        ),
        RiskContext::user_initiated(),
    )?;

    let (document_id, title, chunks) = {
        let documents = state.documents.lock().await;
        let ingested = documents.documents.get(&job.document_id).with_context(|| {
            format!(
                "document `{}` was not ingested in this daemon session",
                job.document_id
            )
        })?;
        (
            ingested.document.id.clone(),
            ingested.document.title.clone(),
            ingested.chunks.clone(),
        )
    };

    let translator = OllamaDocumentTranslator {
        infer: state.infer.clone(),
        request_id,
    };
    let translation_session_id = format!("{request_id}-translate");
    let mut translated_units = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let translated_text = translator
            .translate(TranslationRequest {
                chunk_index: chunk.chunk_index,
                source_text: chunk.text.clone(),
                target_language: target_language.clone(),
            })
            .await?;
        translated_units.push(TranslatedUnit {
            session_id: translation_session_id.clone(),
            chunk_id: chunk.id,
            chunk_index: chunk.chunk_index,
            source_text: chunk.text,
            translated_text,
            target_language: target_language.clone(),
        });
    }

    let translated_text = translated_units
        .iter()
        .map(|unit| unit.translated_text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !translated_text.trim().is_empty() {
        state.clipboard.set_text(&translated_text)?;
    }

    let translated_chunks = translated_units.len();
    {
        let mut documents = state.documents.lock().await;
        documents
            .translations
            .insert(document_id.as_str().to_string(), translated_units);
        documents.persist()?;
    }

    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({
            "document_id": document_id.as_str(),
            "target_language": &target_language,
            "translated_chunks": translated_chunks,
        }),
    );

    Ok(JobResult::DocumentStatus {
        request_id,
        document_id: Some(document_id.as_str().to_string()),
        reading_session_id: None,
        chunks: Some(translated_chunks),
        message: format!("Documento traduzido e copiado para o clipboard: {title}."),
        spoken: false,
    })
}

async fn process_document_read(state: &AppState, job: DocumentReadJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let target_language = normalize_document_target_language(&job.target_language)?;
    let session_id = ensure_request_session(state, request_id).await;
    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-document-read"),
            "read_document_aloud",
            json!({"document_id": &job.document_id, "target_language": &target_language}),
        ),
        RiskContext::user_initiated(),
    )?;

    let Some(piper) = state.piper.clone() else {
        anyhow::bail!(
            "audio is disabled; enable [audio] and configure Piper HTTP before reading documents aloud"
        );
    };
    let (document_id, title, chunks, reading_session) = {
        let mut documents = state.documents.lock().await;
        let ingested = documents.documents.get(&job.document_id).with_context(|| {
            format!(
                "document `{}` was not ingested in this daemon session",
                job.document_id
            )
        })?;
        let document_id = ingested.document.id.clone();
        let title = ingested.document.title.clone();
        let chunks = ingested.chunks.clone();
        let mut reading_session = ReadingSession::new(document_id.clone(), target_language.clone());
        reading_session.start();
        documents
            .reading_sessions
            .insert(reading_session.id.clone(), reading_session.clone());
        documents.persist()?;
        (document_id, title, chunks, reading_session)
    };

    state.audit_log.record_tool_event(
        "document.reading_started",
        Some(session_id.clone()),
        "read_document_aloud",
        RiskLevel::Level2,
        "started",
        json!({
            "document_id": document_id.as_str(),
            "reading_session_id": &reading_session.id,
            "target_language": &target_language,
            "chunks": chunks.len(),
        }),
    );

    let pipeline = TranslatedReadingPipeline::new(
        Arc::new(OllamaDocumentTranslator {
            infer: state.infer.clone(),
            request_id,
        }),
        Arc::new(PiperDocumentTts {
            piper: piper.clone(),
        }),
        Arc::new(PiperDocumentAudioSink {
            piper,
            tts_gate: state.tts_gate.clone(),
        }),
        Arc::new(DaemonReadingProgressStore {
            documents: Arc::clone(&state.documents),
        }),
    );

    let summary = pipeline
        .run(document_id.clone(), reading_session.clone(), chunks)
        .await?;
    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({
            "document_id": document_id.as_str(),
            "reading_session_id": &summary.session_id,
            "chunks_played": summary.chunks_played,
        }),
    );

    Ok(JobResult::DocumentStatus {
        request_id,
        document_id: Some(document_id.as_str().to_string()),
        reading_session_id: Some(summary.session_id),
        chunks: Some(summary.chunks_played),
        message: format!("Leitura concluída: {title}."),
        spoken: true,
    })
}

async fn process_document_control(state: &AppState, job: DocumentControlJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let session_id = ensure_request_session(state, request_id).await;
    let tool_name = match job.control {
        DocumentControlKind::Pause => "pause_reading",
        DocumentControlKind::Resume => "resume_reading",
        DocumentControlKind::Stop => "stop_reading",
    };
    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-{tool_name}"),
            tool_name,
            json!({"reading_session_id": &job.reading_session_id}),
        ),
        RiskContext::user_initiated(),
    )?;

    let status = match job.control {
        DocumentControlKind::Pause => ReadingStatus::Paused,
        DocumentControlKind::Resume => ReadingStatus::Reading,
        DocumentControlKind::Stop => ReadingStatus::Stopped,
    };
    {
        let mut documents = state.documents.lock().await;
        let session = documents
            .reading_sessions
            .get_mut(&job.reading_session_id)
            .with_context(|| {
                format!("reading session `{}` was not found", job.reading_session_id)
            })?;
        match job.control {
            DocumentControlKind::Pause => session.pause(),
            DocumentControlKind::Resume => session.resume(),
            DocumentControlKind::Stop => session.stop(),
        }
        documents.persist()?;
    }

    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({"reading_session_id": &job.reading_session_id, "status": format!("{status:?}")}),
    );

    Ok(JobResult::DocumentStatus {
        request_id,
        document_id: None,
        reading_session_id: Some(job.reading_session_id),
        chunks: None,
        message: format!("Sessão de leitura marcada como {status:?}."),
        spoken: false,
    })
}

async fn process_document_ask(state: &AppState, job: DocumentAskJob) -> Result<JobResult> {
    let request_id = job.request_id;
    let question = job.question.trim();
    if question.is_empty() {
        anyhow::bail!("document question cannot be empty");
    }
    let session_id = ensure_request_session(state, request_id).await;
    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-document-ask"),
            "ask_document",
            json!({"document_id": &job.document_id, "question": question}),
        ),
        RiskContext::user_initiated(),
    )?;

    let (document_id, title, chunks, stored_embeddings) = {
        let documents = state.documents.lock().await;
        let ingested = documents.documents.get(&job.document_id).with_context(|| {
            format!(
                "document `{}` was not ingested in this daemon session",
                job.document_id
            )
        })?;
        (
            ingested.document.id.as_str().to_string(),
            ingested.document.title.clone(),
            ingested.chunks.clone(),
            documents.embeddings.get(&job.document_id).cloned(),
        )
    };
    let selected_chunks = select_document_context_with_optional_embeddings(
        state,
        request_id,
        &document_id,
        &chunks,
        stored_embeddings.as_deref(),
        question,
        DocumentContextLimits {
            max_chunks: 4,
            max_chars: 12_000,
        },
    )
    .await;
    let context = document_context_text(&selected_chunks);
    let prompt = document_question_prompt(&title, question, &context);
    let output = state
        .infer
        .infer_from_text(
            format!("{request_id}-document-ask"),
            Action::Explain,
            Some("document".to_string()),
            prompt,
        )
        .await?;
    let answer = sanitize_output(&Action::Explain, &output.text);
    let result_text = format!(
        "Documento: {title}\nPergunta: {question}\n\nResposta:\n{}",
        answer.trim()
    );
    state.clipboard.set_text(&result_text)?;

    let speech_text = sanitize_for_speech(&Action::Explain, &answer);
    let (tts_enqueue_ms, spoken) = enqueue_tts(
        state.piper.as_ref(),
        &state.tts_gate,
        request_id,
        "AskDocument",
        &speech_text,
        None,
        job.speak,
    );

    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({
            "document_id": &document_id,
            "question_chars": question.chars().count(),
            "context_chunks": selected_chunks.len(),
            "answer_chars": answer.chars().count(),
            "spoken": spoken,
            "tts_enqueue_ms": tts_enqueue_ms,
        }),
    );
    state.audit_log.record_tool_event(
        "document.question_answered",
        Some(session_id),
        "ask_document",
        RiskLevel::Level1,
        "answered",
        json!({
            "document_id": &document_id,
            "context_chunks": selected_chunks.len(),
            "answer_chars": answer.chars().count(),
        }),
    );

    Ok(JobResult::ClipboardText {
        request_id,
        text: result_text,
        spoken,
    })
}

async fn process_document_summarize(
    state: &AppState,
    job: DocumentSummarizeJob,
) -> Result<JobResult> {
    let request_id = job.request_id;
    let session_id = ensure_request_session(state, request_id).await;
    let authorized_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-document-summarize"),
            "summarize_document",
            json!({"document_id": &job.document_id}),
        ),
        RiskContext::user_initiated(),
    )?;

    let (document_id, title, selected_chunks, total_chunks) = {
        let documents = state.documents.lock().await;
        let ingested = documents.documents.get(&job.document_id).with_context(|| {
            format!(
                "document `{}` was not ingested in this daemon session",
                job.document_id
            )
        })?;
        (
            ingested.document.id.as_str().to_string(),
            ingested.document.title.clone(),
            select_document_prefix(&ingested.chunks, 6, 16_000),
            ingested.chunks.len(),
        )
    };
    let context = document_context_text(&selected_chunks);
    let prompt = document_summary_prompt(&title, &context, total_chunks);
    let output = state
        .infer
        .infer_from_text(
            format!("{request_id}-document-summary"),
            Action::Explain,
            Some("document".to_string()),
            prompt,
        )
        .await?;
    let summary = sanitize_output(&Action::Explain, &output.text);
    let result_text = format!("Documento: {title}\n\nResumo:\n{}", summary.trim());
    state.clipboard.set_text(&result_text)?;

    let speech_text = sanitize_for_speech(&Action::Explain, &summary);
    let (tts_enqueue_ms, spoken) = enqueue_tts(
        state.piper.as_ref(),
        &state.tts_gate,
        request_id,
        "SummarizeDocument",
        &speech_text,
        None,
        job.speak,
    );

    record_tool_executed(
        state,
        &session_id,
        &authorized_tool,
        json!({
            "document_id": &document_id,
            "context_chunks": selected_chunks.len(),
            "total_chunks": total_chunks,
            "summary_chars": summary.chars().count(),
            "spoken": spoken,
            "tts_enqueue_ms": tts_enqueue_ms,
        }),
    );
    state.audit_log.record_tool_event(
        "document.summarized",
        Some(session_id),
        "summarize_document",
        RiskLevel::Level1,
        "summarized",
        json!({
            "document_id": &document_id,
            "context_chunks": selected_chunks.len(),
            "total_chunks": total_chunks,
            "summary_chars": summary.chars().count(),
        }),
    );

    Ok(JobResult::ClipboardText {
        request_id,
        text: result_text,
        spoken,
    })
}

fn normalize_document_target_language(target_language: &str) -> Result<String> {
    let target = target_language.trim();
    if target.is_empty() {
        return Ok("pt-BR".to_string());
    }
    ensure_supported_document_target_language(target)?;
    Ok("pt-BR".to_string())
}

fn ensure_supported_document_target_language(target_language: &str) -> Result<()> {
    let normalized = target_language
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    if matches!(
        normalized.as_str(),
        "pt" | "pt-br" | "portuguese" | "portugues" | "português"
    ) {
        return Ok(());
    }
    anyhow::bail!(
        "document translation currently supports only pt-BR; requested `{}`",
        target_language
    );
}

async fn generate_document_embeddings(
    state: &AppState,
    request_id: uuid::Uuid,
    document_id: &str,
    chunks: &[DocumentChunk],
) -> Option<Vec<DocumentChunkEmbedding>> {
    if !state.infer.has_embedding_model() || chunks.is_empty() {
        return None;
    }

    let mut embeddings = Vec::with_capacity(chunks.len());
    for (batch_index, batch) in chunks.chunks(16).enumerate() {
        let texts = batch
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>();
        let output = match state
            .infer
            .embed_texts(format!("{request_id}-document-embed-{batch_index}"), texts)
            .await
        {
            Ok(output) => output,
            Err(error) => {
                warn!(
                    ?error,
                    document_id,
                    batch_index,
                    "document embeddings failed; lexical retrieval remains available"
                );
                return None;
            }
        };

        for (chunk, vector) in batch.iter().zip(output.vectors) {
            embeddings.push(DocumentChunkEmbedding {
                chunk_id: chunk.id.clone(),
                chunk_index: chunk.chunk_index,
                vector,
            });
        }
    }

    if embeddings.len() == chunks.len() {
        info!(
            request_id = %request_id,
            document_id,
            chunks = chunks.len(),
            "document embeddings generated"
        );
        Some(embeddings)
    } else {
        warn!(
            request_id = %request_id,
            document_id,
            expected_chunks = chunks.len(),
            actual_embeddings = embeddings.len(),
            "document embeddings count mismatch; lexical retrieval remains available"
        );
        None
    }
}

async fn select_document_context_with_optional_embeddings(
    state: &AppState,
    request_id: uuid::Uuid,
    document_id: &str,
    chunks: &[DocumentChunk],
    embeddings: Option<&[DocumentChunkEmbedding]>,
    query: &str,
    limits: DocumentContextLimits,
) -> Vec<DocumentChunk> {
    if state.infer.has_embedding_model() {
        if let Some(embeddings) = embeddings {
            match state
                .infer
                .embed_texts(
                    format!("{request_id}-document-query-embed"),
                    vec![query.to_string()],
                )
                .await
            {
                Ok(output) => {
                    if let Some(query_vector) = output.vectors.first() {
                        if let Some(selected) = select_document_context_by_embedding(
                            chunks,
                            embeddings,
                            query_vector,
                            limits.max_chunks,
                            limits.max_chars,
                        ) {
                            info!(
                                request_id = %request_id,
                                document_id,
                                context_chunks = selected.len(),
                                "selected document context with embeddings"
                            );
                            return selected;
                        }
                    }
                }
                Err(error) => {
                    warn!(
                        ?error,
                        document_id,
                        "document query embedding failed; lexical retrieval remains available"
                    );
                }
            }
        }
    }

    select_document_context(chunks, query, limits.max_chunks, limits.max_chars)
}

fn select_document_context_by_embedding(
    chunks: &[DocumentChunk],
    embeddings: &[DocumentChunkEmbedding],
    query_vector: &[f32],
    max_chunks: usize,
    max_chars: usize,
) -> Option<Vec<DocumentChunk>> {
    if chunks.is_empty() || embeddings.is_empty() || query_vector.is_empty() {
        return None;
    }

    let chunks_by_id = chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect::<HashMap<_, _>>();
    let mut scored = embeddings
        .iter()
        .filter_map(|embedding| {
            let chunk = chunks_by_id.get(embedding.chunk_id.as_str())?;
            let score = cosine_similarity(query_vector, &embedding.vector);
            (score > 0.0 && score.is_finite()).then_some((score, *chunk))
        })
        .collect::<Vec<_>>();

    if scored.is_empty() {
        return None;
    }

    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .total_cmp(left_score)
            .then_with(|| left.chunk_index.cmp(&right.chunk_index))
    });

    let selected = collect_context_chunks(
        scored
            .into_iter()
            .map(|(_, chunk)| chunk)
            .collect::<Vec<_>>(),
        max_chunks,
        max_chars,
    );
    (!selected.is_empty()).then_some(selected)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }

    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        return 0.0;
    }

    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn select_document_context(
    chunks: &[DocumentChunk],
    query: &str,
    max_chunks: usize,
    max_chars: usize,
) -> Vec<DocumentChunk> {
    let terms = document_terms(query);
    if terms.is_empty() {
        return select_document_prefix(chunks, max_chunks, max_chars);
    }

    let mut scored = chunks
        .iter()
        .map(|chunk| (document_chunk_score(chunk, &terms), chunk))
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.chunk_index.cmp(&right.chunk_index))
    });

    let ranked = scored
        .into_iter()
        .map(|(_, chunk)| chunk)
        .collect::<Vec<_>>();
    let selected = collect_context_chunks(ranked, max_chunks, max_chars);
    if selected.is_empty() {
        select_document_prefix(chunks, max_chunks, max_chars)
    } else {
        selected
    }
}

fn select_document_prefix(
    chunks: &[DocumentChunk],
    max_chunks: usize,
    max_chars: usize,
) -> Vec<DocumentChunk> {
    collect_context_chunks(chunks.iter().collect::<Vec<_>>(), max_chunks, max_chars)
}

fn collect_context_chunks(
    chunks: Vec<&DocumentChunk>,
    max_chunks: usize,
    max_chars: usize,
) -> Vec<DocumentChunk> {
    let mut selected = Vec::new();
    let mut used_chars = 0_usize;
    let max_chunks = max_chunks.max(1);
    let max_chars = max_chars.max(512);

    for chunk in chunks {
        if selected.len() >= max_chunks {
            break;
        }
        let chunk_chars = chunk.text.chars().count();
        if !selected.is_empty() && used_chars.saturating_add(chunk_chars) > max_chars {
            continue;
        }
        used_chars = used_chars.saturating_add(chunk_chars);
        selected.push(chunk.clone());
    }

    selected.sort_by_key(|chunk| chunk.chunk_index);
    selected
}

fn document_terms(text: &str) -> Vec<String> {
    let mut terms = text
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.chars().count() >= 3)
        .map(|term| term.to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    terms.sort();
    terms
}

fn document_chunk_score(chunk: &DocumentChunk, terms: &[String]) -> usize {
    let text = chunk.text.to_lowercase();
    let title = chunk
        .section_title
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    terms
        .iter()
        .map(|term| {
            let text_score = text.matches(term).count();
            let title_score = if title.contains(term) { 2 } else { 0 };
            text_score + title_score
        })
        .sum()
}

fn document_context_text(chunks: &[DocumentChunk]) -> String {
    chunks
        .iter()
        .map(|chunk| {
            let title = chunk
                .section_title
                .as_deref()
                .map(|value| format!(" | seção: {value}"))
                .unwrap_or_default();
            format!(
                "[chunk {}{}]\n{}",
                chunk.chunk_index,
                title,
                chunk.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn document_question_prompt(title: &str, question: &str, context: &str) -> String {
    format!(
        "Você é um assistente local de leitura de documentos. Responda em PT-BR usando somente os trechos fornecidos. Se os trechos não contiverem a resposta, diga isso claramente e sugira qual parte do documento consultar.\n\nDocumento: {title}\nPergunta: {question}\n\nTrechos relevantes:\n{context}\n\nResposta:"
    )
}

fn document_summary_prompt(title: &str, context: &str, total_chunks: usize) -> String {
    format!(
        "Você é um assistente local de leitura de documentos. Faça um resumo em PT-BR, claro e fiel ao conteúdo fornecido. Não invente conteúdo ausente. Se o contexto for parcial, indique que o resumo cobre apenas os trechos carregados.\n\nDocumento: {title}\nTotal de chunks no documento: {total_chunks}\n\nTrechos para resumir:\n{context}\n\nResumo:"
    )
}

#[cfg(feature = "coddy-protocol")]
struct DaemonReplNativeServices;

#[cfg(feature = "coddy-protocol")]
impl coddy_bridge::ReplNativeServices for DaemonReplNativeServices {
    fn sanitize_search_query(&self, query: &str) -> String {
        sanitize_output(&Action::SearchWeb, query)
    }

    fn answer_repl_question<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        command_text: String,
        speak: bool,
    ) -> coddy_bridge::ReplJobFuture<'a> {
        Box::pin(async move {
            let output = state
                .infer
                .answer_repl_turn(format!("{request_id}-repl-agent"), &command_text)
                .await?;
            let answer = sanitize_output(&Action::Explain, &output.text);
            let speak_requested = state
                .config
                .action_should_speak(Action::Explain.as_str(), speak);
            let speech_text = sanitize_for_speech(&Action::Explain, &answer);
            let (_, spoken) = enqueue_tts(
                state.piper.as_ref(),
                &state.tts_gate,
                request_id,
                Action::Explain.as_str(),
                &speech_text,
                None,
                speak_requested,
            );

            Ok(JobResult::ClipboardText {
                request_id,
                text: answer,
                spoken,
            })
        })
    }

    fn search_web<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        query: String,
        speak: bool,
        total_started_at: Instant,
    ) -> coddy_bridge::ReplJobFuture<'a> {
        Box::pin(async move {
            let speak_requested = state
                .config
                .action_should_speak(Action::SearchWeb.as_str(), speak);
            let search_result =
                execute_search_query(state, request_id, &query, speak_requested, total_started_at)
                    .await?;

            Ok(JobResult::BrowserQuery {
                request_id,
                query,
                summary: search_result.summary,
                spoken: search_result.spoken,
            })
        })
    }

    fn open_application<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        transcript: String,
        app_name: String,
        speak: bool,
    ) -> coddy_bridge::ReplJobFuture<'a> {
        Box::pin(async move {
            process_open_application(
                state,
                ApplicationLaunchJob {
                    request_id,
                    transcript: Some(transcript),
                    app_name,
                    speak,
                },
            )
            .await
        })
    }

    fn open_url<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        transcript: String,
        label: String,
        url: String,
        speak: bool,
    ) -> coddy_bridge::ReplJobFuture<'a> {
        Box::pin(async move {
            process_open_url(
                state,
                UrlOpenJob {
                    request_id,
                    transcript: Some(transcript),
                    label,
                    url,
                    speak,
                },
            )
            .await
        })
    }

    fn voice_search<'a>(
        &'a self,
        state: &'a AppState,
        request_id: uuid::Uuid,
        transcript: String,
        query: String,
        speak: bool,
    ) -> coddy_bridge::ReplJobFuture<'a> {
        Box::pin(async move {
            process_voice_search(
                state,
                VoiceSearchJob {
                    request_id,
                    transcript,
                    query,
                    speak,
                },
            )
            .await
        })
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

#[derive(Debug, Clone)]
struct AuthorizedTool {
    name: String,
    risk_level: RiskLevel,
}

async fn ensure_request_session(state: &AppState, request_id: uuid::Uuid) -> SessionId {
    let session_id = SessionId::from_request_id(request_id);
    let mut sessions = state.sessions.lock().await;
    sessions.ensure_session(session_id.clone(), "pt-BR");
    session_id
}

fn authorize_tool_call(
    state: &AppState,
    session_id: &SessionId,
    call: ToolCall,
    context: RiskContext,
) -> Result<AuthorizedTool> {
    let definition = match state.tools.validate_call(&call) {
        Ok(definition) => definition,
        Err(error) => {
            state.audit_log.record_tool_event(
                "security.blocked",
                Some(session_id.clone()),
                call.name,
                RiskLevel::Level5,
                "schema_rejected",
                json!({"error": error.to_string()}),
            );
            anyhow::bail!("tool call rejected: {error}");
        }
    };

    state.audit_log.record_tool_event(
        "tool.proposed",
        Some(session_id.clone()),
        definition.name.clone(),
        definition.risk_level,
        "proposed",
        json!({"arguments": redact_for_audit(&call.arguments)}),
    );

    let policy_input = PolicyInput {
        tool_name: definition.name.clone(),
        risk_level: definition.risk_level,
        permissions: definition.permissions.clone(),
        confirmation: definition.confirmation,
        arguments: call.arguments,
        context,
    };

    match state.permission_engine.evaluate(&policy_input) {
        PolicyDecision::Allow => {
            state.audit_log.record_tool_event(
                "tool.confirmed",
                Some(session_id.clone()),
                definition.name.clone(),
                definition.risk_level,
                "auto_allowed",
                json!({}),
            );
            Ok(AuthorizedTool {
                name: definition.name.clone(),
                risk_level: definition.risk_level,
            })
        }
        PolicyDecision::RequireConfirmation(request) => {
            state.audit_log.record_tool_event(
                "tool.confirmation_requested",
                Some(session_id.clone()),
                definition.name.clone(),
                definition.risk_level,
                "require_confirmation",
                json!({"reason": request.reason}),
            );
            anyhow::bail!(
                "tool `{}` requires confirmation before execution",
                definition.name
            );
        }
        PolicyDecision::Deny(reason) => {
            state.audit_log.record_tool_event(
                "tool.denied",
                Some(session_id.clone()),
                definition.name.clone(),
                definition.risk_level,
                "deny",
                json!({"reason": reason.to_string()}),
            );
            anyhow::bail!("tool `{}` denied by policy: {reason}", definition.name);
        }
    }
}

fn record_tool_executed(
    state: &AppState,
    session_id: &SessionId,
    tool: &AuthorizedTool,
    data: serde_json::Value,
) {
    state.audit_log.record_tool_event(
        "tool.executed",
        Some(session_id.clone()),
        tool.name.clone(),
        tool.risk_level,
        "executed",
        redact_for_audit(&data),
    );
}

fn record_tool_failed(
    state: &AppState,
    session_id: &SessionId,
    tool_name: &str,
    risk_level: RiskLevel,
    error: &str,
) {
    state.audit_log.record_tool_event(
        "tool.failed",
        Some(session_id.clone()),
        tool_name.to_string(),
        risk_level,
        "failed",
        json!({"error": error}),
    );
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
    let session_id = ensure_request_session(state, request_id).await;
    let capture_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-capture-screen-context"),
            "capture_screen_context",
            json!({"mode": "screenshot_ocr"}),
        ),
        RiskContext::user_initiated(),
    )?;

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
            record_tool_executed(
                state,
                &session_id,
                &capture_tool,
                json!({"action": action_name, "mode": inference_mode, "output_chars": output_chars}),
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
            record_tool_executed(
                state,
                &session_id,
                &capture_tool,
                json!({"action": action_name, "mode": inference_mode, "output_chars": output_chars}),
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
    let session_id = ensure_request_session(state, request_id).await;
    let search_tool = authorize_tool_call(
        state,
        &session_id,
        ToolCall::new(
            format!("{request_id}-search-web"),
            "search_web",
            json!({
                "query": query,
                "max_results": state.config.search.max_results.clamp(1, 10),
            }),
        ),
        RiskContext::user_initiated(),
    )?;
    let mut search_fetch_ms = 0_u64;
    let mut search_summary = None;
    let mut search_spoken_text = None;
    let mut search_result_count = 0_usize;
    let mut ai_overview_chars = 0_usize;
    let mut search_blocked = false;

    #[cfg(feature = "coddy-protocol")]
    coddy_bridge::record_search_started(state, request_id, query, "google").await;

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

    #[cfg(feature = "coddy-protocol")]
    {
        coddy_bridge::record_search_context_extracted(
            state,
            request_id,
            if state.search.is_some() {
                "google"
            } else {
                "disabled"
            },
            search_result_count,
            ai_overview_chars > 0,
        )
        .await;
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
    record_tool_executed(
        state,
        &session_id,
        &search_tool,
        json!({
            "query": query,
            "result_count": search_result_count,
            "ai_overview_chars": ai_overview_chars,
            "spoken": spoken,
        }),
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

#[cfg(feature = "coddy-protocol")]
fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn document_context_selection_prefers_matching_chunks() {
        let chunks = test_document_chunks(&[
            "Introdução geral sobre o livro.",
            "Capítulo de redes com VPN, DNS e configuração de Wi-Fi.",
            "Apêndice sobre atalhos de teclado.",
        ]);

        let selected = select_document_context(&chunks, "Como configurar VPN?", 1, 4_000);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].chunk_index, 1);
    }

    #[test]
    fn document_context_selection_falls_back_to_prefix_without_terms() {
        let chunks = test_document_chunks(&["Primeiro trecho.", "Segundo trecho."]);

        let selected = select_document_context(&chunks, "??", 2, 4_000);

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].chunk_index, 0);
        assert_eq!(selected[1].chunk_index, 1);
    }

    #[test]
    fn document_context_selection_prefers_embedding_match() {
        let chunks = test_document_chunks(&[
            "Introdução geral sobre o livro.",
            "Capítulo de redes com VPN, DNS e configuração de Wi-Fi.",
            "Apêndice sobre atalhos de teclado.",
        ]);
        let embeddings = test_document_embeddings(
            &chunks,
            &[
                vec![1.0, 0.0, 0.0],
                vec![0.0, 1.0, 0.0],
                vec![0.0, 0.0, 1.0],
            ],
        );

        let selected =
            select_document_context_by_embedding(&chunks, &embeddings, &[0.0, 0.9, 0.1], 1, 4_000)
                .expect("semantic context");

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].chunk_index, 1);
    }

    #[test]
    fn document_context_selection_rejects_unusable_embeddings() {
        let chunks = test_document_chunks(&["Primeiro trecho.", "Segundo trecho."]);
        let embeddings = test_document_embeddings(&chunks, &[vec![0.0, 0.0], vec![1.0, 0.0]]);

        assert!(
            select_document_context_by_embedding(&chunks, &embeddings, &[0.0, 0.0], 1, 4_000)
                .is_none()
        );
        assert!(
            select_document_context_by_embedding(&chunks, &embeddings, &[1.0], 1, 4_000).is_none()
        );
    }

    #[test]
    fn cosine_similarity_handles_normalized_and_zero_vectors() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < f32::EPSILON);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    fn document_store_persists_and_reloads_snapshot() {
        let config = AppConfig::default();
        let storage_path = std::env::temp_dir().join(format!(
            "visionclip-document-store-test-{}.json",
            uuid::Uuid::new_v4()
        ));
        let document_id = visionclip_documents::DocumentId::new();
        let chunks = test_document_chunks_with_id(&document_id, &["Persisted chunk."]);
        let mut store = DocumentStore::empty(&config, storage_path.clone());
        store.documents.insert(
            document_id.as_str().to_string(),
            IngestedDocument {
                document: visionclip_documents::LoadedDocument {
                    id: document_id.clone(),
                    source_path: PathBuf::from("/tmp/persisted.txt"),
                    title: "persisted".into(),
                    format: visionclip_documents::DocumentFormat::Text,
                    text: "Persisted chunk.".into(),
                },
                chunks,
            },
        );
        store.embeddings.insert(
            document_id.as_str().to_string(),
            vec![DocumentChunkEmbedding {
                chunk_id: "chunk_0".into(),
                chunk_index: 0,
                vector: vec![0.2, 0.8],
            }],
        );

        store.persist().unwrap();
        let loaded = DocumentStore::load_from_path(&config, storage_path.clone()).unwrap();

        assert!(loaded.documents.contains_key(document_id.as_str()));
        assert_eq!(
            loaded
                .embeddings
                .get(document_id.as_str())
                .and_then(|embeddings| embeddings.first())
                .map(|embedding| embedding.vector.as_slice()),
            Some(&[0.2, 0.8][..])
        );
        let _ = std::fs::remove_file(storage_path);
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

    fn test_document_chunks(texts: &[&str]) -> Vec<DocumentChunk> {
        let document_id = visionclip_documents::DocumentId::new();
        test_document_chunks_with_id(&document_id, texts)
    }

    fn test_document_chunks_with_id(
        document_id: &visionclip_documents::DocumentId,
        texts: &[&str],
    ) -> Vec<DocumentChunk> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| DocumentChunk {
                id: format!("chunk_{index}"),
                document_id: document_id.clone(),
                chunk_index: index,
                page_start: None,
                page_end: None,
                section_title: None,
                text: (*text).to_string(),
                token_count: text.split_whitespace().count(),
            })
            .collect()
    }

    fn test_document_embeddings(
        chunks: &[DocumentChunk],
        vectors: &[Vec<f32>],
    ) -> Vec<DocumentChunkEmbedding> {
        chunks
            .iter()
            .zip(vectors.iter())
            .map(|(chunk, vector)| DocumentChunkEmbedding {
                chunk_id: chunk.id.clone(),
                chunk_index: chunk.chunk_index,
                vector: vector.clone(),
            })
            .collect()
    }
}
