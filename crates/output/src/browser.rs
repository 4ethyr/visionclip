use anyhow::{Context, Result};
use std::process::Command;

pub fn build_search_url(query: &str) -> String {
    format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(query.trim())
    )
}

pub fn open_search_query(query: &str) -> Result<()> {
    let url = build_search_url(query);
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .context("failed to spawn xdg-open")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_search_query() {
        let url = build_search_url("erro rust unwrap");
        assert!(url.contains("erro%20rust%20unwrap"));
    }
}
