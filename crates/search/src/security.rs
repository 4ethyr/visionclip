use crate::config::{expand_home, SearchRuntimeConfig};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSensitivity {
    Normal,
    Sensitive,
    Excluded,
}

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    exclude_dirs: HashSet<String>,
    exclude_globs: Vec<String>,
    sensitive_dirs: Vec<PathBuf>,
}

impl SecurityPolicy {
    pub fn from_config(config: &SearchRuntimeConfig) -> Self {
        let exclude_dirs = config
            .exclude_dirs
            .iter()
            .map(|value| normalize_path_component(value))
            .collect();
        let exclude_globs = config
            .exclude_globs
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        let sensitive_dirs = config
            .exclude_sensitive_dirs
            .iter()
            .filter_map(|value| expand_home(value))
            .filter_map(|path| fs::canonicalize(&path).ok().or(Some(path)))
            .collect();

        Self {
            exclude_dirs,
            exclude_globs,
            sensitive_dirs,
        }
    }

    pub fn classify_path(&self, path: &Path) -> PathSensitivity {
        if self.is_sensitive_path(path) {
            return PathSensitivity::Sensitive;
        }
        if self.path_has_excluded_component(path) || self.file_name_matches_excluded_glob(path) {
            return PathSensitivity::Excluded;
        }
        PathSensitivity::Normal
    }

    pub fn should_skip_dir(&self, path: &Path) -> bool {
        matches!(
            self.classify_path(path),
            PathSensitivity::Sensitive | PathSensitivity::Excluded
        )
    }

    pub fn should_index_file(&self, path: &Path) -> bool {
        matches!(self.classify_path(path), PathSensitivity::Normal)
    }

    pub fn canonical_path_under_root(&self, path: &Path, root: &Path) -> Option<PathBuf> {
        let canonical = fs::canonicalize(path).ok()?;
        let canonical_root = fs::canonicalize(root).ok()?;
        if canonical == canonical_root || canonical.starts_with(&canonical_root) {
            Some(canonical)
        } else {
            None
        }
    }

    fn is_sensitive_path(&self, path: &Path) -> bool {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.sensitive_dirs.iter().any(|sensitive| {
            canonical == *sensitive
                || canonical.starts_with(sensitive)
                || path == sensitive
                || path.starts_with(sensitive)
        })
    }

    fn path_has_excluded_component(&self, path: &Path) -> bool {
        path.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .map(normalize_path_component)
                .is_some_and(|name| self.exclude_dirs.contains(&name))
        })
    }

    fn file_name_matches_excluded_glob(&self, path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            return false;
        };
        let file_name = file_name.to_ascii_lowercase();
        self.exclude_globs
            .iter()
            .any(|pattern| glob_matches(pattern, &file_name))
    }
}

fn normalize_path_component(value: &str) -> String {
    value
        .trim()
        .trim_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    if pattern == value {
        return true;
    }
    if pattern == "*" {
        return true;
    }

    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }

    let mut remaining = value;
    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');

    for (index, part) in parts.iter().filter(|part| !part.is_empty()).enumerate() {
        let Some(found_at) = remaining.find(part) else {
            return false;
        };
        if index == 0 && !starts_with_wildcard && found_at != 0 {
            return false;
        }
        remaining = &remaining[found_at + part.len()..];
    }

    if !ends_with_wildcard {
        if let Some(last) = parts.iter().rev().find(|part| !part.is_empty()) {
            return value.ends_with(last);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "visionclip-search-security-{name}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn denies_secret_file_names() {
        let policy = SecurityPolicy::from_config(&SearchRuntimeConfig::default());

        assert!(!policy.should_index_file(Path::new("/home/user/project/.env")));
        assert!(!policy.should_index_file(Path::new("/home/user/project/api-token.txt")));
        assert!(!policy.should_index_file(Path::new("/home/user/project/id_ed25519")));
    }

    #[test]
    fn rejects_symlinks_that_escape_root() {
        let root = temp_root("symlink-root");
        let outside = temp_root("symlink-outside");
        let outside_file = outside.join("notes.txt");
        fs::write(&outside_file, b"outside").unwrap();
        let link = root.join("outside.txt");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_file, &link).unwrap();

        let policy = SecurityPolicy::from_config(&SearchRuntimeConfig::default());

        #[cfg(unix)]
        assert!(policy.canonical_path_under_root(&link, &root).is_none());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
