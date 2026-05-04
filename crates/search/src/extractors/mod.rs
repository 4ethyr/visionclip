pub mod desktop_entry;
pub mod image_ocr;
pub mod markdown;
pub mod pdf_text;
pub mod text;

use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtractedText {
    pub text: String,
    pub source: String,
}

pub fn extract_file_text(path: &Path, max_bytes: usize) -> Result<Option<ExtractedText>> {
    extract_file_text_with_pdf(path, max_bytes, true)
}

pub fn extract_file_text_without_pdf(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<ExtractedText>> {
    extract_file_text_with_pdf(path, max_bytes, false)
}

fn extract_file_text_with_pdf(
    path: &Path,
    max_bytes: usize,
    include_pdf: bool,
) -> Result<Option<ExtractedText>> {
    if let Some(text) = desktop_entry::extract_desktop_entry(path, max_bytes)? {
        return Ok(Some(text));
    }
    if let Some(text) = markdown::extract_markdown(path, max_bytes)? {
        return Ok(Some(text));
    }
    if let Some(text) = text::extract_text(path, max_bytes)? {
        return Ok(Some(text));
    }
    if include_pdf {
        if let Some(text) = pdf_text::extract_pdf_text(path, max_bytes)? {
            return Ok(Some(text));
        }
    }
    Ok(None)
}
