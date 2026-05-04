use super::ExtractedText;
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn extract_text(path: &Path, max_bytes: usize) -> Result<Option<ExtractedText>> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("txt" | "log" | "csv")) {
        return Ok(None);
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let capped = &bytes[..bytes.len().min(max_bytes)];
    let text = String::from_utf8_lossy(capped).to_string();
    Ok(Some(ExtractedText {
        text,
        source: "content".to_string(),
    }))
}
