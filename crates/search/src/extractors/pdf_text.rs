use super::ExtractedText;
use anyhow::Result;
use std::{path::Path, process::Command};

pub fn extract_pdf_text(path: &Path, max_bytes: usize) -> Result<Option<ExtractedText>> {
    if path.extension().and_then(|value| value.to_str()) != Some("pdf") {
        return Ok(None);
    }

    let Ok(output) = Command::new("pdftotext")
        .arg("-layout")
        .arg("-enc")
        .arg("UTF-8")
        .arg(path)
        .arg("-")
        .output()
    else {
        return Ok(None);
    };

    if !output.status.success() || output.stdout.is_empty() {
        return Ok(None);
    }

    let capped = &output.stdout[..output.stdout.len().min(max_bytes)];
    Ok(Some(ExtractedText {
        text: String::from_utf8_lossy(capped).to_string(),
        source: "content".to_string(),
    }))
}
