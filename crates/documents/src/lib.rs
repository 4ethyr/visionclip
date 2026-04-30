use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{sync::mpsc, task::JoinHandle};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new() -> Self {
        Self(format!("doc_{}", Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentFormat {
    Text,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoadedDocument {
    pub id: DocumentId,
    pub source_path: PathBuf,
    pub title: String,
    pub format: DocumentFormat,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: DocumentId,
    pub chunk_index: usize,
    pub page_start: Option<u32>,
    pub page_end: Option<u32>,
    pub section_title: Option<String>,
    pub text: String,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkerConfig {
    pub target_chars: usize,
    pub overlap_chars: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_chars: 3_200,
            overlap_chars: 320,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentRuntime {
    chunker: ChunkerConfig,
}

impl DocumentRuntime {
    pub fn new(chunker: ChunkerConfig) -> Self {
        Self { chunker }
    }

    pub fn ingest_path(&self, path: impl AsRef<Path>) -> Result<IngestedDocument> {
        let document = load_document(path)?;
        let chunks = chunk_document(&document, &self.chunker);
        Ok(IngestedDocument { document, chunks })
    }
}

impl Default for DocumentRuntime {
    fn default() -> Self {
        Self::new(ChunkerConfig::default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestedDocument {
    pub document: LoadedDocument,
    pub chunks: Vec<DocumentChunk>,
}

pub fn load_document(path: impl AsRef<Path>) -> Result<LoadedDocument> {
    let path = path.as_ref();
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve document path `{}`", path.display()))?;
    let metadata = canonical
        .metadata()
        .with_context(|| format!("failed to stat document `{}`", canonical.display()))?;
    if !metadata.is_file() {
        bail!("document path is not a file: {}", canonical.display());
    }

    let format = document_format(&canonical)?;
    let text = std::fs::read_to_string(&canonical)
        .with_context(|| format!("failed to read document `{}`", canonical.display()))?;
    if text.trim().is_empty() {
        bail!("document is empty: {}", canonical.display());
    }

    Ok(LoadedDocument {
        id: DocumentId::new(),
        title: canonical
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("document")
            .to_string(),
        source_path: canonical,
        format,
        text,
    })
}

fn document_format(path: &Path) -> Result<DocumentFormat> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("txt") => Ok(DocumentFormat::Text),
        Some("md" | "markdown") => Ok(DocumentFormat::Markdown),
        Some("pdf") => {
            bail!("PDF extraction is not implemented yet; use TXT or Markdown for this MVP")
        }
        Some(other) => bail!("unsupported document extension `{other}`"),
        None => bail!("document path has no extension"),
    }
}

pub fn chunk_document(document: &LoadedDocument, config: &ChunkerConfig) -> Vec<DocumentChunk> {
    let target_chars = config.target_chars.max(256);
    let paragraphs = document
        .text
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>();

    let mut raw_chunks = Vec::new();
    let mut current = String::new();
    for paragraph in paragraphs {
        if paragraph.chars().count() > target_chars {
            push_current_chunk(&mut raw_chunks, &mut current);
            split_large_paragraph(paragraph, target_chars, &mut raw_chunks);
            continue;
        }

        let candidate_len = current.chars().count() + paragraph.chars().count() + 2;
        if !current.is_empty() && candidate_len > target_chars {
            push_current_chunk(&mut raw_chunks, &mut current);
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    push_current_chunk(&mut raw_chunks, &mut current);

    if raw_chunks.is_empty() {
        raw_chunks.push(document.text.trim().to_string());
    }

    raw_chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| DocumentChunk {
            id: format!("{}_chunk_{index}", document.id.as_str()),
            document_id: document.id.clone(),
            chunk_index: index,
            page_start: None,
            page_end: None,
            section_title: section_title(&text),
            token_count: text.split_whitespace().count(),
            text,
        })
        .collect()
}

fn push_current_chunk(chunks: &mut Vec<String>, current: &mut String) {
    let text = current.trim();
    if !text.is_empty() {
        chunks.push(text.to_string());
    }
    current.clear();
}

fn split_large_paragraph(paragraph: &str, target_chars: usize, chunks: &mut Vec<String>) {
    let chars = paragraph.chars().collect::<Vec<_>>();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + target_chars).min(chars.len());
        chunks.push(chars[start..end].iter().collect::<String>());
        start = end;
    }
}

fn section_title(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with('#') && line.chars().any(|ch| ch.is_alphabetic()))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|line| !line.is_empty())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadingStatus {
    Idle,
    Reading,
    Paused,
    Stopped,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadingSession {
    pub id: String,
    pub document_id: DocumentId,
    pub target_language: String,
    pub current_chunk_index: usize,
    pub status: ReadingStatus,
}

impl ReadingSession {
    pub fn new(document_id: DocumentId, target_language: impl Into<String>) -> Self {
        Self {
            id: format!("read_{}", Uuid::new_v4()),
            document_id,
            target_language: target_language.into(),
            current_chunk_index: 0,
            status: ReadingStatus::Idle,
        }
    }

    pub fn start(&mut self) {
        self.status = ReadingStatus::Reading;
    }

    pub fn pause(&mut self) {
        if self.status == ReadingStatus::Reading {
            self.status = ReadingStatus::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.status == ReadingStatus::Paused {
            self.status = ReadingStatus::Reading;
        }
    }

    pub fn stop(&mut self) {
        self.status = ReadingStatus::Stopped;
    }

    pub fn mark_completed(&mut self) {
        self.status = ReadingStatus::Completed;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocReadUnit {
    pub session_id: String,
    pub chunk: DocumentChunk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationRequest {
    pub chunk_index: usize,
    pub source_text: String,
    pub target_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslatedUnit {
    pub session_id: String,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub source_text: String,
    pub translated_text: String,
    pub target_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TtsRequest {
    pub chunk_index: usize,
    pub text: String,
    pub voice_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioChunk {
    pub id: String,
    pub chunk_index: usize,
    pub text: String,
    pub bytes: Vec<u8>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadingProgress {
    pub session_id: String,
    pub document_id: DocumentId,
    pub current_chunk_index: usize,
    pub status: ReadingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslatedReadingConfig {
    pub chunk_buffer: usize,
    pub translation_buffer: usize,
    pub audio_buffer: usize,
}

impl Default for TranslatedReadingConfig {
    fn default() -> Self {
        Self {
            chunk_buffer: 8,
            translation_buffer: 8,
            audio_buffer: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslatedReadingSummary {
    pub session_id: String,
    pub chunks_played: usize,
    pub last_chunk_index: Option<usize>,
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    async fn translate(&self, request: TranslationRequest) -> Result<String>;
}

#[async_trait]
pub trait TtsProvider: Send + Sync {
    async fn synthesize(&self, request: TtsRequest) -> Result<Vec<u8>>;
}

#[async_trait]
pub trait AudioSink: Send + Sync {
    async fn play(&self, chunk: AudioChunk) -> Result<()>;
}

#[async_trait]
pub trait ReadingProgressStore: Send + Sync {
    async fn save_progress(&self, progress: ReadingProgress) -> Result<()>;
}

#[derive(Clone)]
pub struct TranslatedReadingPipeline<T, S, A, P> {
    translator: Arc<T>,
    tts: Arc<S>,
    audio: Arc<A>,
    progress: Arc<P>,
    config: TranslatedReadingConfig,
}

impl<T, S, A, P> TranslatedReadingPipeline<T, S, A, P>
where
    T: TranslationProvider + 'static,
    S: TtsProvider + 'static,
    A: AudioSink + 'static,
    P: ReadingProgressStore + 'static,
{
    pub fn new(translator: Arc<T>, tts: Arc<S>, audio: Arc<A>, progress: Arc<P>) -> Self {
        Self {
            translator,
            tts,
            audio,
            progress,
            config: TranslatedReadingConfig::default(),
        }
    }

    pub fn with_config(mut self, config: TranslatedReadingConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(
        &self,
        document_id: DocumentId,
        session: ReadingSession,
        chunks: Vec<DocumentChunk>,
    ) -> Result<TranslatedReadingSummary> {
        let (chunk_tx, chunk_rx) = mpsc::channel::<DocReadUnit>(self.config.chunk_buffer.max(1));
        let (translation_tx, translation_rx) =
            mpsc::channel::<TranslatedUnit>(self.config.translation_buffer.max(1));
        let (audio_tx, mut audio_rx) = mpsc::channel::<AudioChunk>(self.config.audio_buffer.max(1));

        let session_id = session.id.clone();
        let producer = spawn_chunk_producer(
            chunk_tx,
            session.id.clone(),
            chunks,
            session.current_chunk_index,
        );
        let translator = spawn_translation_worker(
            Arc::clone(&self.translator),
            chunk_rx,
            translation_tx,
            session.target_language.clone(),
        );
        let tts = spawn_tts_worker(Arc::clone(&self.tts), translation_rx, audio_tx);

        let mut chunks_played = 0;
        let mut last_chunk_index = None;
        while let Some(audio_chunk) = audio_rx.recv().await {
            self.audio.play(audio_chunk.clone()).await?;
            chunks_played += 1;
            last_chunk_index = Some(audio_chunk.chunk_index);
            self.progress
                .save_progress(ReadingProgress {
                    session_id: session.id.clone(),
                    document_id: document_id.clone(),
                    current_chunk_index: audio_chunk.chunk_index + 1,
                    status: ReadingStatus::Reading,
                })
                .await?;
        }

        producer
            .await
            .context("document chunk producer task failed")??;
        translator
            .await
            .context("translation worker task failed")??;
        tts.await.context("tts worker task failed")??;

        self.progress
            .save_progress(ReadingProgress {
                session_id: session.id,
                document_id,
                current_chunk_index: last_chunk_index.map(|index| index + 1).unwrap_or(0),
                status: ReadingStatus::Completed,
            })
            .await?;

        Ok(TranslatedReadingSummary {
            session_id,
            chunks_played,
            last_chunk_index,
        })
    }
}

fn spawn_chunk_producer(
    tx: mpsc::Sender<DocReadUnit>,
    session_id: String,
    chunks: Vec<DocumentChunk>,
    start_chunk_index: usize,
) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        for chunk in chunks
            .into_iter()
            .filter(|chunk| chunk.chunk_index >= start_chunk_index)
        {
            tx.send(DocReadUnit {
                session_id: session_id.clone(),
                chunk,
            })
            .await
            .context("failed to send document chunk")?;
        }
        Ok(())
    })
}

fn spawn_translation_worker<T>(
    translator: Arc<T>,
    mut rx: mpsc::Receiver<DocReadUnit>,
    tx: mpsc::Sender<TranslatedUnit>,
    target_language: String,
) -> JoinHandle<Result<()>>
where
    T: TranslationProvider + 'static,
{
    tokio::spawn(async move {
        while let Some(unit) = rx.recv().await {
            let translated = translator
                .translate(TranslationRequest {
                    chunk_index: unit.chunk.chunk_index,
                    source_text: unit.chunk.text.clone(),
                    target_language: target_language.clone(),
                })
                .await?;
            tx.send(TranslatedUnit {
                session_id: unit.session_id,
                chunk_id: unit.chunk.id,
                chunk_index: unit.chunk.chunk_index,
                source_text: unit.chunk.text,
                translated_text: translated,
                target_language: target_language.clone(),
            })
            .await
            .context("failed to send translated chunk")?;
        }
        Ok(())
    })
}

fn spawn_tts_worker<S>(
    tts: Arc<S>,
    mut rx: mpsc::Receiver<TranslatedUnit>,
    tx: mpsc::Sender<AudioChunk>,
) -> JoinHandle<Result<()>>
where
    S: TtsProvider + 'static,
{
    tokio::spawn(async move {
        while let Some(unit) = rx.recv().await {
            let bytes = tts
                .synthesize(TtsRequest {
                    chunk_index: unit.chunk_index,
                    text: unit.translated_text.clone(),
                    voice_id: None,
                })
                .await?;
            tx.send(AudioChunk {
                id: format!("audio_{}", Uuid::new_v4()),
                chunk_index: unit.chunk_index,
                text: unit.translated_text,
                bytes,
                duration_ms: None,
            })
            .await
            .context("failed to send audio chunk")?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn chunks_text_by_paragraphs_with_stable_indices() {
        let first = "First paragraph ".repeat(9);
        let second = "Second paragraph ".repeat(10);
        let third = "Third paragraph ".repeat(9);
        let document = LoadedDocument {
            id: DocumentId::new(),
            source_path: PathBuf::from("/tmp/book.txt"),
            title: "book".into(),
            format: DocumentFormat::Text,
            text: format!("{first}\n\n{second}\n\n{third}"),
        };

        let chunks = chunk_document(
            &document,
            &ChunkerConfig {
                target_chars: 256,
                overlap_chars: 0,
            },
        );

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].chunk_index, 1);
        assert_eq!(chunks[2].chunk_index, 2);
        assert!(chunks[0].id.ends_with("_chunk_0"));
    }

    #[test]
    fn reading_session_supports_pause_resume_and_stop() {
        let mut session = ReadingSession::new(DocumentId::new(), "pt-BR");

        session.start();
        assert_eq!(session.status, ReadingStatus::Reading);
        session.pause();
        assert_eq!(session.status, ReadingStatus::Paused);
        session.resume();
        assert_eq!(session.status, ReadingStatus::Reading);
        session.stop();
        assert_eq!(session.status, ReadingStatus::Stopped);
    }

    #[test]
    fn loader_rejects_pdf_until_pdf_extractor_exists() {
        let error = document_format(Path::new("book.pdf")).unwrap_err();
        assert!(error
            .to_string()
            .contains("PDF extraction is not implemented"));
    }

    #[tokio::test]
    async fn translated_reading_pipeline_processes_chunks_incrementally() {
        let document_id = DocumentId::new();
        let chunks = (0..3)
            .map(|index| DocumentChunk {
                id: format!("chunk_{index}"),
                document_id: document_id.clone(),
                chunk_index: index,
                page_start: None,
                page_end: None,
                section_title: None,
                text: format!("source {index}"),
                token_count: 2,
            })
            .collect::<Vec<_>>();
        let session = ReadingSession::new(document_id.clone(), "pt-BR");
        let audio = Arc::new(RecordingAudioSink::default());
        let progress = Arc::new(RecordingProgressStore::default());
        let pipeline = TranslatedReadingPipeline::new(
            Arc::new(EchoTranslator),
            Arc::new(TextBytesTts),
            Arc::clone(&audio),
            Arc::clone(&progress),
        );

        let summary = pipeline.run(document_id, session, chunks).await.unwrap();

        assert_eq!(summary.chunks_played, 3);
        assert_eq!(summary.last_chunk_index, Some(2));
        assert_eq!(
            audio.played.lock().unwrap().as_slice(),
            &[
                (0, "[pt-BR] source 0".to_string()),
                (1, "[pt-BR] source 1".to_string()),
                (2, "[pt-BR] source 2".to_string()),
            ]
        );
        assert_eq!(
            progress.saved.lock().unwrap().last().unwrap().status,
            ReadingStatus::Completed
        );
    }

    struct EchoTranslator;

    #[async_trait]
    impl TranslationProvider for EchoTranslator {
        async fn translate(&self, request: TranslationRequest) -> Result<String> {
            Ok(format!(
                "[{}] {}",
                request.target_language, request.source_text
            ))
        }
    }

    struct TextBytesTts;

    #[async_trait]
    impl TtsProvider for TextBytesTts {
        async fn synthesize(&self, request: TtsRequest) -> Result<Vec<u8>> {
            Ok(request.text.into_bytes())
        }
    }

    #[derive(Default)]
    struct RecordingAudioSink {
        played: Mutex<Vec<(usize, String)>>,
    }

    #[async_trait]
    impl AudioSink for RecordingAudioSink {
        async fn play(&self, chunk: AudioChunk) -> Result<()> {
            self.played
                .lock()
                .unwrap()
                .push((chunk.chunk_index, chunk.text));
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingProgressStore {
        saved: Mutex<Vec<ReadingProgress>>,
    }

    #[async_trait]
    impl ReadingProgressStore for RecordingProgressStore {
        async fn save_progress(&self, progress: ReadingProgress) -> Result<()> {
            self.saved.lock().unwrap().push(progress);
            Ok(())
        }
    }
}
