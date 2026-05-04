use std::{env, path::PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchRuntimeConfig {
    pub enabled: bool,
    pub index_on_startup: bool,
    pub watch_enabled: bool,
    pub debounce_ms: u64,
    pub max_file_size_mb: u64,
    pub max_text_bytes: usize,
    pub max_workers: usize,
    pub content_index: bool,
    pub semantic_index: bool,
    pub ocr_index: bool,
    pub vector_backend: String,
    pub roots: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub exclude_sensitive_dirs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub ranking: RankingConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RankingConfig {
    pub prefer_filename_for_short_queries: bool,
    pub recency_boost: bool,
    pub frecency_boost: bool,
    pub hybrid_fusion: String,
}

impl Default for SearchRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            index_on_startup: true,
            watch_enabled: true,
            debounce_ms: 800,
            max_file_size_mb: 64,
            max_text_bytes: 4_000_000,
            max_workers: 2,
            content_index: true,
            semantic_index: false,
            ocr_index: false,
            vector_backend: "sqlite_vec".to_string(),
            roots: default_roots(),
            exclude_dirs: default_exclude_dirs(),
            exclude_sensitive_dirs: default_sensitive_dirs(),
            exclude_globs: default_exclude_globs(),
            ranking: RankingConfig::default(),
        }
    }
}

impl Default for RankingConfig {
    fn default() -> Self {
        Self {
            prefer_filename_for_short_queries: true,
            recency_boost: true,
            frecency_boost: true,
            hybrid_fusion: "rrf".to_string(),
        }
    }
}

impl SearchRuntimeConfig {
    pub fn expanded_roots(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .filter_map(|root| expand_home(root))
            .collect()
    }

    pub fn max_file_size_bytes(&self) -> u64 {
        self.max_file_size_mb.saturating_mul(1024 * 1024)
    }
}

pub fn expand_home(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "~" {
        return env::var_os("HOME").map(PathBuf::from);
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return env::var_os("HOME").map(|home| PathBuf::from(home).join(rest));
    }
    Some(PathBuf::from(trimmed))
}

fn default_roots() -> Vec<String> {
    [
        "/usr/share/applications",
        "/var/lib/flatpak/exports/share/applications",
        "~/.local/share/applications",
        "~/.local/share/flatpak/exports/share/applications",
        "~/Documents",
        "~/Downloads",
        "~/Desktop",
        "~/Pictures",
        "~/Projects",
        "~/dev",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_exclude_dirs() -> Vec<String> {
    [
        ".git",
        "node_modules",
        "target",
        "vendor",
        ".venv",
        "venv",
        "__pycache__",
        ".hg",
        ".svn",
        "dist",
        "build",
        ".cache",
        ".local/share/Trash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_sensitive_dirs() -> Vec<String> {
    [
        "~/.ssh",
        "~/.gnupg",
        "~/.local/share/keyrings",
        "~/.password-store",
        "~/.aws",
        "~/.azure",
        "~/.kube",
        "~/.docker",
        "~/.mozilla",
        "~/.config/google-chrome",
        "~/.config/chromium",
        "~/.config/BraveSoftware",
        "~/.config/Signal",
        "~/.config/discord",
        "~/.cache",
        "~/.local/share/Trash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_exclude_globs() -> Vec<String> {
    [
        ".env",
        ".env.*",
        "*.pem",
        "*.key",
        "*.p12",
        "*.pfx",
        "id_rsa",
        "id_ed25519",
        "*credentials*",
        "*secret*",
        "*token*",
        "*password*",
        "*.sqlite",
        "*.db",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_disable_semantic_and_ocr_indexing() {
        let config = SearchRuntimeConfig::default();

        assert!(config.enabled);
        assert!(!config.semantic_index);
        assert!(!config.ocr_index);
        assert_eq!(config.vector_backend, "sqlite_vec");
    }
}
