use super::ExtractedText;
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn extract_markdown(path: &Path, max_bytes: usize) -> Result<Option<ExtractedText>> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("md" | "markdown")) {
        return Ok(None);
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let capped = &bytes[..bytes.len().min(max_bytes)];
    Ok(Some(ExtractedText {
        text: String::from_utf8_lossy(capped).to_string(),
        source: "content".to_string(),
    }))
}
