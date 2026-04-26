use crate::error::AppResult;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Action {
    CopyText,
    ExtractCode,
    TranslatePtBr,
    Explain,
    SearchWeb,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::CopyText => "CopyText",
            Action::ExtractCode => "ExtractCode",
            Action::TranslatePtBr => "TranslatePtBr",
            Action::Explain => "Explain",
            Action::SearchWeb => "SearchWeb",
        }
    }
}

impl std::str::FromStr for Action {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "copytext" | "copy_text" => Ok(Action::CopyText),
            "extractcode" | "extract_code" => Ok(Action::ExtractCode),
            "translateptbr" | "translate_ptbr" | "translate" => Ok(Action::TranslatePtBr),
            "explain" => Ok(Action::Explain),
            "searchweb" | "search_web" | "search" => Ok(Action::SearchWeb),
            other => Err(format!("unknown action: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionType {
    Wayland,
    X11,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureJob {
    pub request_id: Uuid,
    pub action: Action,
    pub mime_type: String,
    pub image_bytes: Vec<u8>,
    pub session_type: SessionType,
    pub speak: bool,
    pub source_app: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSearchJob {
    pub request_id: Uuid,
    pub transcript: String,
    pub query: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationLaunchJob {
    pub request_id: Uuid,
    pub transcript: Option<String>,
    pub app_name: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlOpenJob {
    pub request_id: Uuid,
    pub transcript: Option<String>,
    pub label: String,
    pub url: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckJob {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisionRequest {
    Capture(CaptureJob),
    VoiceSearch(VoiceSearchJob),
    OpenApplication(ApplicationLaunchJob),
    OpenUrl(UrlOpenJob),
    HealthCheck(HealthCheckJob),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobResult {
    ClipboardText {
        request_id: Uuid,
        text: String,
        spoken: bool,
    },
    BrowserQuery {
        request_id: Uuid,
        query: String,
        summary: Option<String>,
        spoken: bool,
    },
    ActionStatus {
        request_id: Uuid,
        message: String,
        spoken: bool,
    },
    Error {
        request_id: Uuid,
        code: String,
        message: String,
    },
}

pub async fn write_message<W, T>(writer: &mut W, value: &T) -> AppResult<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = bincode::serde::encode_to_vec(value, bincode::config::standard())?;
    let length = payload.len() as u32;
    writer.write_u32(length).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_message<R, T>(reader: &mut R) -> AppResult<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let length = reader.read_u32().await? as usize;
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload).await?;
    let (value, _): (T, usize) =
        bincode::serde::decode_from_slice(&payload, bincode::config::standard())?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_aliases() {
        assert_eq!(
            "translate".parse::<Action>().unwrap(),
            Action::TranslatePtBr
        );
        assert_eq!("copy_text".parse::<Action>().unwrap(), Action::CopyText);
    }

    #[test]
    fn health_check_request_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let request = VisionRequest::HealthCheck(HealthCheckJob { request_id });
        let payload = bincode::serde::encode_to_vec(&request, bincode::config::standard())
            .expect("encode health check");
        let (decoded, _): (VisionRequest, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode health check");

        match decoded {
            VisionRequest::HealthCheck(job) => assert_eq!(job.request_id, request_id),
            _ => panic!("unexpected decoded request"),
        }
    }

    #[test]
    fn open_url_request_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let request = VisionRequest::OpenUrl(UrlOpenJob {
            request_id,
            transcript: Some("youtube".into()),
            label: "YouTube".into(),
            url: "https://www.youtube.com/".into(),
            speak: true,
        });
        let payload = bincode::serde::encode_to_vec(&request, bincode::config::standard())
            .expect("encode open url");
        let (decoded, _): (VisionRequest, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode open url");

        match decoded {
            VisionRequest::OpenUrl(job) => {
                assert_eq!(job.request_id, request_id);
                assert_eq!(job.label, "YouTube");
                assert_eq!(job.url, "https://www.youtube.com/");
                assert!(job.speak);
            }
            _ => panic!("unexpected decoded request"),
        }
    }
}
