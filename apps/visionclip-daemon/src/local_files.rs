use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use which::which;

const MAX_SEARCH_DEPTH: usize = 8;
const MAX_INDEXED_FILES: usize = 25_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDocumentMatch {
    pub path: PathBuf,
    pub title: String,
    pub score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDocumentOpenResult {
    pub path: PathBuf,
    pub title: String,
}

pub fn open_document_by_query(query: &str) -> Result<LocalDocumentOpenResult> {
    let matched = resolve_document_candidate(query)?;
    open_path(&matched.path)?;
    Ok(LocalDocumentOpenResult {
        path: matched.path,
        title: matched.title,
    })
}

fn resolve_document_candidate(query: &str) -> Result<LocalDocumentMatch> {
    let roots = default_document_search_roots();
    resolve_document_candidate_in_roots(query, &roots)
}

pub(crate) fn resolve_document_candidate_in_roots(
    query: &str,
    roots: &[PathBuf],
) -> Result<LocalDocumentMatch> {
    let normalized_query = normalize_search_text(query);
    if normalized_query.is_empty() {
        anyhow::bail!("document query is empty");
    }

    let canonical_roots = canonical_search_roots(roots);
    if canonical_roots.is_empty() {
        anyhow::bail!("no document search roots are available");
    }

    let mut matches = Vec::new();
    let mut indexed_files = 0_usize;
    for root in &canonical_roots {
        collect_document_matches(
            root,
            root,
            0,
            &normalized_query,
            &mut matches,
            &mut indexed_files,
        );
        if indexed_files >= MAX_INDEXED_FILES {
            break;
        }
    }

    matches
        .into_iter()
        .max_by(|left, right| {
            left.score
                .cmp(&right.score)
                .then_with(|| {
                    right
                        .path
                        .components()
                        .count()
                        .cmp(&left.path.components().count())
                })
                .then_with(|| right.title.len().cmp(&left.title.len()))
        })
        .filter(|candidate| candidate.score >= 20)
        .with_context(|| format!("no local document matched `{}`", query.trim()))
}

fn collect_document_matches(
    root: &Path,
    dir: &Path,
    depth: usize,
    normalized_query: &str,
    matches: &mut Vec<LocalDocumentMatch>,
    indexed_files: &mut usize,
) {
    if depth > MAX_SEARCH_DEPTH || *indexed_files >= MAX_INDEXED_FILES {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if *indexed_files >= MAX_INDEXED_FILES {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if should_skip_directory(&path) {
                continue;
            }
            collect_document_matches(
                root,
                &path,
                depth + 1,
                normalized_query,
                matches,
                indexed_files,
            );
            continue;
        }
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }
        if !is_supported_document(&path) {
            continue;
        }

        *indexed_files += 1;
        let Ok(canonical_path) = fs::canonicalize(&path) else {
            continue;
        };
        if !path_is_under(&canonical_path, root) {
            continue;
        }
        if let Some(score) = score_document_candidate(&canonical_path, normalized_query) {
            matches.push(LocalDocumentMatch {
                title: document_title(&canonical_path),
                path: canonical_path,
                score,
            });
        }
    }
}

fn canonical_search_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    roots
        .iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .filter(|root| root.is_dir())
        .filter(|root| seen.insert(root.clone()))
        .collect()
}

fn default_document_search_roots() -> Vec<PathBuf> {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    let mut roots = Vec::new();
    for key in [
        "XDG_DOCUMENTS_DIR",
        "XDG_DOWNLOAD_DIR",
        "XDG_DESKTOP_DIR",
        "XDG_PUBLICSHARE_DIR",
    ] {
        if let Some(path) = read_user_dir(key, &home) {
            push_unique_path(&mut roots, path);
        }
    }

    for relative in [
        "Documents",
        "Downloads",
        "Desktop",
        "Books",
        "Livros",
        "Ebooks",
        "eBooks",
        "Calibre Library",
    ] {
        push_unique_path(&mut roots, home.join(relative));
    }

    roots.into_iter().filter(|path| path.is_dir()).collect()
}

fn read_user_dir(key: &str, home: &Path) -> Option<PathBuf> {
    let path = home.join(".config/user-dirs.dirs");
    let contents = fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(key) {
            continue;
        }
        let (_, value) = trimmed.split_once('=')?;
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return None;
        }
        if let Some(rest) = value.strip_prefix("$HOME/") {
            return Some(home.join(rest));
        }
        if value == "$HOME" {
            return Some(home.to_path_buf());
        }
        return Some(PathBuf::from(value));
    }
    None
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|candidate| candidate == &path) {
        paths.push(path);
    }
}

fn should_skip_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let lowered = name.to_ascii_lowercase();
    name.starts_with('.')
        || matches!(
            lowered.as_str(),
            "node_modules"
                | "target"
                | "vendor"
                | "venv"
                | ".venv"
                | "__pycache__"
                | "cache"
                | "tmp"
                | "temp"
        )
}

fn is_supported_document(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some(
            "pdf"
                | "epub"
                | "mobi"
                | "azw"
                | "azw3"
                | "djvu"
                | "txt"
                | "md"
                | "markdown"
                | "doc"
                | "docx"
                | "odt"
                | "rtf"
        )
    )
}

fn score_document_candidate(path: &Path, normalized_query: &str) -> Option<usize> {
    let title = document_title(path);
    let normalized_title = normalize_search_text(&title);
    if normalized_title.is_empty() {
        return None;
    }

    let query_tokens = normalized_query.split_whitespace().collect::<Vec<_>>();
    if query_tokens.is_empty() {
        return None;
    }

    let title_tokens = normalized_title.split_whitespace().collect::<Vec<_>>();
    let matched_tokens = query_tokens
        .iter()
        .filter(|query_token| {
            title_tokens
                .iter()
                .any(|title_token| title_token == *query_token)
        })
        .count();
    if matched_tokens == 0 {
        return None;
    }

    let mut score = matched_tokens * 20;
    if matched_tokens == query_tokens.len() {
        score += 80;
    }
    if normalized_title == normalized_query {
        score += 500;
    } else if normalized_title.contains(normalized_query) {
        score += 220;
    }
    if has_ordered_tokens(&title_tokens, &query_tokens) {
        score += 60;
    }
    if let Some(ext) = path.extension().and_then(|value| value.to_str()) {
        score += match ext.to_ascii_lowercase().as_str() {
            "pdf" | "epub" => 20,
            "mobi" | "azw" | "azw3" => 16,
            _ => 0,
        };
    }

    Some(score)
}

fn has_ordered_tokens(title_tokens: &[&str], query_tokens: &[&str]) -> bool {
    let mut next_index = 0_usize;
    for query_token in query_tokens {
        if let Some(offset) = title_tokens[next_index..]
            .iter()
            .position(|title_token| title_token == query_token)
        {
            next_index += offset + 1;
        } else {
            return false;
        }
    }
    true
}

fn document_title(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .trim()
        .to_string()
}

fn path_is_under(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn open_path(path: &Path) -> Result<()> {
    if let Some(command) = first_available(&["xdg-open"]) {
        spawn_detached_path(&command, path)?;
        return Ok(());
    }
    if let Some(command) = first_available(&["gio"]) {
        Command::new(command)
            .arg("open")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn `gio open`")?;
        return Ok(());
    }
    anyhow::bail!("no safe document opener found; install `xdg-open` or `gio`");
}

fn spawn_detached_path(program: &str, path: &Path) -> Result<()> {
    Command::new(program)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn `{program}` for {}", path.display()))?;
    Ok(())
}

fn first_available(commands: &[&str]) -> Option<String> {
    commands
        .iter()
        .find(|command| which(command).is_ok())
        .map(|command| (*command).to_string())
}

fn normalize_search_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        let folded = match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => "a".to_string(),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => "e".to_string(),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => "i".to_string(),
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => "o".to_string(),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => "u".to_string(),
            'ç' | 'Ç' => "c".to_string(),
            'ñ' | 'Ñ' => "n".to_string(),
            other => other.to_lowercase().collect(),
        };
        for lowered in folded.chars() {
            if lowered.is_alphanumeric() || lowered.is_whitespace() {
                output.push(lowered);
            } else {
                output.push(' ');
            }
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "visionclip-local-files-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn resolves_book_by_title_tokens() {
        let root = test_root("book-title");
        let book =
            root.join("Programming TypeScript - Making Your JavaScript Applications Scale.pdf");
        fs::write(&book, b"pdf").unwrap();

        let matched = resolve_document_candidate_in_roots(
            "programming typescript",
            std::slice::from_ref(&root),
        )
        .unwrap();

        assert_eq!(matched.path, fs::canonicalize(&book).unwrap());
        assert!(matched.score >= 100);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_unsupported_files() {
        let root = test_root("unsupported");
        fs::write(root.join("Programming TypeScript.exe"), b"binary").unwrap();

        let error = resolve_document_candidate_in_roots(
            "programming typescript",
            std::slice::from_ref(&root),
        )
        .unwrap_err();

        assert!(error.to_string().contains("no local document matched"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prefers_exact_document_title() {
        let root = test_root("exact-title");
        let exact = root.join("Programming TypeScript.epub");
        let partial = root.join("TypeScript Notes.pdf");
        fs::write(&exact, b"epub").unwrap();
        fs::write(&partial, b"pdf").unwrap();

        let matched = resolve_document_candidate_in_roots(
            "Programming TypeScript",
            std::slice::from_ref(&root),
        )
        .unwrap();

        assert_eq!(matched.path, fs::canonicalize(&exact).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
