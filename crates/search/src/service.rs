use crate::{
    catalog::{CatalogStats, SearchAudit, SearchCatalog, SearchHitRecord},
    config::SearchRuntimeConfig,
    crawler::{crawl_roots, crawl_roots_startup, CrawlSummary},
    query::{classify_query, parse_query, QueryFilter, QueryShape},
};
use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LocalSearchMode {
    Auto,
    Locate,
    Lexical,
    Grep,
    Semantic,
    Hybrid,
    Apps,
    Recent,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LocalSearchRequest {
    pub query: String,
    pub mode: LocalSearchMode,
    pub root_hint: Option<PathBuf>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchControlReport {
    pub status: CatalogStats,
    pub last_crawl: Option<CrawlSummary>,
}

pub struct SearchService {
    config: SearchRuntimeConfig,
    catalog: SearchCatalog,
    bootstrapped: bool,
    paused: bool,
    last_crawl: Option<CrawlSummary>,
}

impl SearchService {
    pub fn open(path: impl AsRef<Path>, config: SearchRuntimeConfig) -> Result<Self> {
        Ok(Self {
            catalog: SearchCatalog::open(path)?,
            config,
            bootstrapped: false,
            paused: false,
            last_crawl: None,
        })
    }

    pub fn open_in_memory(config: SearchRuntimeConfig) -> Result<Self> {
        Ok(Self {
            catalog: SearchCatalog::open_in_memory()?,
            config,
            bootstrapped: false,
            paused: false,
            last_crawl: None,
        })
    }

    pub fn ensure_bootstrapped(&mut self) -> Result<()> {
        if self.bootstrapped || !self.config.enabled {
            return Ok(());
        }
        for root in self.config.expanded_roots() {
            if root.is_dir() {
                let canonical = std::fs::canonicalize(&root).unwrap_or(root);
                self.catalog.upsert_root(&canonical, "normal")?;
            }
        }
        self.bootstrapped = true;
        Ok(())
    }

    pub fn search(&mut self, request: LocalSearchRequest) -> Result<Vec<SearchHitRecord>> {
        self.ensure_bootstrapped()?;
        let limit = request.limit.clamp(1, 100);
        let root_hint = request.root_hint.as_deref();
        let query = match request.mode {
            LocalSearchMode::Apps if !request.query.to_ascii_lowercase().contains("kind:") => {
                format!("{} kind:app", request.query)
            }
            _ => request.query.clone(),
        };
        match request.mode {
            LocalSearchMode::Locate | LocalSearchMode::Apps | LocalSearchMode::Recent => {
                self.catalog.search_name_path(&query, limit, root_hint)
            }
            LocalSearchMode::Grep => {
                let mut hits = self.catalog.search_content(&query, limit, root_hint)?;
                merge_duplicate_hits(&mut hits);
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
            LocalSearchMode::Auto
            | LocalSearchMode::Lexical
            | LocalSearchMode::Hybrid
            | LocalSearchMode::Semantic => {
                let mut hits = self.catalog.search_name_path(&query, limit, root_hint)?;
                if self.config.content_index && should_search_content(request.mode, &query) {
                    hits.extend(self.catalog.search_content(&query, limit, root_hint)?);
                    merge_duplicate_hits(&mut hits);
                    hits.sort_by(|left, right| {
                        right
                            .score
                            .partial_cmp(&left.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| left.title.cmp(&right.title))
                    });
                    hits.truncate(limit);
                }
                Ok(hits)
            }
        }
    }

    pub fn status(&mut self) -> Result<SearchControlReport> {
        self.ensure_bootstrapped()?;
        Ok(SearchControlReport {
            status: self.catalog.stats(self.paused)?,
            last_crawl: self.last_crawl.clone(),
        })
    }

    pub fn audit(&mut self) -> Result<SearchAudit> {
        self.ensure_bootstrapped()?;
        self.catalog.audit()
    }

    pub fn rebuild(&mut self) -> Result<CrawlSummary> {
        self.paused = false;
        let summary = crawl_roots(&self.catalog, &self.config)?;
        self.last_crawl = Some(summary.clone());
        self.bootstrapped = true;
        Ok(summary)
    }

    pub fn rebuild_startup_index(&mut self) -> Result<CrawlSummary> {
        self.paused = false;
        let summary = crawl_roots_startup(&self.catalog, &self.config)?;
        self.last_crawl = Some(summary.clone());
        self.bootstrapped = true;
        Ok(summary)
    }

    pub fn rebuild_startup_index_if_needed(&mut self) -> Result<Option<CrawlSummary>> {
        self.ensure_bootstrapped()?;
        let stats = self.catalog.stats(self.paused)?;
        if stats.file_count > 0 && stats.chunk_count > 0 {
            return Ok(None);
        }
        self.rebuild_startup_index().map(Some)
    }

    pub fn add_root(&mut self, path: PathBuf) -> Result<SearchControlReport> {
        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        self.catalog.upsert_root(&canonical, "normal")?;
        let root_string = canonical.display().to_string();
        if !self.config.roots.iter().any(|root| root == &root_string) {
            self.config.roots.push(root_string);
        }
        self.bootstrapped = false;
        self.status()
    }

    pub fn remove_root(&mut self, path: &Path) -> Result<SearchControlReport> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.catalog.remove_root(&canonical)?;
        let requested = path.display().to_string();
        let canonical = canonical.display().to_string();
        self.config
            .roots
            .retain(|root| root != &requested && root != &canonical);
        self.status()
    }

    pub fn pause(&mut self) -> Result<SearchControlReport> {
        self.paused = true;
        self.status()
    }

    pub fn resume(&mut self) -> Result<SearchControlReport> {
        self.paused = false;
        self.status()
    }

    pub fn record_open(&mut self, file_id: i64) -> Result<()> {
        self.catalog.record_usage(file_id, "open")
    }

    pub fn file_path(&mut self, file_id: i64) -> Result<Option<PathBuf>> {
        self.ensure_bootstrapped()?;
        self.catalog.file_path(file_id)
    }
}

fn should_search_content(mode: LocalSearchMode, query: &str) -> bool {
    let parsed = parse_query(query);
    let requested_content = parsed.filters.iter().any(|filter| {
        matches!(
            filter,
            QueryFilter::Source(source)
                if matches!(source.as_str(), "content" | "ocr" | "semantic" | "app")
        )
    });
    if requested_content {
        return true;
    }

    match mode {
        LocalSearchMode::Auto => {
            !parsed.phrases.is_empty()
                || matches!(
                    classify_query(&parsed.terms, query),
                    QueryShape::Natural | QueryShape::Code
                ) && parsed.terms.len() >= 3
        }
        LocalSearchMode::Locate | LocalSearchMode::Apps | LocalSearchMode::Recent => false,
        LocalSearchMode::Grep
        | LocalSearchMode::Lexical
        | LocalSearchMode::Hybrid
        | LocalSearchMode::Semantic => true,
    }
}

fn merge_duplicate_hits(hits: &mut Vec<SearchHitRecord>) {
    let mut merged: Vec<SearchHitRecord> = Vec::new();
    for hit in hits.drain(..) {
        if let Some(existing) = merged
            .iter_mut()
            .find(|candidate| candidate.file_id == hit.file_id)
        {
            if hit.score > existing.score {
                let existing_source = existing.source.clone();
                *existing = hit;
                if existing.source == "content" && existing_source == "filename" {
                    existing.score += 40.0;
                }
            } else if existing.snippet.is_none() {
                existing.snippet = hit.snippet;
            }
        } else {
            merged.push(hit);
        }
    }
    *hits = merged;
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "visionclip-search-service-{name}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn bootstraps_catalog_and_locates_files() {
        let root = temp_root("bootstrap");
        std::fs::write(root.join("Architecture Notes.md"), b"# Notes").unwrap();
        let config = SearchRuntimeConfig {
            roots: vec![root.display().to_string()],
            ..Default::default()
        };
        let mut service = SearchService::open_in_memory(config).unwrap();
        service.rebuild().unwrap();

        let hits = service
            .search(LocalSearchRequest {
                query: "architecture".to_string(),
                mode: LocalSearchMode::Locate,
                root_hint: None,
                limit: 10,
            })
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Architecture Notes");
        let _ = std::fs::remove_dir_all(root);
    }
}
