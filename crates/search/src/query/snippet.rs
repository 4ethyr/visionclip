pub fn short_snippet(text: &str, terms: &[String], max_chars: usize) -> Option<String> {
    if text.trim().is_empty() || max_chars == 0 {
        return None;
    }

    let lowered = text.to_ascii_lowercase();
    let start = terms
        .iter()
        .filter(|term| !term.is_empty())
        .filter_map(|term| lowered.find(term))
        .min()
        .unwrap_or(0);

    let prefix = start.saturating_sub(max_chars / 4);
    let snippet = text
        .chars()
        .skip(prefix)
        .take(max_chars)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if snippet.is_empty() {
        None
    } else {
        Some(snippet)
    }
}
