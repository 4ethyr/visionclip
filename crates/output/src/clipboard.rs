use anyhow::{Context, Result};
use arboard::Clipboard;
use std::sync::Mutex;

pub struct ClipboardOwner {
    clipboard: Mutex<Clipboard>,
}

impl ClipboardOwner {
    pub fn new() -> Result<Self> {
        let clipboard = Clipboard::new().context("failed to initialize clipboard")?;
        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }

    pub fn set_text(&self, text: &str) -> Result<()> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| anyhow::anyhow!("clipboard mutex poisoned"))?;
        clipboard
            .set_text(text.to_string())
            .context("failed to write text to clipboard")?;
        Ok(())
    }
}
