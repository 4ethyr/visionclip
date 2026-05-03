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
        "Documentos",
        "Downloads",
        "Transferências",
        "Transferencias",
        "Desktop",
        "Área de Trabalho",
        "Area de Trabalho",
        "Books",
        "Livros",
        "Ebooks",
        "eBooks",
        "E-Books",
        "Calibre Library",
        "Biblioteca do Calibre",
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
    let normalized_query = normalize_document_query_for_matching(normalized_query);
    if normalized_query.is_empty() {
        return None;
    }

    let title = document_title(path);
    let normalized_title = normalize_search_text(&title);
    if normalized_title.is_empty() {
        return None;
    }

    let query_tokens = expand_document_query_tokens(&normalized_query);
    if query_tokens.is_empty() {
        return None;
    }

    let title_tokens = normalized_title.split_whitespace().collect::<Vec<_>>();
    let compact_query = compact_normalized(&normalized_query);
    let compact_title = compact_normalized(&normalized_title);
    let compact_title_contains_query =
        compact_query.len() >= 4 && compact_title.contains(&compact_query);

    let matched_query_tokens = query_tokens
        .iter()
        .filter(|query_token| query_token_matches_title(query_token, &title_tokens, &compact_title))
        .count();

    let significant_query_tokens = query_tokens
        .iter()
        .filter(|token| !is_low_signal_query_token(token))
        .collect::<Vec<_>>();
    let significant_matches = significant_query_tokens
        .iter()
        .filter(|query_token| query_token_matches_title(query_token, &title_tokens, &compact_title))
        .count();
    if significant_query_tokens.is_empty() || matched_query_tokens == 0 {
        return None;
    }
    if !compact_title_contains_query
        && significant_matches
            < minimum_required_significant_matches(significant_query_tokens.len())
    {
        return None;
    }
    if !critical_query_tokens_match(&query_tokens, &title_tokens) {
        return None;
    }

    let mut score = significant_matches * 32 + matched_query_tokens * 8;
    if significant_matches == significant_query_tokens.len() {
        score += 80;
    }
    if normalized_title == normalized_query {
        score += 500;
    } else if normalized_title.contains(&normalized_query) {
        score += 220;
    }
    if compact_title_contains_query {
        score += 260 + compact_query.len().min(80);
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

fn normalize_document_query_for_matching(normalized_query: &str) -> String {
    normalized_query
        .split_whitespace()
        .filter(|token| !is_document_query_noise_token(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_document_query_noise_token(token: &str) -> bool {
    matches!(
        token,
        "open"
            | "launch"
            | "start"
            | "find"
            | "locate"
            | "please"
            | "my"
            | "this"
            | "called"
            | "named"
            | "titled"
            | "book"
            | "boke"
            | "bokeh"
            | "boek"
            | "ebook"
            | "document"
            | "file"
            | "pdf"
            | "epub"
            | "mobi"
            | "azw"
            | "azw3"
            | "abra"
            | "abru"
            | "abre"
            | "abri"
            | "abrir"
            | "por"
            | "favor"
            | "meu"
            | "minha"
            | "o"
            | "a"
            | "os"
            | "as"
            | "livro"
            | "livru"
            | "documento"
            | "arquivo"
            | "apostila"
            | "chamado"
            | "chamada"
            | "intitulado"
            | "intitulada"
            | "libro"
            | "archivo"
            | "mi"
            | "llamado"
            | "llamada"
    )
}

fn expand_document_query_tokens(normalized_query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for token in normalized_query.split_whitespace() {
        match token {
            "blackhead" | "blackhat" => {
                tokens.push("black".to_string());
                tokens.push("hat".to_string());
            }
            "greyhead" | "grayhead" | "greathead" | "greyhat" | "grayhat" | "greathat" => {
                tokens.push("gray".to_string());
                tokens.push("hat".to_string());
            }
            "headfirst" => {
                tokens.push("head".to_string());
                tokens.push("first".to_string());
            }
            "headfirstsql" => {
                tokens.push("head".to_string());
                tokens.push("first".to_string());
                tokens.push("sql".to_string());
            }
            _ => tokens.push(token.to_string()),
        }
    }
    tokens
}

fn query_token_matches_title(
    query_token: &str,
    title_tokens: &[&str],
    compact_title: &str,
) -> bool {
    title_tokens
        .iter()
        .any(|title_token| tokens_match(query_token, title_token))
        || (query_token.chars().count() >= 4 && compact_title.contains(query_token))
}

fn minimum_required_significant_matches(significant_query_tokens: usize) -> usize {
    match significant_query_tokens {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 2,
        4 | 5 => 3,
        count => count.div_ceil(2),
    }
}

fn critical_query_tokens_match(query_tokens: &[String], title_tokens: &[&str]) -> bool {
    query_tokens
        .iter()
        .filter(|token| is_critical_short_query_token(token))
        .all(|query_token| {
            title_tokens
                .iter()
                .any(|title_token| tokens_match(query_token, title_token))
        })
}

fn is_critical_short_query_token(token: &str) -> bool {
    matches!(token, "ai" | "c" | "go" | "goo" | "js" | "sql" | "py" | "r")
}

fn is_low_signal_query_token(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "a"
            | "an"
            | "and"
            | "or"
            | "of"
            | "for"
            | "from"
            | "with"
            | "to"
            | "in"
            | "on"
            | "at"
            | "by"
            | "this"
            | "that"
            | "de"
            | "da"
            | "do"
            | "das"
            | "dos"
            | "e"
            | "para"
            | "por"
            | "el"
            | "la"
            | "los"
            | "las"
            | "del"
            | "y"
    )
}

fn tokens_match(query_token: &str, title_token: &str) -> bool {
    if query_token == title_token || token_aliases_match(query_token, title_token) {
        return true;
    }

    let query_len = query_token.chars().count();
    let title_len = title_token.chars().count();
    let min_len = query_len.min(title_len);
    let max_len = query_len.max(title_len);
    if min_len < 4 {
        return false;
    }

    let distance = levenshtein_distance(query_token, title_token);
    if max_len >= 8 {
        distance <= 2
    } else {
        distance <= 1
    }
}

fn token_aliases_match(query_token: &str, title_token: &str) -> bool {
    let Some(query_alias) = token_alias(query_token) else {
        return false;
    };
    token_alias(title_token).is_some_and(|title_alias| query_alias == title_alias)
}

fn token_alias(token: &str) -> Option<&'static str> {
    match token {
        "grey" | "gray" | "great" => Some("gray"),
        "hat" | "head" => Some("hat"),
        "go" | "goo" => Some("go"),
        "js" | "javascript" => Some("javascript"),
        "py" | "python" => Some("python"),
        _ => None,
    }
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    let mut current = vec![0; previous.len()];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.chars().enumerate() {
            let substitution_cost = usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.chars().count()]
}

fn has_ordered_tokens(title_tokens: &[&str], query_tokens: &[String]) -> bool {
    let mut next_index = 0_usize;
    for query_token in query_tokens {
        if let Some(offset) = title_tokens[next_index..]
            .iter()
            .position(|title_token| tokens_match(query_token, title_token))
        {
            next_index += offset + 1;
        } else {
            return false;
        }
    }
    true
}

fn compact_normalized(normalized: &str) -> String {
    normalized
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
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

    #[test]
    fn resolves_compact_voice_query_against_spaced_title() {
        let root = test_root("compact-title");
        let book = root.join("Head First SQL.pdf");
        fs::write(&book, b"pdf").unwrap();

        let matched =
            resolve_document_candidate_in_roots("headfirstsql", std::slice::from_ref(&root))
                .unwrap();

        assert_eq!(matched.path, fs::canonicalize(&book).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_mixed_language_command_noise_and_title_aliases() {
        let root = test_root("mixed-language-title");
        let intended = root.join("Gray Hat Python.pdf");
        let generic = root.join("Pesquisa Detalhada Sobre Hacking Wi-Fi.pdf");
        fs::write(&intended, b"pdf").unwrap();
        fs::write(&generic, b"pdf").unwrap();

        let matched = resolve_document_candidate_in_roots(
            "abra o livro Greyhead Hacking",
            std::slice::from_ref(&root),
        )
        .unwrap();

        assert_eq!(matched.path, fs::canonicalize(&intended).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_single_generic_token_match_for_multi_token_query() {
        let root = test_root("generic-token");
        fs::write(
            root.join("Pesquisa Detalhada Sobre Hacking Wi-Fi.pdf"),
            b"pdf",
        )
        .unwrap();

        let error =
            resolve_document_candidate_in_roots("Greyhead Hacking", std::slice::from_ref(&root))
                .unwrap_err();

        assert!(error.to_string().contains("no local document matched"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn requires_critical_short_title_tokens() {
        let root = test_root("critical-token");
        fs::write(root.join("Black Hat Rust.pdf"), b"pdf").unwrap();

        let error =
            resolve_document_candidate_in_roots("Black Hat Go", std::slice::from_ref(&root))
                .unwrap_err();

        assert!(error.to_string().contains("no local document matched"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_asr_alias_for_short_title_token() {
        let root = test_root("short-token-alias");
        let book = root.join("Black Hat Go.pdf");
        fs::write(&book, b"pdf").unwrap();

        let matched =
            resolve_document_candidate_in_roots("Black Hat Goo", std::slice::from_ref(&root))
                .unwrap();

        assert_eq!(matched.path, fs::canonicalize(&book).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
