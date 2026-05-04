use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssistantStatusKind {
    Idle,
    Listening,
    Speaking,
}

impl AssistantStatusKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AssistantStatusKind::Idle => "idle",
            AssistantStatusKind::Listening => "listening",
            AssistantStatusKind::Speaking => "speaking",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantStatusSnapshot {
    pub state: AssistantStatusKind,
    pub updated_at_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

pub fn assistant_status_path() -> PathBuf {
    let state_dir = BaseDirs::new()
        .and_then(|dirs| dirs.state_dir().map(|path| path.to_path_buf()))
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(".local/state")
        });
    state_dir.join("visionclip/status.json")
}

pub fn write_assistant_status(
    state: AssistantStatusKind,
    detail: Option<&str>,
    request_id: Option<&str>,
) -> io::Result<()> {
    let snapshot = AssistantStatusSnapshot {
        state,
        updated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        detail: detail
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        request_id: request_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    };

    let path = assistant_status_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&snapshot)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&tmp_path, bytes)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_kind_serializes_as_snake_case() {
        let json = serde_json::to_string(&AssistantStatusKind::Listening).unwrap();
        assert_eq!(json, "\"listening\"");
        assert_eq!(AssistantStatusKind::Speaking.as_str(), "speaking");
    }
}
