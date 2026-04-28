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
pub struct ReplCommandJob {
    pub request_id: Uuid,
    pub command: coddy_core::ReplCommand,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplSessionSnapshotJob {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisionRequest {
    Capture(CaptureJob),
    VoiceSearch(VoiceSearchJob),
    OpenApplication(ApplicationLaunchJob),
    OpenUrl(UrlOpenJob),
    HealthCheck(HealthCheckJob),
    ReplCommand(ReplCommandJob),
    ReplSessionSnapshot(ReplSessionSnapshotJob),
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
    ReplSessionSnapshot {
        request_id: Uuid,
        snapshot: Box<coddy_core::ReplSessionSnapshot>,
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
    fn legacy_vision_request_variant_tags_remain_stable() {
        let request_id = Uuid::nil();

        assert_eq!(
            encoded_variant_tag(&VisionRequest::Capture(CaptureJob {
                request_id,
                action: Action::CopyText,
                mime_type: "image/png".into(),
                image_bytes: Vec::new(),
                session_type: SessionType::Unknown,
                speak: false,
                source_app: None,
            })),
            0
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::VoiceSearch(VoiceSearchJob {
                request_id,
                transcript: "teste".into(),
                query: "teste".into(),
                speak: false,
            })),
            1
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::OpenApplication(ApplicationLaunchJob {
                request_id,
                transcript: None,
                app_name: "terminal".into(),
                speak: false,
            })),
            2
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::OpenUrl(UrlOpenJob {
                request_id,
                transcript: None,
                label: "Example".into(),
                url: "https://example.com".into(),
                speak: false,
            })),
            3
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::HealthCheck(HealthCheckJob { request_id })),
            4
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::ReplCommand(ReplCommandJob {
                request_id,
                command: coddy_core::ReplCommand::StopActiveRun,
                speak: false,
            })),
            5
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::ReplSessionSnapshot(
                ReplSessionSnapshotJob { request_id }
            )),
            6
        );
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

    #[test]
    fn repl_command_request_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let request = VisionRequest::ReplCommand(ReplCommandJob {
            request_id,
            command: coddy_core::ReplCommand::VoiceTurn {
                transcript_override: Some("quem foi rousseau?".to_string()),
            },
            speak: true,
        });
        let payload = bincode::serde::encode_to_vec(&request, bincode::config::standard())
            .expect("encode repl command");
        let (decoded, _): (VisionRequest, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode repl command");

        match decoded {
            VisionRequest::ReplCommand(job) => {
                assert_eq!(job.request_id, request_id);
                assert!(job.speak);
                assert_eq!(
                    job.command,
                    coddy_core::ReplCommand::VoiceTurn {
                        transcript_override: Some("quem foi rousseau?".to_string()),
                    }
                );
            }
            _ => panic!("unexpected decoded request"),
        }
    }

    #[test]
    fn repl_session_snapshot_result_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let selected_model = coddy_core::ModelRef {
            provider: "ollama".to_string(),
            name: "gemma4-e2b".to_string(),
        };
        let session =
            coddy_core::ReplSession::new(coddy_core::ReplMode::FloatingTerminal, selected_model);
        let result = JobResult::ReplSessionSnapshot {
            request_id,
            snapshot: Box::new(coddy_core::ReplSessionSnapshot {
                session,
                last_sequence: 7,
            }),
        };
        let payload = bincode::serde::encode_to_vec(&result, bincode::config::standard())
            .expect("encode snapshot result");
        let (decoded, _): (JobResult, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode snapshot result");

        match decoded {
            JobResult::ReplSessionSnapshot {
                request_id: decoded_request_id,
                snapshot,
            } => {
                assert_eq!(decoded_request_id, request_id);
                assert_eq!(snapshot.last_sequence, 7);
            }
            _ => panic!("unexpected decoded result"),
        }
    }

    fn encoded_variant_tag(request: &VisionRequest) -> u8 {
        *bincode::serde::encode_to_vec(request, bincode::config::standard())
            .expect("encode request")
            .first()
            .expect("encoded request has variant tag")
    }
}
