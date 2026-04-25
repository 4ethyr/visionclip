use anyhow::Result;
use std::process::Command;
use tracing::warn;

pub fn notify(summary: &str, body: &str) -> Result<()> {
    let result = Command::new("notify-send").arg(summary).arg(body).spawn();

    if let Err(error) = result {
        warn!(
            ?error,
            "notify-send is unavailable; continuing without desktop notification"
        );
    }

    Ok(())
}
