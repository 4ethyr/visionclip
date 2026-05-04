use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryInto,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{sleep, Duration},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new() -> Self {
        Self(format!("doc_{}", Uuid::new_v4()))
    }

    pub fn from_existing(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            bail!("document id cannot be empty");
        }
        Ok(Self(value))
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
    Pdf,
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
    let text = match format {
        DocumentFormat::Text | DocumentFormat::Markdown => std::fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read document `{}`", canonical.display()))?,
        DocumentFormat::Pdf => extract_pdf_text(&canonical)?,
    };
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
        Some("pdf") => Ok(DocumentFormat::Pdf),
        Some(other) => bail!("unsupported document extension `{other}`"),
        None => bail!("document path has no extension"),
    }
}

fn extract_pdf_text(path: &Path) -> Result<String> {
    extract_pdf_text_with_extractors(path, Path::new("pdftotext"), Path::new("mutool"))
}

fn extract_pdf_text_with_extractors(
    path: &Path,
    pdftotext: &Path,
    mutool: &Path,
) -> Result<String> {
    let mut failures = Vec::new();
    match extract_pdf_text_with_pdftotext(path, pdftotext) {
        Ok(text) => return Ok(text),
        Err(error) => failures.push(format!("pdftotext: {error:#}")),
    }

    match extract_pdf_text_with_mutool(path, mutool) {
        Ok(text) => return Ok(text),
        Err(error) => failures.push(format!("mutool: {error:#}")),
    }

    bail!(
        "failed to extract PDF text from `{}`; install poppler-utils (`pdftotext`) or mupdf-tools (`mutool`). Attempts:\n- {}",
        path.display(),
        failures.join("\n- ")
    );
}

fn extract_pdf_text_with_pdftotext(path: &Path, extractor: &Path) -> Result<String> {
    let output = Command::new(extractor)
        .args(["-layout", "-enc", "UTF-8"])
        .arg(path)
        .arg("-")
        .output()
        .with_context(|| {
            format!(
                "failed to execute `pdftotext`; install poppler-utils to ingest PDF `{}`",
                path.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`pdftotext` failed for `{}` with status {}: {}",
            path.display(),
            output.status,
            stderr.trim()
        );
    }

    let text = normalize_pdf_text(
        &String::from_utf8(output.stdout)
            .context("pdftotext returned non-UTF-8 output despite UTF-8 encoding request")?,
    );
    if text.trim().is_empty() {
        bail!(
            "PDF text extraction produced no text for `{}`; scanned PDFs need OCR support",
            path.display()
        );
    }
    Ok(text)
}

fn extract_pdf_text_with_mutool(path: &Path, extractor: &Path) -> Result<String> {
    let output = Command::new(extractor)
        .args(["draw", "-q", "-F", "txt", "-o", "-"])
        .arg(path)
        .output()
        .with_context(|| {
            format!(
                "failed to execute `mutool`; install mupdf-tools to ingest PDF `{}`",
                path.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`mutool` failed for `{}` with status {}: {}",
            path.display(),
            output.status,
            stderr.trim()
        );
    }

    let text = normalize_pdf_text(
        &String::from_utf8(output.stdout)
            .context("mutool returned non-UTF-8 output for PDF text extraction")?,
    );
    if text.trim().is_empty() {
        bail!(
            "PDF text extraction produced no text for `{}`; scanned PDFs need OCR support",
            path.display()
        );
    }
    Ok(text)
}

fn normalize_pdf_text(input: &str) -> String {
    let input = input.replace('\u{0c}', "\n\n");
    let mut normalized = Vec::new();
    let mut previous_blank = false;
    for line in input.lines().map(str::trim_end) {
        let blank = line.trim().is_empty();
        if blank && previous_blank {
            continue;
        }
        normalized.push(line);
        previous_blank = blank;
    }
    normalized.join("\n").trim().to_string()
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
    pub chunk_id: String,
    pub chunk_index: usize,
    pub target_language: String,
    pub voice_id: Option<String>,
    pub text: String,
    pub bytes: Vec<u8>,
    pub duration_ms: Option<u64>,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioCacheEntry {
    pub document_id: DocumentId,
    pub session_id: String,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub target_language: String,
    pub voice_id: Option<String>,
    pub text: String,
    pub bytes: Vec<u8>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioCacheLookup {
    pub document_id: DocumentId,
    pub session_id: String,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub target_language: String,
    pub voice_id: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadingProgress {
    pub session_id: String,
    pub document_id: DocumentId,
    pub current_chunk_index: usize,
    pub status: ReadingStatus,
}

#[derive(Debug)]
pub struct SqliteDocumentStore {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredChunkEmbedding {
    pub document_id: DocumentId,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub model: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAudioChunk {
    pub document_id: DocumentId,
    pub chunk_id: String,
    pub chunk_index: usize,
    pub target_language: String,
    pub voice_id: String,
    pub text_hash: String,
    pub audio_path: PathBuf,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAuditEvent {
    pub id: String,
    pub captured_at_unix_ms: u64,
    pub session_id: Option<String>,
    pub event_type: String,
    pub risk_level: Option<u8>,
    pub tool_name: Option<String>,
    pub provider: Option<String>,
    pub decision: Option<String>,
    pub data_json: String,
}

impl SqliteDocumentStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        Self::from_connection(
            Connection::open(path)
                .with_context(|| format!("failed to open SQLite store {}", path.display()))?,
        )
    }

    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch(SQLITE_SCHEMA)?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('store_version', '1')",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        let value: String = self.conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'store_version'",
            [],
            |row| row.get(0),
        )?;
        value
            .parse::<u32>()
            .context("failed to parse SQLite document store version")
    }

    pub fn save_document(&mut self, ingested: &IngestedDocument) -> Result<()> {
        let tx = self.conn.transaction()?;
        let now = now_ms();
        tx.execute(
            "INSERT INTO documents (id, source_path, title, format, text, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
               source_path = excluded.source_path,
               title = excluded.title,
               format = excluded.format,
               text = excluded.text,
               updated_at_ms = excluded.updated_at_ms",
            params![
                ingested.document.id.as_str(),
                ingested.document.source_path.display().to_string(),
                &ingested.document.title,
                encode_document_format(ingested.document.format),
                &ingested.document.text,
                now,
            ],
        )?;
        tx.execute(
            "DELETE FROM document_chunks WHERE document_id = ?1",
            params![ingested.document.id.as_str()],
        )?;
        for chunk in &ingested.chunks {
            tx.execute(
                "INSERT INTO document_chunks (
                   id, document_id, chunk_index, page_start, page_end, section_title, text, token_count
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &chunk.id,
                    chunk.document_id.as_str(),
                    usize_to_i64(chunk.chunk_index, "chunk_index")?,
                    chunk.page_start.map(i64::from),
                    chunk.page_end.map(i64::from),
                    &chunk.section_title,
                    &chunk.text,
                    usize_to_i64(chunk.token_count, "token_count")?,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_document(&self, document_id: &str) -> Result<Option<IngestedDocument>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, source_path, title, format, text FROM documents WHERE id = ?1",
                params![document_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;

        let Some((id, source_path, title, format, text)) = row else {
            return Ok(None);
        };

        let document_id = DocumentId::from_existing(id)?;
        let document = LoadedDocument {
            id: document_id.clone(),
            source_path: PathBuf::from(source_path),
            title,
            format: decode_document_format(&format)?,
            text,
        };
        let chunks = self.load_chunks(document_id.as_str())?;
        Ok(Some(IngestedDocument { document, chunks }))
    }

    pub fn load_documents(&self) -> Result<Vec<IngestedDocument>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM documents ORDER BY created_at_ms ASC, id ASC")?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        ids.into_iter()
            .map(|id| {
                self.load_document(&id)?
                    .with_context(|| format!("document `{id}` disappeared while loading SQLite"))
            })
            .collect()
    }

    pub fn save_reading_session(&mut self, session: &ReadingSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reading_sessions (
               id, document_id, target_language, current_chunk_index, status, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               document_id = excluded.document_id,
               target_language = excluded.target_language,
               current_chunk_index = excluded.current_chunk_index,
               status = excluded.status,
               updated_at_ms = excluded.updated_at_ms",
            params![
                &session.id,
                session.document_id.as_str(),
                &session.target_language,
                usize_to_i64(session.current_chunk_index, "current_chunk_index")?,
                encode_reading_status(session.status),
                now_ms(),
            ],
        )?;
        Ok(())
    }

    pub fn load_reading_session(&self, session_id: &str) -> Result<Option<ReadingSession>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, document_id, target_language, current_chunk_index, status
                 FROM reading_sessions WHERE id = ?1",
                params![session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;

        let Some((id, document_id, target_language, current_chunk_index, status)) = row else {
            return Ok(None);
        };
        Ok(Some(ReadingSession {
            id,
            document_id: DocumentId::from_existing(document_id)?,
            target_language,
            current_chunk_index: i64_to_usize(current_chunk_index, "current_chunk_index")?,
            status: decode_reading_status(&status)?,
        }))
    }

    pub fn load_reading_sessions(&self) -> Result<Vec<ReadingSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, document_id, target_language, current_chunk_index, status
             FROM reading_sessions
             ORDER BY updated_at_ms ASC, id ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut sessions = Vec::new();
        while let Some(row) = rows.next()? {
            sessions.push(ReadingSession {
                id: row.get(0)?,
                document_id: DocumentId::from_existing(row.get::<_, String>(1)?)?,
                target_language: row.get(2)?,
                current_chunk_index: i64_to_usize(row.get(3)?, "current_chunk_index")?,
                status: decode_reading_status(&row.get::<_, String>(4)?)?,
            });
        }
        Ok(sessions)
    }

    pub fn save_progress(&mut self, progress: &ReadingProgress) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reading_progress (
               session_id, document_id, current_chunk_index, status, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id) DO UPDATE SET
               document_id = excluded.document_id,
               current_chunk_index = excluded.current_chunk_index,
               status = excluded.status,
               updated_at_ms = excluded.updated_at_ms",
            params![
                &progress.session_id,
                progress.document_id.as_str(),
                usize_to_i64(progress.current_chunk_index, "current_chunk_index")?,
                encode_reading_status(progress.status),
                now_ms(),
            ],
        )?;
        Ok(())
    }

    pub fn load_progress(&self, session_id: &str) -> Result<Option<ReadingProgress>> {
        let row = self
            .conn
            .query_row(
                "SELECT session_id, document_id, current_chunk_index, status
                 FROM reading_progress WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((session_id, document_id, current_chunk_index, status)) = row else {
            return Ok(None);
        };
        Ok(Some(ReadingProgress {
            session_id,
            document_id: DocumentId::from_existing(document_id)?,
            current_chunk_index: i64_to_usize(current_chunk_index, "current_chunk_index")?,
            status: decode_reading_status(&status)?,
        }))
    }

    pub fn load_all_progress(&self) -> Result<Vec<ReadingProgress>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, document_id, current_chunk_index, status
             FROM reading_progress
             ORDER BY updated_at_ms ASC, session_id ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut progress = Vec::new();
        while let Some(row) = rows.next()? {
            progress.push(ReadingProgress {
                session_id: row.get(0)?,
                document_id: DocumentId::from_existing(row.get::<_, String>(1)?)?,
                current_chunk_index: i64_to_usize(row.get(2)?, "current_chunk_index")?,
                status: decode_reading_status(&row.get::<_, String>(3)?)?,
            });
        }
        Ok(progress)
    }

    pub fn save_translations(&mut self, units: &[TranslatedUnit]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let now = now_ms();
        for unit in units {
            tx.execute(
                "INSERT INTO translated_chunks (
                   session_id, chunk_id, chunk_index, source_text, translated_text,
                   target_language, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(session_id, chunk_id) DO UPDATE SET
                   source_text = excluded.source_text,
                   translated_text = excluded.translated_text,
                   target_language = excluded.target_language",
                params![
                    &unit.session_id,
                    &unit.chunk_id,
                    usize_to_i64(unit.chunk_index, "chunk_index")?,
                    &unit.source_text,
                    &unit.translated_text,
                    &unit.target_language,
                    now,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_translations(&self, session_id: &str) -> Result<Vec<TranslatedUnit>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, chunk_id, chunk_index, source_text, translated_text, target_language
             FROM translated_chunks
             WHERE session_id = ?1
             ORDER BY chunk_index ASC",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        let mut units = Vec::new();
        while let Some(row) = rows.next()? {
            units.push(TranslatedUnit {
                session_id: row.get(0)?,
                chunk_id: row.get(1)?,
                chunk_index: i64_to_usize(row.get(2)?, "chunk_index")?,
                source_text: row.get(3)?,
                translated_text: row.get(4)?,
                target_language: row.get(5)?,
            });
        }
        Ok(units)
    }

    pub fn load_translations_for_document(&self, document_id: &str) -> Result<Vec<TranslatedUnit>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.session_id, t.chunk_id, t.chunk_index, t.source_text, t.translated_text, t.target_language
             FROM translated_chunks t
             INNER JOIN document_chunks c ON c.id = t.chunk_id
             WHERE c.document_id = ?1
             ORDER BY t.chunk_index ASC",
        )?;
        let mut rows = stmt.query(params![document_id])?;
        let mut units = Vec::new();
        while let Some(row) = rows.next()? {
            units.push(TranslatedUnit {
                session_id: row.get(0)?,
                chunk_id: row.get(1)?,
                chunk_index: i64_to_usize(row.get(2)?, "chunk_index")?,
                source_text: row.get(3)?,
                translated_text: row.get(4)?,
                target_language: row.get(5)?,
            });
        }
        Ok(units)
    }

    pub fn save_embeddings(&mut self, embeddings: &[StoredChunkEmbedding]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let now = now_ms();
        for embedding in embeddings {
            tx.execute(
                "INSERT INTO chunk_embeddings (
                   document_id, chunk_id, chunk_index, model, vector, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(document_id, chunk_id, model) DO UPDATE SET
                   chunk_index = excluded.chunk_index,
                   vector = excluded.vector,
                   updated_at_ms = excluded.updated_at_ms",
                params![
                    embedding.document_id.as_str(),
                    &embedding.chunk_id,
                    usize_to_i64(embedding.chunk_index, "chunk_index")?,
                    &embedding.model,
                    encode_vector(&embedding.vector),
                    now,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_embeddings(
        &self,
        document_id: &str,
        model: &str,
    ) -> Result<Vec<StoredChunkEmbedding>> {
        let mut stmt = self.conn.prepare(
            "SELECT document_id, chunk_id, chunk_index, model, vector
             FROM chunk_embeddings
             WHERE document_id = ?1 AND model = ?2
             ORDER BY chunk_index ASC",
        )?;
        let mut rows = stmt.query(params![document_id, model])?;
        let mut embeddings = Vec::new();
        while let Some(row) = rows.next()? {
            embeddings.push(StoredChunkEmbedding {
                document_id: DocumentId::from_existing(row.get::<_, String>(0)?)?,
                chunk_id: row.get(1)?,
                chunk_index: i64_to_usize(row.get(2)?, "chunk_index")?,
                model: row.get(3)?,
                vector: decode_vector(&row.get::<_, Vec<u8>>(4)?)?,
            });
        }
        Ok(embeddings)
    }

    pub fn save_audio_chunk(&mut self, chunk: &StoredAudioChunk) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audio_chunks (
               document_id, chunk_id, chunk_index, target_language, voice_id,
               text_hash, audio_path, duration_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(document_id, chunk_id, target_language, voice_id, text_hash) DO UPDATE SET
               chunk_index = excluded.chunk_index,
               audio_path = excluded.audio_path,
               duration_ms = excluded.duration_ms,
               updated_at_ms = excluded.updated_at_ms",
            params![
                chunk.document_id.as_str(),
                &chunk.chunk_id,
                usize_to_i64(chunk.chunk_index, "chunk_index")?,
                &chunk.target_language,
                &chunk.voice_id,
                &chunk.text_hash,
                chunk.audio_path.display().to_string(),
                chunk
                    .duration_ms
                    .map(|value| u64_to_i64(value, "duration_ms"))
                    .transpose()?,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    pub fn load_audio_chunks(
        &self,
        document_id: &str,
        target_language: &str,
        voice_id: &str,
    ) -> Result<Vec<StoredAudioChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT document_id, chunk_id, chunk_index, target_language, voice_id,
                    text_hash, audio_path, duration_ms
             FROM audio_chunks
             WHERE document_id = ?1 AND target_language = ?2 AND voice_id = ?3
             ORDER BY chunk_index ASC",
        )?;
        let mut rows = stmt.query(params![document_id, target_language, voice_id])?;
        let mut chunks = Vec::new();
        while let Some(row) = rows.next()? {
            chunks.push(StoredAudioChunk {
                document_id: DocumentId::from_existing(row.get::<_, String>(0)?)?,
                chunk_id: row.get(1)?,
                chunk_index: i64_to_usize(row.get(2)?, "chunk_index")?,
                target_language: row.get(3)?,
                voice_id: row.get(4)?,
                text_hash: row.get(5)?,
                audio_path: PathBuf::from(row.get::<_, String>(6)?),
                duration_ms: row
                    .get::<_, Option<i64>>(7)?
                    .map(|value| i64_to_u64(value, "duration_ms"))
                    .transpose()?,
            });
        }
        Ok(chunks)
    }

    pub fn save_audit_event(&mut self, event: &StoredAuditEvent) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_events (
               id, captured_at_unix_ms, session_id, event_type, risk_level,
               tool_name, provider, decision, data_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               captured_at_unix_ms = excluded.captured_at_unix_ms,
               session_id = excluded.session_id,
               event_type = excluded.event_type,
               risk_level = excluded.risk_level,
               tool_name = excluded.tool_name,
               provider = excluded.provider,
               decision = excluded.decision,
               data_json = excluded.data_json",
            params![
                &event.id,
                u64_to_i64(event.captured_at_unix_ms, "captured_at_unix_ms")?,
                &event.session_id,
                &event.event_type,
                event.risk_level.map(i64::from),
                &event.tool_name,
                &event.provider,
                &event.decision,
                &event.data_json,
            ],
        )?;
        Ok(())
    }

    pub fn load_audit_events(&self, limit: usize) -> Result<Vec<StoredAuditEvent>> {
        let limit = usize_to_i64(limit.max(1), "limit")?;
        let mut stmt = self.conn.prepare(
            "SELECT id, captured_at_unix_ms, session_id, event_type, risk_level,
                    tool_name, provider, decision, data_json
             FROM audit_events
             ORDER BY captured_at_unix_ms ASC, id ASC
             LIMIT ?1",
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            events.push(StoredAuditEvent {
                id: row.get(0)?,
                captured_at_unix_ms: i64_to_u64(row.get(1)?, "captured_at_unix_ms")?,
                session_id: row.get(2)?,
                event_type: row.get(3)?,
                risk_level: row
                    .get::<_, Option<i64>>(4)?
                    .map(|value| i64_to_u8(value, "risk_level"))
                    .transpose()?,
                tool_name: row.get(5)?,
                provider: row.get(6)?,
                decision: row.get(7)?,
                data_json: row.get(8)?,
            });
        }
        Ok(events)
    }

    fn load_chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, document_id, chunk_index, page_start, page_end, section_title, text, token_count
             FROM document_chunks
             WHERE document_id = ?1
             ORDER BY chunk_index ASC",
        )?;
        let mut rows = stmt.query(params![document_id])?;
        let mut chunks = Vec::new();
        while let Some(row) = rows.next()? {
            chunks.push(DocumentChunk {
                id: row.get(0)?,
                document_id: DocumentId::from_existing(row.get::<_, String>(1)?)?,
                chunk_index: i64_to_usize(row.get(2)?, "chunk_index")?,
                page_start: row
                    .get::<_, Option<i64>>(3)?
                    .map(|value| i64_to_u32(value, "page_start"))
                    .transpose()?,
                page_end: row
                    .get::<_, Option<i64>>(4)?
                    .map(|value| i64_to_u32(value, "page_end"))
                    .transpose()?,
                section_title: row.get(5)?,
                text: row.get(6)?,
                token_count: i64_to_usize(row.get(7)?, "token_count")?,
            });
        }
        Ok(chunks)
    }
}

const SQLITE_SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY,
  source_path TEXT NOT NULL,
  title TEXT NOT NULL,
  format TEXT NOT NULL,
  text TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS document_chunks (
  id TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  page_start INTEGER,
  page_end INTEGER,
  section_title TEXT,
  text TEXT NOT NULL,
  token_count INTEGER NOT NULL,
  UNIQUE(document_id, chunk_index)
);

CREATE TABLE IF NOT EXISTS reading_sessions (
  id TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  target_language TEXT NOT NULL,
  current_chunk_index INTEGER NOT NULL,
  status TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS reading_progress (
  session_id TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  current_chunk_index INTEGER NOT NULL,
  status TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS translated_chunks (
  session_id TEXT NOT NULL,
  chunk_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  source_text TEXT NOT NULL,
  translated_text TEXT NOT NULL,
  target_language TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY(session_id, chunk_id)
);

CREATE TABLE IF NOT EXISTS chunk_embeddings (
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  model TEXT NOT NULL,
  vector BLOB NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(document_id, chunk_id, model)
);

CREATE TABLE IF NOT EXISTS audio_chunks (
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  target_language TEXT NOT NULL,
  voice_id TEXT NOT NULL,
  text_hash TEXT NOT NULL,
  audio_path TEXT NOT NULL,
  duration_ms INTEGER,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(document_id, chunk_id, target_language, voice_id, text_hash)
);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  captured_at_unix_ms INTEGER NOT NULL,
  session_id TEXT,
  event_type TEXT NOT NULL,
  risk_level INTEGER,
  tool_name TEXT,
  provider TEXT,
  decision TEXT,
  data_json TEXT NOT NULL
);
"#;

fn encode_document_format(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Text => "text",
        DocumentFormat::Markdown => "markdown",
        DocumentFormat::Pdf => "pdf",
    }
}

fn decode_document_format(value: &str) -> Result<DocumentFormat> {
    match value {
        "text" => Ok(DocumentFormat::Text),
        "markdown" => Ok(DocumentFormat::Markdown),
        "pdf" => Ok(DocumentFormat::Pdf),
        other => bail!("unknown document format `{other}`"),
    }
}

fn encode_reading_status(status: ReadingStatus) -> &'static str {
    match status {
        ReadingStatus::Idle => "idle",
        ReadingStatus::Reading => "reading",
        ReadingStatus::Paused => "paused",
        ReadingStatus::Stopped => "stopped",
        ReadingStatus::Completed => "completed",
    }
}

fn decode_reading_status(value: &str) -> Result<ReadingStatus> {
    match value {
        "idle" => Ok(ReadingStatus::Idle),
        "reading" => Ok(ReadingStatus::Reading),
        "paused" => Ok(ReadingStatus::Paused),
        "stopped" => Ok(ReadingStatus::Stopped),
        "completed" => Ok(ReadingStatus::Completed),
        other => bail!("unknown reading status `{other}`"),
    }
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn decode_vector(bytes: &[u8]) -> Result<Vec<f32>> {
    let chunks = bytes.chunks_exact(4);
    if !chunks.remainder().is_empty() {
        bail!("invalid embedding vector byte length {}", bytes.len());
    }

    chunks
        .map(|chunk| {
            let bytes: [u8; 4] = chunk
                .try_into()
                .context("failed to decode embedding vector bytes")?;
            Ok(f32::from_le_bytes(bytes))
        })
        .collect()
}

fn usize_to_i64(value: usize, field: &str) -> Result<i64> {
    i64::try_from(value).with_context(|| format!("{field} is too large for SQLite integer"))
}

fn i64_to_usize(value: i64, field: &str) -> Result<usize> {
    usize::try_from(value).with_context(|| format!("{field} is negative or too large"))
}

fn i64_to_u32(value: i64, field: &str) -> Result<u32> {
    u32::try_from(value).with_context(|| format!("{field} is negative or too large"))
}

fn u64_to_i64(value: u64, field: &str) -> Result<i64> {
    i64::try_from(value).with_context(|| format!("{field} is too large for SQLite integer"))
}

fn i64_to_u64(value: i64, field: &str) -> Result<u64> {
    u64::try_from(value).with_context(|| format!("{field} is negative or too large"))
}

fn i64_to_u8(value: i64, field: &str) -> Result<u8> {
    u8::try_from(value).with_context(|| format!("{field} is negative or too large"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslatedReadingConfig {
    pub chunk_buffer: usize,
    pub translation_buffer: usize,
    pub audio_buffer: usize,
    pub control_poll_interval_ms: u64,
}

impl Default for TranslatedReadingConfig {
    fn default() -> Self {
        Self {
            chunk_buffer: 8,
            translation_buffer: 8,
            audio_buffer: 4,
            control_poll_interval_ms: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslatedReadingSummary {
    pub session_id: String,
    pub chunks_played: usize,
    pub last_chunk_index: Option<usize>,
    pub status: ReadingStatus,
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
pub trait AudioCacheStore: Send + Sync {
    async fn load_audio_chunk(&self, _lookup: AudioCacheLookup) -> Result<Option<AudioChunk>> {
        Ok(None)
    }

    async fn save_audio_chunk(&self, entry: AudioCacheEntry) -> Result<()>;
}

#[async_trait]
pub trait ReadingProgressStore: Send + Sync {
    async fn save_progress(&self, progress: ReadingProgress) -> Result<()>;

    async fn load_status(&self, _session_id: &str) -> Result<Option<ReadingStatus>> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct TranslatedReadingPipeline<T, S, A, P> {
    translator: Arc<T>,
    tts: Arc<S>,
    audio: Arc<A>,
    progress: Arc<P>,
    audio_cache: Option<Arc<dyn AudioCacheStore>>,
    voice_id: Option<String>,
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
            audio_cache: None,
            voice_id: None,
            config: TranslatedReadingConfig::default(),
        }
    }

    pub fn with_audio_cache<C>(mut self, audio_cache: Arc<C>) -> Self
    where
        C: AudioCacheStore + 'static,
    {
        self.audio_cache = Some(audio_cache);
        self
    }

    pub fn with_voice_id(mut self, voice_id: Option<String>) -> Self {
        self.voice_id = voice_id.filter(|value| !value.trim().is_empty());
        self
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
        let tts = spawn_tts_worker(
            Arc::clone(&self.tts),
            translation_rx,
            audio_tx,
            document_id.clone(),
            session.id.clone(),
            self.voice_id.clone(),
            self.audio_cache.clone(),
        );

        let mut chunks_played = 0;
        let mut last_chunk_index = None;
        let mut interrupted_status = None;
        let control_poll_interval =
            Duration::from_millis(self.config.control_poll_interval_ms.max(1));
        while let Some(audio_chunk) = audio_rx.recv().await {
            let control_status = wait_for_reading_control(
                self.progress.as_ref(),
                &session.id,
                &document_id,
                audio_chunk.chunk_index,
                control_poll_interval,
            )
            .await?;
            if matches!(
                control_status,
                ReadingStatus::Stopped | ReadingStatus::Completed
            ) {
                interrupted_status = Some(control_status);
                break;
            }

            self.audio.play(audio_chunk.clone()).await?;
            if !audio_chunk.cached {
                if let Some(audio_cache) = &self.audio_cache {
                    audio_cache
                        .save_audio_chunk(AudioCacheEntry {
                            document_id: document_id.clone(),
                            session_id: session.id.clone(),
                            chunk_id: audio_chunk.chunk_id.clone(),
                            chunk_index: audio_chunk.chunk_index,
                            target_language: audio_chunk.target_language.clone(),
                            voice_id: audio_chunk.voice_id.clone(),
                            text: audio_chunk.text.clone(),
                            bytes: audio_chunk.bytes.clone(),
                            duration_ms: audio_chunk.duration_ms,
                        })
                        .await?;
                }
            }
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

        if let Some(status) = interrupted_status {
            producer.abort();
            translator.abort();
            tts.abort();
            return Ok(TranslatedReadingSummary {
                session_id,
                chunks_played,
                last_chunk_index,
                status,
            });
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
            status: ReadingStatus::Completed,
        })
    }
}

async fn wait_for_reading_control<P>(
    progress: &P,
    session_id: &str,
    document_id: &DocumentId,
    current_chunk_index: usize,
    poll_interval: Duration,
) -> Result<ReadingStatus>
where
    P: ReadingProgressStore + ?Sized,
{
    loop {
        match progress
            .load_status(session_id)
            .await?
            .unwrap_or(ReadingStatus::Reading)
        {
            ReadingStatus::Idle | ReadingStatus::Reading => return Ok(ReadingStatus::Reading),
            ReadingStatus::Paused => {
                progress
                    .save_progress(ReadingProgress {
                        session_id: session_id.to_string(),
                        document_id: document_id.clone(),
                        current_chunk_index,
                        status: ReadingStatus::Paused,
                    })
                    .await?;
                sleep(poll_interval).await;
            }
            ReadingStatus::Stopped => {
                progress
                    .save_progress(ReadingProgress {
                        session_id: session_id.to_string(),
                        document_id: document_id.clone(),
                        current_chunk_index,
                        status: ReadingStatus::Stopped,
                    })
                    .await?;
                return Ok(ReadingStatus::Stopped);
            }
            ReadingStatus::Completed => return Ok(ReadingStatus::Completed),
        }
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
    document_id: DocumentId,
    session_id: String,
    voice_id: Option<String>,
    audio_cache: Option<Arc<dyn AudioCacheStore>>,
) -> JoinHandle<Result<()>>
where
    S: TtsProvider + 'static,
{
    tokio::spawn(async move {
        while let Some(unit) = rx.recv().await {
            let request_voice_id = voice_id.clone();
            if let Some(audio_cache) = &audio_cache {
                if let Some(audio_chunk) = audio_cache
                    .load_audio_chunk(AudioCacheLookup {
                        document_id: document_id.clone(),
                        session_id: session_id.clone(),
                        chunk_id: unit.chunk_id.clone(),
                        chunk_index: unit.chunk_index,
                        target_language: unit.target_language.clone(),
                        voice_id: request_voice_id.clone(),
                        text: unit.translated_text.clone(),
                    })
                    .await?
                {
                    tx.send(audio_chunk)
                        .await
                        .context("failed to send cached audio chunk")?;
                    continue;
                }
            }

            let bytes = tts
                .synthesize(TtsRequest {
                    chunk_index: unit.chunk_index,
                    text: unit.translated_text.clone(),
                    voice_id: request_voice_id.clone(),
                })
                .await?;
            tx.send(AudioChunk {
                id: format!("audio_{}", Uuid::new_v4()),
                chunk_id: unit.chunk_id,
                chunk_index: unit.chunk_index,
                target_language: unit.target_language,
                voice_id: request_voice_id,
                text: unit.translated_text,
                bytes,
                duration_ms: None,
                cached: false,
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
    use std::collections::VecDeque;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

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
    fn document_format_supports_pdf_files() {
        assert_eq!(
            document_format(Path::new("book.pdf")).unwrap(),
            DocumentFormat::Pdf
        );
    }

    #[test]
    fn pdf_text_normalization_replaces_page_breaks() {
        assert_eq!(
            normalize_pdf_text("Page one  \n\n\u{0c}Page two\n"),
            "Page one\n\nPage two"
        );
    }

    #[test]
    fn pdf_loader_uses_fixed_pdftotext_arguments() {
        let temp_dir = std::env::temp_dir().join(format!(
            "visionclip-pdf-extractor-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let script_path = temp_dir.join("pdftotext-test");
        let args_path = temp_dir.join("args.txt");
        let pdf_path = temp_dir.join("book.pdf");
        std::fs::write(&pdf_path, b"%PDF test placeholder").unwrap();
        std::fs::write(
            &script_path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'First page\\fSecond page\\n'\n",
                args_path.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script_path, permissions).unwrap();

        let text = extract_pdf_text_with_pdftotext(&pdf_path, &script_path).unwrap();
        let args = std::fs::read_to_string(&args_path).unwrap();

        assert_eq!(text, "First page\n\nSecond page");
        assert_eq!(
            args,
            format!("-layout\n-enc\nUTF-8\n{}\n-\n", pdf_path.display())
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn pdf_loader_falls_back_to_mutool_when_pdftotext_is_missing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "visionclip-pdf-mutool-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let missing_pdftotext = temp_dir.join("missing-pdftotext");
        let mutool_path = temp_dir.join("mutool-test");
        let args_path = temp_dir.join("mutool-args.txt");
        let pdf_path = temp_dir.join("book.pdf");
        std::fs::write(&pdf_path, b"%PDF test placeholder").unwrap();
        std::fs::write(
            &mutool_path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'Chapter one\\fChapter two\\n'\n",
                args_path.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&mutool_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&mutool_path, permissions).unwrap();

        let text =
            extract_pdf_text_with_extractors(&pdf_path, &missing_pdftotext, &mutool_path).unwrap();
        let args = std::fs::read_to_string(&args_path).unwrap();

        assert_eq!(text, "Chapter one\n\nChapter two");
        assert_eq!(
            args,
            format!("draw\n-q\n-F\ntxt\n-o\n-\n{}\n", pdf_path.display())
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn sqlite_store_persists_document_chunks_and_progress() {
        let mut store = SqliteDocumentStore::in_memory().unwrap();
        let ingested = test_ingested_document(&["Intro text.", "Second chunk about VPN."]);
        let document_id = ingested.document.id.clone();
        store.save_document(&ingested).unwrap();

        let loaded = store
            .load_document(document_id.as_str())
            .unwrap()
            .expect("stored document");
        let all_documents = store.load_documents().unwrap();
        assert_eq!(loaded.document.title, "book");
        assert_eq!(loaded.chunks.len(), 2);
        assert_eq!(loaded.chunks[1].text, "Second chunk about VPN.");
        assert_eq!(all_documents.len(), 1);
        assert_eq!(store.schema_version().unwrap(), 1);

        let mut session = ReadingSession::new(document_id.clone(), "pt-BR");
        session.start();
        session.current_chunk_index = 1;
        store.save_reading_session(&session).unwrap();
        let loaded_session = store
            .load_reading_session(&session.id)
            .unwrap()
            .expect("stored reading session");
        let all_sessions = store.load_reading_sessions().unwrap();
        assert_eq!(loaded_session.status, ReadingStatus::Reading);
        assert_eq!(loaded_session.current_chunk_index, 1);
        assert_eq!(all_sessions.len(), 1);

        let progress = ReadingProgress {
            session_id: session.id.clone(),
            document_id,
            current_chunk_index: 2,
            status: ReadingStatus::Completed,
        };
        store.save_progress(&progress).unwrap();
        let loaded_progress = store
            .load_progress(&session.id)
            .unwrap()
            .expect("stored progress");
        let all_progress = store.load_all_progress().unwrap();
        assert_eq!(loaded_progress.current_chunk_index, 2);
        assert_eq!(loaded_progress.status, ReadingStatus::Completed);
        assert_eq!(all_progress.len(), 1);
    }

    #[test]
    fn sqlite_store_persists_translations_and_embeddings() {
        let mut store = SqliteDocumentStore::in_memory().unwrap();
        let ingested = test_ingested_document(&["Source one.", "Source two."]);
        let document_id = ingested.document.id.clone();
        let chunks = ingested.chunks.clone();
        store.save_document(&ingested).unwrap();

        let translations = chunks
            .iter()
            .map(|chunk| TranslatedUnit {
                session_id: "read_test".into(),
                chunk_id: chunk.id.clone(),
                chunk_index: chunk.chunk_index,
                source_text: chunk.text.clone(),
                translated_text: format!("Traduzido {}", chunk.chunk_index),
                target_language: "pt-BR".into(),
            })
            .collect::<Vec<_>>();
        store.save_translations(&translations).unwrap();
        let loaded_translations = store.load_translations("read_test").unwrap();
        let loaded_document_translations = store
            .load_translations_for_document(document_id.as_str())
            .unwrap();
        assert_eq!(loaded_translations, translations);
        assert_eq!(loaded_document_translations, translations);

        let embeddings = chunks
            .iter()
            .map(|chunk| StoredChunkEmbedding {
                document_id: document_id.clone(),
                chunk_id: chunk.id.clone(),
                chunk_index: chunk.chunk_index,
                model: "nomic-embed-text".into(),
                vector: vec![chunk.chunk_index as f32, 0.5, 1.0],
            })
            .collect::<Vec<_>>();
        store.save_embeddings(&embeddings).unwrap();
        let loaded_embeddings = store
            .load_embeddings(document_id.as_str(), "nomic-embed-text")
            .unwrap();

        assert_eq!(loaded_embeddings, embeddings);
    }

    #[test]
    fn sqlite_store_persists_audio_cache_entries() {
        let mut store = SqliteDocumentStore::in_memory().unwrap();
        let ingested = test_ingested_document(&["Source one.", "Source two."]);
        let document_id = ingested.document.id.clone();
        let chunk = ingested.chunks[0].clone();
        store.save_document(&ingested).unwrap();

        let audio = StoredAudioChunk {
            document_id: document_id.clone(),
            chunk_id: chunk.id,
            chunk_index: chunk.chunk_index,
            target_language: "pt-BR".into(),
            voice_id: "pt_BR-faber-medium".into(),
            text_hash: "sha256:abc".into(),
            audio_path: PathBuf::from("/tmp/visionclip-audio.wav"),
            duration_ms: Some(1234),
        };
        store.save_audio_chunk(&audio).unwrap();

        let loaded = store
            .load_audio_chunks(document_id.as_str(), "pt-BR", "pt_BR-faber-medium")
            .unwrap();

        assert_eq!(loaded, vec![audio]);
    }

    #[test]
    fn sqlite_store_persists_audit_events() {
        let mut store = SqliteDocumentStore::in_memory().unwrap();
        let first = StoredAuditEvent {
            id: "evt_1".into(),
            captured_at_unix_ms: 10,
            session_id: Some("sess_1".into()),
            event_type: "tool.executed".into(),
            risk_level: Some(2),
            tool_name: Some("ingest_document".into()),
            provider: Some("ollama/local".into()),
            decision: Some("allow".into()),
            data_json: "{\"document_id\":\"doc_1\"}".into(),
        };
        let second = StoredAuditEvent {
            id: "evt_2".into(),
            captured_at_unix_ms: 20,
            session_id: None,
            event_type: "security.blocked".into(),
            risk_level: Some(5),
            tool_name: Some("run_safe_command".into()),
            provider: None,
            decision: Some("deny".into()),
            data_json: "{}".into(),
        };

        store.save_audit_event(&second).unwrap();
        store.save_audit_event(&first).unwrap();

        assert_eq!(store.load_audit_events(1).unwrap(), vec![first.clone()]);
        assert_eq!(store.load_audit_events(10).unwrap(), vec![first, second]);
    }

    #[test]
    fn sqlite_store_rejects_invalid_existing_document_id() {
        let error = DocumentId::from_existing(" ").unwrap_err();
        assert!(error.to_string().contains("cannot be empty"));
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
        assert_eq!(summary.status, ReadingStatus::Completed);
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

    #[tokio::test]
    async fn translated_reading_pipeline_writes_audio_cache_entries() {
        let document_id = DocumentId::new();
        let chunks = (0..2)
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
        let cache = Arc::new(RecordingAudioCacheStore::default());
        let pipeline = TranslatedReadingPipeline::new(
            Arc::new(EchoTranslator),
            Arc::new(TextBytesTts),
            audio,
            progress,
        )
        .with_voice_id(Some("pt_BR-test".into()))
        .with_audio_cache(Arc::clone(&cache));

        let summary = pipeline
            .run(document_id.clone(), session, chunks)
            .await
            .unwrap();

        assert_eq!(summary.chunks_played, 2);
        let saved = cache.saved.lock().unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].document_id, document_id);
        assert_eq!(saved[0].chunk_id, "chunk_0");
        assert_eq!(saved[0].chunk_index, 0);
        assert_eq!(saved[0].target_language, "pt-BR");
        assert_eq!(saved[0].voice_id.as_deref(), Some("pt_BR-test"));
        assert_eq!(saved[0].text, "[pt-BR] source 0");
        assert_eq!(saved[0].bytes, b"[pt-BR] source 0");
    }

    #[tokio::test]
    async fn translated_reading_pipeline_uses_audio_cache_before_tts() {
        let document_id = DocumentId::new();
        let chunks = vec![DocumentChunk {
            id: "chunk_0".into(),
            document_id: document_id.clone(),
            chunk_index: 0,
            page_start: None,
            page_end: None,
            section_title: None,
            text: "source 0".into(),
            token_count: 2,
        }];
        let session = ReadingSession::new(document_id.clone(), "pt-BR");
        let audio = Arc::new(RecordingAudioSink::default());
        let progress = Arc::new(RecordingProgressStore::default());
        let cache = Arc::new(CacheHitAudioStore::default());
        let tts = Arc::new(CountingTts::default());
        let pipeline = TranslatedReadingPipeline::new(
            Arc::new(EchoTranslator),
            Arc::clone(&tts),
            Arc::clone(&audio),
            progress,
        )
        .with_voice_id(Some("pt_BR-test".into()))
        .with_audio_cache(Arc::clone(&cache));

        let summary = pipeline.run(document_id, session, chunks).await.unwrap();

        assert_eq!(summary.chunks_played, 1);
        assert_eq!(tts.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            audio.played.lock().unwrap().as_slice(),
            &[(0, "cached audio text".to_string())]
        );
        assert!(cache.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn translated_reading_pipeline_waits_while_paused() {
        let document_id = DocumentId::new();
        let chunks = (0..1)
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
        let progress = Arc::new(ControlledProgressStore::new([
            ReadingStatus::Paused,
            ReadingStatus::Reading,
        ]));
        let pipeline = TranslatedReadingPipeline::new(
            Arc::new(EchoTranslator),
            Arc::new(TextBytesTts),
            Arc::clone(&audio),
            Arc::clone(&progress),
        )
        .with_config(TranslatedReadingConfig {
            control_poll_interval_ms: 1,
            ..TranslatedReadingConfig::default()
        });

        let summary = pipeline.run(document_id, session, chunks).await.unwrap();

        assert_eq!(summary.status, ReadingStatus::Completed);
        assert_eq!(
            audio.played.lock().unwrap().as_slice(),
            &[(0, "[pt-BR] source 0".to_string())]
        );
        assert!(progress
            .saved
            .lock()
            .unwrap()
            .iter()
            .any(|progress| progress.status == ReadingStatus::Paused));
    }

    #[tokio::test]
    async fn translated_reading_pipeline_stops_before_next_chunk() {
        let document_id = DocumentId::new();
        let chunks = (0..2)
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
        let progress = Arc::new(ControlledProgressStore::new([
            ReadingStatus::Reading,
            ReadingStatus::Stopped,
        ]));
        let pipeline = TranslatedReadingPipeline::new(
            Arc::new(EchoTranslator),
            Arc::new(TextBytesTts),
            Arc::clone(&audio),
            Arc::clone(&progress),
        )
        .with_config(TranslatedReadingConfig {
            control_poll_interval_ms: 1,
            chunk_buffer: 1,
            translation_buffer: 1,
            audio_buffer: 1,
        });

        let summary = pipeline.run(document_id, session, chunks).await.unwrap();

        assert_eq!(summary.status, ReadingStatus::Stopped);
        assert_eq!(summary.chunks_played, 1);
        assert_eq!(summary.last_chunk_index, Some(0));
        assert_eq!(
            audio.played.lock().unwrap().as_slice(),
            &[(0, "[pt-BR] source 0".to_string())]
        );
        assert_eq!(
            progress.saved.lock().unwrap().last().unwrap().status,
            ReadingStatus::Stopped
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
    struct CountingTts {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl TtsProvider for CountingTts {
        async fn synthesize(&self, request: TtsRequest) -> Result<Vec<u8>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
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
    struct RecordingAudioCacheStore {
        saved: Mutex<Vec<AudioCacheEntry>>,
    }

    #[async_trait]
    impl AudioCacheStore for RecordingAudioCacheStore {
        async fn save_audio_chunk(&self, entry: AudioCacheEntry) -> Result<()> {
            self.saved.lock().unwrap().push(entry);
            Ok(())
        }
    }

    #[derive(Default)]
    struct CacheHitAudioStore {
        saved: Mutex<Vec<AudioCacheEntry>>,
    }

    #[async_trait]
    impl AudioCacheStore for CacheHitAudioStore {
        async fn load_audio_chunk(&self, lookup: AudioCacheLookup) -> Result<Option<AudioChunk>> {
            Ok(Some(AudioChunk {
                id: "audio_cached".into(),
                chunk_id: lookup.chunk_id,
                chunk_index: lookup.chunk_index,
                target_language: lookup.target_language,
                voice_id: lookup.voice_id,
                text: "cached audio text".into(),
                bytes: b"cached-wav".to_vec(),
                duration_ms: Some(7),
                cached: true,
            }))
        }

        async fn save_audio_chunk(&self, entry: AudioCacheEntry) -> Result<()> {
            self.saved.lock().unwrap().push(entry);
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

    struct ControlledProgressStore {
        saved: Mutex<Vec<ReadingProgress>>,
        statuses: Mutex<VecDeque<ReadingStatus>>,
    }

    impl ControlledProgressStore {
        fn new(statuses: impl IntoIterator<Item = ReadingStatus>) -> Self {
            Self {
                saved: Mutex::new(Vec::new()),
                statuses: Mutex::new(statuses.into_iter().collect()),
            }
        }
    }

    #[async_trait]
    impl ReadingProgressStore for ControlledProgressStore {
        async fn save_progress(&self, progress: ReadingProgress) -> Result<()> {
            self.saved.lock().unwrap().push(progress);
            Ok(())
        }

        async fn load_status(&self, _session_id: &str) -> Result<Option<ReadingStatus>> {
            Ok(self.statuses.lock().unwrap().pop_front())
        }
    }

    fn test_ingested_document(texts: &[&str]) -> IngestedDocument {
        let document_id = DocumentId::new();
        let text = texts.join("\n\n");
        let document = LoadedDocument {
            id: document_id.clone(),
            source_path: PathBuf::from("/tmp/book.md"),
            title: "book".into(),
            format: DocumentFormat::Markdown,
            text,
        };
        let chunks = texts
            .iter()
            .enumerate()
            .map(|(index, text)| DocumentChunk {
                id: format!("{}_chunk_{index}", document_id.as_str()),
                document_id: document_id.clone(),
                chunk_index: index,
                page_start: None,
                page_end: None,
                section_title: None,
                text: (*text).to_string(),
                token_count: text.split_whitespace().count(),
            })
            .collect();

        IngestedDocument { document, chunks }
    }
}
