use super::ExtractedText;
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn extract_desktop_entry(path: &Path, max_bytes: usize) -> Result<Option<ExtractedText>> {
    if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
        return Ok(None);
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let capped = &bytes[..bytes.len().min(max_bytes)];
    let text = String::from_utf8_lossy(capped)
        .lines()
        .filter(|line| {
            let Some((key, _)) = line.split_once('=') else {
                return false;
            };
            let key = key.split_once('[').map_or(key, |(base, _)| base);
            matches!(key, "Name" | "GenericName" | "Comment" | "Keywords")
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Some(ExtractedText {
        text,
        source: "app".to_string(),
    }))
}
