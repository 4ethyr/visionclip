use super::ExtractedText;
use anyhow::Result;
use std::path::Path;

pub fn extract_image_ocr(_path: &Path) -> Result<Option<ExtractedText>> {
    Ok(None)
}
