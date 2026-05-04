use crate::{
    extractors::ExtractedText,
    query::{parse_query, score_name_path_hit, QueryFilter},
    schema,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq)]
pub struct SearchFileRecord {
    pub path: PathBuf,
    pub canonical_path: PathBuf,
    pub parent_dir: String,
    pub file_name: String,
    pub title: Option<String>,
    pub extension: Option<String>,
    pub mime: Option<String>,
    pub kind: String,
    pub size_bytes: u64,
    pub mtime_ns: Option<i64>,
    pub ctime_ns: Option<i64>,
    pub inode: Option<i64>,
    pub dev: Option<i64>,
    pub indexed_state: String,
    pub sensitivity: String,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchHitRecord {
    pub file_id: i64,
    pub path: PathBuf,
    pub title: String,
    pub kind: String,
    pub score: f32,
    pub source: String,
    pub snippet: Option<String>,
    pub modified_at: Option<i64>,
    pub size_bytes: Option<u64>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CatalogStats {
    pub root_count: usize,
    pub file_count: usize,
    pub chunk_count: usize,
    pub pending_job_count: usize,
    pub failed_job_count: usize,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchAudit {
    pub roots: Vec<String>,
    pub file_count: usize,
    pub chunk_count: usize,
    pub sensitive_skipped_count: usize,
    pub failed_jobs: Vec<String>,
}

pub struct SearchCatalog {
    connection: Connection,
}

impl SearchCatalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        connection.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            "#,
        )?;
        schema::init_schema(&connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch("PRAGMA busy_timeout = 5000;")?;
        schema::init_schema(&connection)?;
        Ok(Self { connection })
    }

    pub fn upsert_root(&self, path: &Path, policy: &str) -> Result<i64> {
        let path = path.display().to_string();
        let now = unix_ms_now();
        self.connection.execute(
            r#"
            INSERT INTO search_roots(path, enabled, added_at, policy)
            VALUES(?1, 1, ?2, ?3)
            ON CONFLICT(path) DO UPDATE SET enabled = 1, policy = excluded.policy
            "#,
            params![path, now, policy],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn remove_root(&self, path: &Path) -> Result<()> {
        self.connection.execute(
            "UPDATE search_roots SET enabled = 0 WHERE path = ?1",
            params![path.display().to_string()],
        )?;
        Ok(())
    }

    pub fn active_roots(&self) -> Result<Vec<PathBuf>> {
        let mut statement = self
            .connection
            .prepare("SELECT path FROM search_roots WHERE enabled = 1 ORDER BY path")?;
        let roots = statement
            .query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(roots)
    }

    pub fn upsert_file(&self, record: &SearchFileRecord) -> Result<i64> {
        self.connection.execute(
            r#"
            INSERT INTO search_files(
              path, canonical_path, parent_dir, file_name, title, extension, mime, kind,
              size_bytes, mtime_ns, ctime_ns, inode, dev, indexed_state, sensitivity, last_seen_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(path) DO UPDATE SET
              canonical_path = excluded.canonical_path,
              parent_dir = excluded.parent_dir,
              file_name = excluded.file_name,
              title = excluded.title,
              extension = excluded.extension,
              mime = excluded.mime,
              kind = excluded.kind,
              size_bytes = excluded.size_bytes,
              mtime_ns = excluded.mtime_ns,
              ctime_ns = excluded.ctime_ns,
              inode = excluded.inode,
              dev = excluded.dev,
              indexed_state = excluded.indexed_state,
              sensitivity = excluded.sensitivity,
              last_seen_at = excluded.last_seen_at,
              last_error = NULL
            "#,
            params![
                record.path.display().to_string(),
                record.canonical_path.display().to_string(),
                record.parent_dir,
                record.file_name,
                record.title,
                record.extension,
                record.mime,
                record.kind,
                record.size_bytes as i64,
                record.mtime_ns,
                record.ctime_ns,
                record.inode,
                record.dev,
                record.indexed_state,
                record.sensitivity,
                record.last_seen_at,
            ],
        )?;

        let file_id = self.connection.query_row(
            "SELECT file_id FROM search_files WHERE path = ?1",
            params![record.path.display().to_string()],
            |row| row.get(0),
        )?;
        self.upsert_file_fts(file_id, record)?;
        Ok(file_id)
    }

    pub fn record_usage(&self, file_id: i64, action: &str) -> Result<()> {
        let now = unix_ms_now();
        self.connection.execute(
            r#"
            INSERT INTO search_usage(file_id, action, count, last_used_at)
            VALUES(?1, ?2, 1, ?3)
            ON CONFLICT(file_id, action) DO UPDATE SET
              count = count + 1,
              last_used_at = excluded.last_used_at
            "#,
            params![file_id, action, now],
        )?;
        Ok(())
    }

    pub fn file_path(&self, file_id: i64) -> Result<Option<PathBuf>> {
        let path = self
            .connection
            .query_row(
                "SELECT path FROM search_files WHERE file_id = ?1",
                params![file_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(PathBuf::from);
        Ok(path)
    }

    pub fn replace_chunks(&self, file_id: i64, extracted: &ExtractedText) -> Result<()> {
        let existing_chunk_ids = {
            let mut statement = self
                .connection
                .prepare("SELECT chunk_id FROM search_chunks WHERE file_id = ?1")?;
            let chunk_ids = statement
                .query_map(params![file_id], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            chunk_ids
        };
        for chunk_id in existing_chunk_ids {
            self.connection.execute(
                "DELETE FROM search_chunks_fts WHERE rowid = ?1",
                params![chunk_id],
            )?;
        }
        self.connection.execute(
            "DELETE FROM search_chunks WHERE file_id = ?1",
            params![file_id],
        )?;
        for (chunk_index, chunk) in chunk_text(&extracted.text, 4_000).into_iter().enumerate() {
            self.connection.execute(
                r#"
                INSERT INTO search_chunks(
                  file_id, chunk_index, text, text_hash, byte_start, byte_end,
                  token_count, language, source, embedding_id
                )
                VALUES(?1, ?2, ?3, NULL, NULL, NULL, ?4, NULL, ?5, NULL)
                "#,
                params![
                    file_id,
                    chunk_index as i64,
                    chunk,
                    chunk.split_whitespace().count() as i64,
                    extracted.source.as_str(),
                ],
            )?;
            let chunk_id = self.connection.last_insert_rowid();
            self.connection.execute(
                r#"
                INSERT INTO search_chunks_fts(rowid, text, source)
                VALUES(?1, ?2, ?3)
                "#,
                params![chunk_id, chunk, extracted.source.as_str()],
            )?;
        }
        Ok(())
    }

    pub fn search_name_path(
        &self,
        query: &str,
        limit: usize,
        root_hint: Option<&Path>,
    ) -> Result<Vec<SearchHitRecord>> {
        let parsed = parse_query(query);
        if parsed.terms.is_empty() {
            return Ok(Vec::new());
        }
        let filters = SearchFilters::from_query_filters(&parsed.filters);
        let Some(fts_query) = fts_query_from_terms(&parsed.terms) else {
            return self.search_name_path_like(query, limit, root_hint, &parsed, &filters);
        };

        self.search_name_path_fts(query, limit, root_hint, &parsed, &filters, &fts_query)
            .or_else(|_| self.search_name_path_like(query, limit, root_hint, &parsed, &filters))
    }

    fn search_name_path_fts(
        &self,
        query: &str,
        limit: usize,
        root_hint: Option<&Path>,
        parsed: &crate::query::SearchQuery,
        filters: &SearchFilters,
        fts_query: &str,
    ) -> Result<Vec<SearchHitRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT f.file_id, f.path, f.file_name, COALESCE(f.title, f.file_name), f.kind,
                   f.extension, f.mtime_ns, f.size_bytes, f.sensitivity
            FROM search_files_fts
            JOIN search_files f ON f.file_id = search_files_fts.rowid
            WHERE search_files_fts MATCH ?1
              AND f.indexed_state != 'excluded'
            ORDER BY bm25(search_files_fts, 8.0, 5.0, 1.0)
            LIMIT ?2
            "#,
        )?;
        let rows = statement.query_map(params![fts_query, candidate_limit(limit)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;

        self.collect_name_path_hits(rows, query, root_hint, parsed, filters, limit)
    }

    fn search_name_path_like(
        &self,
        query: &str,
        limit: usize,
        root_hint: Option<&Path>,
        parsed: &crate::query::SearchQuery,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHitRecord>> {
        let seed = parsed
            .terms
            .first()
            .map(|term| format!("%{}%", escape_like(term)))
            .unwrap_or_else(|| "%".to_string());
        let mut statement = self.connection.prepare(
            r#"
            SELECT file_id, path, file_name, COALESCE(title, file_name), kind, extension,
                   mtime_ns, size_bytes, sensitivity
            FROM search_files
            WHERE indexed_state != 'excluded'
              AND (file_name LIKE ?1 ESCAPE '\' OR path LIKE ?1 ESCAPE '\')
            ORDER BY last_seen_at DESC
            LIMIT ?2
            "#,
        )?;
        let rows = statement.query_map(params![seed, candidate_limit(limit)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;

        self.collect_name_path_hits(rows, query, root_hint, parsed, filters, limit)
    }

    fn collect_name_path_hits(
        &self,
        rows: impl Iterator<
            Item = rusqlite::Result<(
                i64,
                PathBuf,
                String,
                String,
                String,
                Option<String>,
                Option<i64>,
                i64,
                String,
            )>,
        >,
        query: &str,
        root_hint: Option<&Path>,
        parsed: &crate::query::SearchQuery,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchHitRecord>> {
        let mut hits = Vec::new();
        for row in rows {
            let (
                file_id,
                path,
                file_name,
                title,
                kind,
                extension,
                modified_at,
                size_bytes,
                sensitivity,
            ) = row?;
            if root_hint.is_some_and(|root| !path.starts_with(root)) {
                continue;
            }
            if !filters.matches(&path, &kind, extension.as_deref(), "filename") {
                continue;
            }
            if !all_terms_match(&parsed.terms, &file_name, &title, &path) {
                continue;
            }
            let mut score = score_name_path_hit(&path, &title, &parsed.terms, query);
            if let Some(frecency) = self.usage_score(file_id)? {
                score += frecency;
            }
            if score <= 0.0 {
                continue;
            }
            hits.push(SearchHitRecord {
                file_id,
                path,
                title,
                kind,
                score,
                source: "filename".to_string(),
                snippet: None,
                modified_at,
                size_bytes: Some(size_bytes.max(0) as u64),
                requires_confirmation: sensitivity != "normal",
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.title.cmp(&right.title))
        });
        hits.truncate(limit);
        Ok(hits)
    }

    pub fn search_content(
        &self,
        query: &str,
        limit: usize,
        root_hint: Option<&Path>,
    ) -> Result<Vec<SearchHitRecord>> {
        let parsed = parse_query(query);
        if parsed.terms.is_empty() {
            return Ok(Vec::new());
        }
        let filters = SearchFilters::from_query_filters(&parsed.filters);
        let Some(fts_query) = fts_query_from_terms(&parsed.terms) else {
            return self.search_content_like(query, limit, root_hint, &parsed, &filters);
        };

        self.search_content_fts(query, limit, root_hint, &parsed, &filters, &fts_query)
            .or_else(|_| self.search_content_like(query, limit, root_hint, &parsed, &filters))
    }

    fn search_content_fts(
        &self,
        query: &str,
        limit: usize,
        root_hint: Option<&Path>,
        parsed: &crate::query::SearchQuery,
        filters: &SearchFilters,
        fts_query: &str,
    ) -> Result<Vec<SearchHitRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT f.file_id, f.path, f.file_name, COALESCE(f.title, f.file_name), f.kind,
                   f.extension, f.mtime_ns, f.size_bytes, f.sensitivity, c.text, c.source
            FROM search_chunks_fts
            JOIN search_chunks c ON c.chunk_id = search_chunks_fts.rowid
            JOIN search_files f ON f.file_id = c.file_id
            WHERE search_chunks_fts MATCH ?1
              AND f.indexed_state != 'excluded'
            ORDER BY bm25(search_chunks_fts, 6.0)
            LIMIT ?2
            "#,
        )?;
        let rows = statement.query_map(params![fts_query, candidate_limit(limit)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;

        self.collect_content_hits(rows, query, root_hint, parsed, filters, limit)
    }

    fn search_content_like(
        &self,
        query: &str,
        limit: usize,
        root_hint: Option<&Path>,
        parsed: &crate::query::SearchQuery,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHitRecord>> {
        let seed = parsed
            .terms
            .first()
            .map(|term| format!("%{}%", escape_like(term)))
            .unwrap_or_else(|| "%".to_string());
        let mut statement = self.connection.prepare(
            r#"
            SELECT f.file_id, f.path, f.file_name, COALESCE(f.title, f.file_name), f.kind,
                   f.extension, f.mtime_ns, f.size_bytes, f.sensitivity, c.text, c.source
            FROM search_chunks c
            JOIN search_files f ON f.file_id = c.file_id
            WHERE f.indexed_state != 'excluded'
              AND c.text LIKE ?1 ESCAPE '\'
            ORDER BY f.last_seen_at DESC, c.chunk_index ASC
            LIMIT ?2
            "#,
        )?;
        let rows = statement.query_map(params![seed, candidate_limit(limit)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;

        self.collect_content_hits(rows, query, root_hint, parsed, filters, limit)
    }

    fn collect_content_hits(
        &self,
        rows: impl Iterator<
            Item = rusqlite::Result<(
                i64,
                PathBuf,
                String,
                String,
                String,
                Option<String>,
                Option<i64>,
                i64,
                String,
                String,
                String,
            )>,
        >,
        query: &str,
        root_hint: Option<&Path>,
        parsed: &crate::query::SearchQuery,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchHitRecord>> {
        let mut hits = Vec::new();
        for row in rows {
            let (
                file_id,
                path,
                file_name,
                title,
                kind,
                extension,
                modified_at,
                size_bytes,
                sensitivity,
                text,
                source,
            ) = row?;
            if root_hint.is_some_and(|root| !path.starts_with(root)) {
                continue;
            }
            if !filters.matches(&path, &kind, extension.as_deref(), &source) {
                continue;
            }
            if !all_terms_match_content(&parsed.terms, &file_name, &title, &path, &text) {
                continue;
            }

            let mut score = 180.0
                + content_term_score(&text, &parsed.terms)
                + score_name_path_hit(&path, &title, &parsed.terms, query) * 0.25;
            if source == "app" {
                score += 80.0;
            }
            if let Some(frecency) = self.usage_score(file_id)? {
                score += frecency;
            }
            hits.push(SearchHitRecord {
                file_id,
                path,
                title,
                kind,
                score,
                source,
                snippet: crate::query::snippet::short_snippet(&text, &parsed.terms, 180),
                modified_at,
                size_bytes: Some(size_bytes.max(0) as u64),
                requires_confirmation: sensitivity != "normal",
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.title.cmp(&right.title))
        });
        hits.truncate(limit);
        Ok(hits)
    }

    pub fn stats(&self, paused: bool) -> Result<CatalogStats> {
        Ok(CatalogStats {
            root_count: count_table_where(&self.connection, "search_roots", "enabled = 1")?,
            file_count: count_table(&self.connection, "search_files")?,
            chunk_count: count_table(&self.connection, "search_chunks")?,
            pending_job_count: count_table_where(
                &self.connection,
                "search_jobs",
                "status IN ('queued', 'running')",
            )?,
            failed_job_count: count_table_where(
                &self.connection,
                "search_jobs",
                "status = 'failed'",
            )?,
            paused,
        })
    }

    pub fn audit(&self) -> Result<SearchAudit> {
        let roots = self
            .active_roots()?
            .into_iter()
            .map(|path| path.display().to_string())
            .collect();
        let failed_jobs = {
            let mut statement = self.connection.prepare(
                "SELECT path || ': ' || COALESCE(last_error, 'failed') FROM search_jobs WHERE status = 'failed' ORDER BY finished_at DESC LIMIT 20",
            )?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        Ok(SearchAudit {
            roots,
            file_count: count_table(&self.connection, "search_files")?,
            chunk_count: count_table(&self.connection, "search_chunks")?,
            sensitive_skipped_count: count_table_where(
                &self.connection,
                "search_files",
                "sensitivity != 'normal'",
            )?,
            failed_jobs,
        })
    }

    fn usage_score(&self, file_id: i64) -> Result<Option<f32>> {
        let score = self
            .connection
            .query_row(
                "SELECT MIN(count, 20) * 4 FROM search_usage WHERE file_id = ?1",
                params![file_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|value| value as f32);
        Ok(score)
    }

    fn upsert_file_fts(&self, file_id: i64, record: &SearchFileRecord) -> Result<()> {
        self.connection.execute(
            "DELETE FROM search_files_fts WHERE rowid = ?1",
            params![file_id],
        )?;
        self.connection.execute(
            r#"
            INSERT INTO search_files_fts(rowid, file_name, title, path, kind, extension)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                file_id,
                record.file_name.as_str(),
                record.title.as_deref().unwrap_or(&record.file_name),
                record.path.display().to_string(),
                record.kind.as_str(),
                record.extension.as_deref().unwrap_or_default(),
            ],
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct SearchFilters {
    kind: Option<String>,
    extension: Option<String>,
    path_contains: Option<String>,
    source: Option<String>,
}

impl SearchFilters {
    fn from_query_filters(filters: &[QueryFilter]) -> Self {
        let mut output = Self::default();
        for filter in filters {
            match filter {
                QueryFilter::Kind(value) => output.kind = Some(value.clone()),
                QueryFilter::Extension(value) => output.extension = Some(value.clone()),
                QueryFilter::Path(value) => output.path_contains = Some(value.clone()),
                QueryFilter::Source(value) => output.source = Some(value.clone()),
                QueryFilter::Modified(_) | QueryFilter::Size(_) => {}
            }
        }
        output
    }

    fn matches(&self, path: &Path, kind: &str, extension: Option<&str>, source: &str) -> bool {
        if self
            .kind
            .as_deref()
            .is_some_and(|expected| expected != kind)
        {
            return false;
        }
        if self
            .extension
            .as_deref()
            .is_some_and(|expected| extension != Some(expected))
        {
            return false;
        }
        if self.path_contains.as_deref().is_some_and(|expected| {
            !path
                .display()
                .to_string()
                .to_ascii_lowercase()
                .contains(expected)
        }) {
            return false;
        }
        if self
            .source
            .as_deref()
            .is_some_and(|expected| expected != source)
        {
            return false;
        }
        true
    }
}

pub fn record_from_path(
    path: &Path,
    canonical_path: &Path,
    max_file_size_bytes: u64,
) -> Result<SearchFileRecord> {
    let metadata = fs::metadata(canonical_path)
        .with_context(|| format!("failed to read metadata for {}", canonical_path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    let kind = classify_kind(extension.as_deref());
    let indexed_state = if metadata.len() > max_file_size_bytes {
        "metadata_only"
    } else {
        "metadata_indexed"
    }
    .to_string();

    let title = if extension.as_deref() == Some("desktop") {
        desktop_entry_title(canonical_path)
    } else {
        None
    }
    .or_else(|| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())
    });

    Ok(SearchFileRecord {
        path: path.to_path_buf(),
        canonical_path: canonical_path.to_path_buf(),
        parent_dir: path
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_default(),
        title,
        file_name,
        extension: extension.clone(),
        mime: extension
            .as_deref()
            .map(extension_to_mime)
            .map(str::to_string),
        kind,
        size_bytes: metadata.len(),
        mtime_ns: metadata_time_ns(metadata.modified().ok()),
        ctime_ns: metadata_time_ns(metadata.created().ok()),
        inode: inode(&metadata),
        dev: dev(&metadata),
        indexed_state,
        sensitivity: "normal".to_string(),
        last_seen_at: unix_ms_now(),
    })
}

fn all_terms_match(terms: &[String], file_name: &str, title: &str, path: &Path) -> bool {
    let haystack = format!(
        "{} {} {}",
        file_name.to_ascii_lowercase(),
        title.to_ascii_lowercase(),
        path.display().to_string().to_ascii_lowercase()
    );
    terms.iter().all(|term| haystack.contains(term))
}

fn all_terms_match_content(
    terms: &[String],
    file_name: &str,
    title: &str,
    path: &Path,
    text: &str,
) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        file_name.to_ascii_lowercase(),
        title.to_ascii_lowercase(),
        path.display().to_string().to_ascii_lowercase(),
        text.to_ascii_lowercase()
    );
    terms.iter().all(|term| haystack.contains(term))
}

fn content_term_score(text: &str, terms: &[String]) -> f32 {
    let text = text.to_ascii_lowercase();
    terms
        .iter()
        .filter(|term| !term.is_empty())
        .map(|term| if text.contains(term) { 44.0 } else { 0.0 })
        .sum()
}

fn candidate_limit(limit: usize) -> i64 {
    limit.max(1).saturating_mul(32).clamp(80, 512) as i64
}

fn fts_query_from_terms(terms: &[String]) -> Option<String> {
    let tokens = terms
        .iter()
        .flat_map(|term| {
            term.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .map(str::trim)
                .map(str::to_lowercase)
                .filter(|term| !term.is_empty())
                .collect::<Vec<_>>()
        })
        .take(8)
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>();

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if current.chars().count() >= max_chars {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn desktop_entry_title(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let lang = std::env::var("LANG")
        .ok()
        .and_then(|value| value.split('.').next().map(str::to_string));
    let mut fallback = None;
    for line in String::from_utf8_lossy(&bytes[..bytes.len().min(64 * 1024)]).lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "Name" && fallback.is_none() && !value.trim().is_empty() {
            fallback = Some(value.trim().to_string());
        }
        if let Some(lang) = &lang {
            if key
                .strip_prefix("Name[")
                .and_then(|value| value.strip_suffix(']'))
                .is_some_and(|locale| locale == lang || lang.starts_with(locale))
                && !value.trim().is_empty()
            {
                return Some(value.trim().to_string());
            }
        }
    }
    fallback
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('%', r"\%")
        .replace('_', r"\_")
}

fn classify_kind(extension: Option<&str>) -> String {
    match extension.unwrap_or_default() {
        "desktop" => "app",
        "rs" | "c" | "cc" | "cpp" | "h" | "hpp" | "py" | "js" | "ts" | "tsx" | "jsx" | "go"
        | "java" | "kt" | "swift" | "rb" | "php" | "sh" | "toml" | "yaml" | "yml" | "json" => {
            "code"
        }
        "pdf" | "epub" | "mobi" | "azw" | "azw3" | "doc" | "docx" | "odt" | "rtf" | "txt"
        | "md" | "markdown" => "document",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "svg" => "image",
        _ => "file",
    }
    .to_string()
}

fn extension_to_mime(extension: &str) -> &'static str {
    match extension {
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "rs" => "text/x-rust",
        "toml" => "application/toml",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "desktop" => "application/x-desktop",
        _ => "application/octet-stream",
    }
}

fn metadata_time_ns(time: Option<SystemTime>) -> Option<i64> {
    let duration = time?.duration_since(UNIX_EPOCH).ok()?;
    Some((duration.as_secs() as i64).saturating_mul(1_000_000_000) + duration.subsec_nanos() as i64)
}

#[cfg(unix)]
fn inode(metadata: &fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.ino() as i64)
}

#[cfg(not(unix))]
fn inode(_metadata: &fs::Metadata) -> Option<i64> {
    None
}

#[cfg(unix)]
fn dev(metadata: &fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.dev() as i64)
}

#[cfg(not(unix))]
fn dev(_metadata: &fs::Metadata) -> Option<i64> {
    None
}

fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn count_table(connection: &Connection, table: &str) -> Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count = connection.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
    Ok(count as usize)
}

fn count_table_where(connection: &Connection, table: &str, where_clause: &str) -> Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}");
    let count = connection.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
    Ok(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "visionclip-search-catalog-{name}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn stores_and_locates_file_metadata() {
        let catalog = SearchCatalog::open_in_memory().unwrap();
        let root = temp_root("locate");
        let file = root.join("docker-compose.yml");
        fs::write(&file, b"services: {}").unwrap();
        let canonical = fs::canonicalize(&file).unwrap();

        catalog.upsert_root(&root, "normal").unwrap();
        let record = record_from_path(&file, &canonical, 1024 * 1024).unwrap();
        let file_id = catalog.upsert_file(&record).unwrap();

        let hits = catalog.search_name_path("docker compose", 5, None).unwrap();

        assert_eq!(hits[0].file_id, file_id);
        assert_eq!(hits[0].title, "docker-compose");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn filters_by_extension() {
        let catalog = SearchCatalog::open_in_memory().unwrap();
        let root = temp_root("filters");
        let rs = root.join("main.rs");
        let txt = root.join("main.txt");
        fs::write(&rs, b"fn main() {}").unwrap();
        fs::write(&txt, b"notes").unwrap();

        for file in [&rs, &txt] {
            let canonical = fs::canonicalize(file).unwrap();
            let record = record_from_path(file, &canonical, 1024 * 1024).unwrap();
            catalog.upsert_file(&record).unwrap();
        }

        let hits = catalog.search_name_path("main ext:rs", 5, None).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, rs);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn searches_extracted_content_chunks() {
        let catalog = SearchCatalog::open_in_memory().unwrap();
        let root = temp_root("content");
        let file = root.join("notes.txt");
        fs::write(&file, b"database architecture and auth middleware").unwrap();
        let canonical = fs::canonicalize(&file).unwrap();
        let record = record_from_path(&file, &canonical, 1024 * 1024).unwrap();
        let file_id = catalog.upsert_file(&record).unwrap();
        catalog
            .replace_chunks(
                file_id,
                &ExtractedText {
                    text: "database architecture and auth middleware".to_string(),
                    source: "content".to_string(),
                },
            )
            .unwrap();

        let hits = catalog.search_content("auth middleware", 5, None).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, file);
        assert_eq!(hits[0].source, "content");
        assert!(hits[0].snippet.as_deref().unwrap().contains("auth"));
        let _ = fs::remove_dir_all(root);
    }
}
