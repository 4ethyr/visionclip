use crate::{
    catalog::{record_from_path, SearchCatalog},
    config::SearchRuntimeConfig,
    extractors::{extract_file_text, extract_file_text_without_pdf},
    security::SecurityPolicy,
};
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_CRAWL_DEPTH: usize = 16;
const MAX_CRAWLED_FILES_PER_ROOT: usize = 100_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CrawlSummary {
    pub roots_seen: usize,
    pub files_indexed: usize,
    pub dirs_skipped: usize,
    pub files_skipped: usize,
    pub errors: usize,
}

pub fn crawl_roots(catalog: &SearchCatalog, config: &SearchRuntimeConfig) -> Result<CrawlSummary> {
    crawl_roots_with_content(catalog, config, ContentExtraction::Full)
}

pub fn crawl_roots_startup(
    catalog: &SearchCatalog,
    config: &SearchRuntimeConfig,
) -> Result<CrawlSummary> {
    crawl_roots_with_content(catalog, config, ContentExtraction::Cheap)
}

fn crawl_roots_with_content(
    catalog: &SearchCatalog,
    config: &SearchRuntimeConfig,
    content_extraction: ContentExtraction,
) -> Result<CrawlSummary> {
    let policy = SecurityPolicy::from_config(config);
    let mut summary = CrawlSummary::default();
    let mut content_candidates = Vec::new();

    for root in config.expanded_roots() {
        let Ok(canonical_root) = fs::canonicalize(&root) else {
            continue;
        };
        if !canonical_root.is_dir() || policy.should_skip_dir(&canonical_root) {
            continue;
        }
        catalog.upsert_root(&canonical_root, "normal")?;
        summary.roots_seen += 1;
        let mut files_seen = 0_usize;
        let mut context = CrawlContext {
            catalog,
            config,
            policy: &policy,
            root: &canonical_root,
            files_seen: &mut files_seen,
            content_candidates: &mut content_candidates,
            summary: &mut summary,
        };
        crawl_dir(&mut context, &canonical_root, 0)?;
    }

    if config.content_index {
        for (file_id, canonical_path) in content_candidates {
            let extracted = match content_extraction {
                ContentExtraction::Cheap => {
                    extract_file_text_without_pdf(&canonical_path, config.max_text_bytes)
                        .ok()
                        .flatten()
                }
                ContentExtraction::Full => {
                    extract_file_text(&canonical_path, config.max_text_bytes)
                        .ok()
                        .flatten()
                }
            };
            if let Some(extracted) = extracted {
                catalog.replace_chunks(file_id, &extracted)?;
            }
        }
    }

    Ok(summary)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentExtraction {
    Cheap,
    Full,
}

struct CrawlContext<'a> {
    catalog: &'a SearchCatalog,
    config: &'a SearchRuntimeConfig,
    policy: &'a SecurityPolicy,
    root: &'a Path,
    files_seen: &'a mut usize,
    content_candidates: &'a mut Vec<(i64, PathBuf)>,
    summary: &'a mut CrawlSummary,
}

fn crawl_dir(context: &mut CrawlContext<'_>, dir: &Path, depth: usize) -> Result<()> {
    if depth > MAX_CRAWL_DEPTH || *context.files_seen >= MAX_CRAWLED_FILES_PER_ROOT {
        return Ok(());
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            context.summary.errors += 1;
            return Ok(());
        }
    };

    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        if *context.files_seen >= MAX_CRAWLED_FILES_PER_ROOT {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            context.summary.errors += 1;
            continue;
        };
        if file_type.is_dir() {
            if context.policy.should_skip_dir(&path) {
                context.summary.dirs_skipped += 1;
                continue;
            }
            crawl_dir(context, &path, depth + 1)?;
            continue;
        }
        if !file_type.is_file() && !file_type.is_symlink() {
            context.summary.files_skipped += 1;
            continue;
        }
        if !context.policy.should_index_file(&path) {
            context.summary.files_skipped += 1;
            continue;
        }
        let Some(canonical_path) = context
            .policy
            .canonical_path_under_root(&path, context.root)
        else {
            context.summary.files_skipped += 1;
            continue;
        };
        let record =
            match record_from_path(&path, &canonical_path, context.config.max_file_size_bytes())
                .with_context(|| format!("failed to catalog {}", path.display()))
            {
                Ok(record) => record,
                Err(_) => {
                    context.summary.errors += 1;
                    continue;
                }
            };
        let file_id = context.catalog.upsert_file(&record)?;
        if context.config.content_index && record.indexed_state != "metadata_only" {
            context.content_candidates.push((file_id, canonical_path));
        }
        *context.files_seen += 1;
        context.summary.files_indexed += 1;
    }

    Ok(())
}

pub fn configured_existing_roots(config: &SearchRuntimeConfig) -> Vec<PathBuf> {
    config
        .expanded_roots()
        .into_iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .filter(|root| root.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "visionclip-search-crawler-{name}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn crawls_allowed_files_and_skips_secrets() {
        let root = temp_root("allowed");
        fs::write(root.join("notes.txt"), b"notes").unwrap();
        fs::write(root.join(".env"), b"TOKEN=secret").unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules/pkg.js"), b"pkg").unwrap();

        let catalog = SearchCatalog::open_in_memory().unwrap();
        let config = SearchRuntimeConfig {
            roots: vec![root.display().to_string()],
            ..Default::default()
        };

        let summary = crawl_roots(&catalog, &config).unwrap();
        let hits = catalog.search_name_path("notes", 5, None).unwrap();
        let secret_hits = catalog.search_name_path("env", 5, None).unwrap();

        assert_eq!(summary.files_indexed, 1);
        assert_eq!(hits.len(), 1);
        assert!(secret_hits.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
