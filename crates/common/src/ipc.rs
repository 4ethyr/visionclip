use crate::error::AppResult;
use crate::language::AssistantLanguage;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::PathBuf;
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
    pub transcript: Option<String>,
    pub input_language: Option<AssistantLanguage>,
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
    pub input_language: Option<AssistantLanguage>,
    pub query: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationLaunchJob {
    pub request_id: Uuid,
    pub transcript: Option<String>,
    pub input_language: Option<AssistantLanguage>,
    pub app_name: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlOpenJob {
    pub request_id: Uuid,
    pub transcript: Option<String>,
    pub input_language: Option<AssistantLanguage>,
    pub label: String,
    pub url: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentOpenJob {
    pub request_id: Uuid,
    pub transcript: Option<String>,
    pub input_language: Option<AssistantLanguage>,
    pub query: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchRequest {
    pub request_id: String,
    pub query: String,
    pub mode: SearchMode,
    pub root_hint: Option<String>,
    pub limit: u16,
    pub include_snippets: bool,
    pub include_ocr: bool,
    pub include_semantic: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchMode {
    Auto,
    Locate,
    Lexical,
    Grep,
    Semantic,
    Hybrid,
    Apps,
    Recent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchControlRequest {
    Status {
        request_id: String,
    },
    AddRoot {
        request_id: String,
        path: String,
    },
    RemoveRoot {
        request_id: String,
        path: String,
    },
    Pause {
        request_id: String,
    },
    Resume {
        request_id: String,
    },
    Rebuild {
        request_id: String,
        root: Option<String>,
    },
    Audit {
        request_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchOpenRequest {
    pub request_id: String,
    pub result_id: String,
    pub action: OpenAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpenAction {
    Open,
    Reveal,
    AskAbout,
    Summarize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResponse {
    pub request_id: String,
    pub elapsed_ms: u32,
    pub mode_used: SearchMode,
    pub hits: Vec<SearchHit>,
    pub diagnostics: Option<SearchDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub result_id: String,
    pub file_id: i64,
    pub path: String,
    pub title: String,
    pub kind: String,
    pub score: f32,
    pub source: SearchHitSource,
    pub snippet: Option<String>,
    pub modified_at: Option<i64>,
    pub size_bytes: Option<u64>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchHitSource {
    FileName,
    Path,
    Content,
    Ocr,
    Semantic,
    Recent,
    App,
    Document,
    Code,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchDiagnostics {
    pub indexed_files: usize,
    pub indexed_chunks: usize,
    pub roots: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckJob {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentIngestJob {
    pub request_id: Uuid,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTranslateJob {
    pub request_id: Uuid,
    pub document_id: String,
    pub target_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentReadJob {
    pub request_id: Uuid,
    pub document_id: String,
    pub target_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentControlKind {
    Pause,
    Resume,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentControlJob {
    pub request_id: Uuid,
    pub reading_session_id: String,
    pub control: DocumentControlKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAskJob {
    pub request_id: Uuid,
    pub document_id: String,
    pub question: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummarizeJob {
    pub request_id: Uuid,
    pub document_id: String,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisionRequest {
    Capture(CaptureJob),
    VoiceSearch(VoiceSearchJob),
    OpenApplication(ApplicationLaunchJob),
    OpenUrl(UrlOpenJob),
    HealthCheck(HealthCheckJob),
    DocumentIngest(DocumentIngestJob),
    DocumentTranslate(DocumentTranslateJob),
    DocumentRead(DocumentReadJob),
    DocumentControl(DocumentControlJob),
    DocumentAsk(DocumentAskJob),
    DocumentSummarize(DocumentSummarizeJob),
    OpenDocument(DocumentOpenJob),
    Search(SearchRequest),
    SearchControl(SearchControlRequest),
    SearchOpen(SearchOpenRequest),
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
    DocumentStatus {
        request_id: Uuid,
        document_id: Option<String>,
        reading_session_id: Option<String>,
        chunks: Option<usize>,
        message: String,
        spoken: bool,
    },
    Error {
        request_id: Uuid,
        code: String,
        message: String,
    },
    Search(SearchResponse),
}

pub async fn write_message<W, T>(writer: &mut W, value: &T) -> AppResult<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = encode_message_payload(value)?;
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
    decode_message_payload(&read_message_payload(reader).await?)
}

pub async fn read_message_payload<R>(reader: &mut R) -> AppResult<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let length = reader.read_u32().await? as usize;
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

pub fn encode_message_payload<T>(value: &T) -> AppResult<Vec<u8>>
where
    T: Serialize,
{
    Ok(bincode::serde::encode_to_vec(
        value,
        bincode::config::standard(),
    )?)
}

pub fn decode_message_payload<T>(payload: &[u8]) -> AppResult<T>
where
    T: DeserializeOwned,
{
    let (value, _): (T, usize) =
        bincode::serde::decode_from_slice(payload, bincode::config::standard())?;
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
        let payload = encode_message_payload(&request).expect("encode health check");
        let decoded: VisionRequest = decode_message_payload(&payload).expect("decode health check");

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
                transcript: None,
                input_language: None,
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
                input_language: Some(AssistantLanguage::PortugueseBrazil),
                query: "teste".into(),
                speak: false,
            })),
            1
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::OpenApplication(ApplicationLaunchJob {
                request_id,
                transcript: None,
                input_language: None,
                app_name: "terminal".into(),
                speak: false,
            })),
            2
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::OpenUrl(UrlOpenJob {
                request_id,
                transcript: None,
                input_language: None,
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
            encoded_variant_tag(&VisionRequest::DocumentIngest(DocumentIngestJob {
                request_id,
                path: PathBuf::from("/tmp/book.txt"),
            })),
            5
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::DocumentTranslate(DocumentTranslateJob {
                request_id,
                document_id: "doc_1".into(),
                target_language: "pt-BR".into(),
            })),
            6
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::DocumentRead(DocumentReadJob {
                request_id,
                document_id: "doc_1".into(),
                target_language: "pt-BR".into(),
            })),
            7
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::DocumentControl(DocumentControlJob {
                request_id,
                reading_session_id: "read_1".into(),
                control: DocumentControlKind::Stop,
            })),
            8
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::DocumentAsk(DocumentAskJob {
                request_id,
                document_id: "doc_1".into(),
                question: "Qual é o tema?".into(),
                speak: false,
            })),
            9
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::DocumentSummarize(DocumentSummarizeJob {
                request_id,
                document_id: "doc_1".into(),
                speak: false,
            })),
            10
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::OpenDocument(DocumentOpenJob {
                request_id,
                transcript: None,
                input_language: None,
                query: "Programming TypeScript".into(),
                speak: false,
            })),
            11
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::Search(SearchRequest {
                request_id: "search_1".into(),
                query: "docker".into(),
                mode: SearchMode::Locate,
                root_hint: None,
                limit: 10,
                include_snippets: true,
                include_ocr: false,
                include_semantic: false,
            })),
            12
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::SearchControl(
                SearchControlRequest::Status {
                    request_id: "status_1".into()
                }
            )),
            13
        );
        assert_eq!(
            encoded_variant_tag(&VisionRequest::SearchOpen(SearchOpenRequest {
                request_id: "open_1".into(),
                result_id: "file:1".into(),
                action: OpenAction::Open,
            })),
            14
        );
    }

    #[test]
    fn open_url_request_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let request = VisionRequest::OpenUrl(UrlOpenJob {
            request_id,
            transcript: Some("youtube".into()),
            input_language: Some(AssistantLanguage::English),
            label: "YouTube".into(),
            url: "https://www.youtube.com/".into(),
            speak: true,
        });
        let payload = encode_message_payload(&request).expect("encode open url");
        let decoded: VisionRequest = decode_message_payload(&payload).expect("decode open url");

        match decoded {
            VisionRequest::OpenUrl(job) => {
                assert_eq!(job.request_id, request_id);
                assert_eq!(job.label, "YouTube");
                assert_eq!(job.url, "https://www.youtube.com/");
                assert_eq!(job.input_language, Some(AssistantLanguage::English));
                assert!(job.speak);
            }
            _ => panic!("unexpected decoded request"),
        }
    }

    #[test]
    fn open_document_request_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let request = VisionRequest::OpenDocument(DocumentOpenJob {
            request_id,
            transcript: Some("Open the book Programming TypeScript".into()),
            input_language: Some(AssistantLanguage::English),
            query: "Programming TypeScript".into(),
            speak: true,
        });
        let payload = encode_message_payload(&request).expect("encode open document");
        let decoded: VisionRequest =
            decode_message_payload(&payload).expect("decode open document");

        match decoded {
            VisionRequest::OpenDocument(job) => {
                assert_eq!(job.request_id, request_id);
                assert_eq!(job.input_language, Some(AssistantLanguage::English));
                assert_eq!(job.query, "Programming TypeScript");
                assert!(job.speak);
            }
            _ => panic!("unexpected decoded request"),
        }
    }

    #[test]
    fn voice_search_request_roundtrips_language_metadata() {
        let request_id = Uuid::new_v4();
        let request = VisionRequest::VoiceSearch(VoiceSearchJob {
            request_id,
            transcript: "打开终端".into(),
            input_language: Some(AssistantLanguage::Chinese),
            query: "Rust async".into(),
            speak: true,
        });
        let payload = encode_message_payload(&request).expect("encode voice search");
        let decoded: VisionRequest = decode_message_payload(&payload).expect("decode voice search");

        match decoded {
            VisionRequest::VoiceSearch(job) => {
                assert_eq!(job.request_id, request_id);
                assert_eq!(job.input_language, Some(AssistantLanguage::Chinese));
                assert_eq!(job.transcript, "打开终端");
                assert_eq!(job.query, "Rust async");
                assert!(job.speak);
            }
            _ => panic!("unexpected decoded request"),
        }
    }

    #[test]
    fn document_status_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let result = JobResult::DocumentStatus {
            request_id,
            document_id: Some("doc_1".into()),
            reading_session_id: Some("read_1".into()),
            chunks: Some(3),
            message: "ok".into(),
            spoken: true,
        };
        let payload = encode_message_payload(&result).expect("encode document status");
        let decoded: JobResult = decode_message_payload(&payload).expect("decode document status");

        match decoded {
            JobResult::DocumentStatus {
                request_id: decoded_id,
                document_id,
                reading_session_id,
                chunks,
                spoken,
                ..
            } => {
                assert_eq!(decoded_id, request_id);
                assert_eq!(document_id.as_deref(), Some("doc_1"));
                assert_eq!(reading_session_id.as_deref(), Some("read_1"));
                assert_eq!(chunks, Some(3));
                assert!(spoken);
            }
            _ => panic!("unexpected decoded response"),
        }
    }

    #[test]
    fn search_request_roundtrips_through_bincode() {
        let request = VisionRequest::Search(SearchRequest {
            request_id: "search_1".into(),
            query: "auth middleware kind:code".into(),
            mode: SearchMode::Hybrid,
            root_hint: Some("./src".into()),
            limit: 8,
            include_snippets: true,
            include_ocr: false,
            include_semantic: true,
        });
        let payload = encode_message_payload(&request).expect("encode search request");
        let decoded: VisionRequest = decode_message_payload(&payload).expect("decode search");

        match decoded {
            VisionRequest::Search(job) => {
                assert_eq!(job.request_id, "search_1");
                assert_eq!(job.mode, SearchMode::Hybrid);
                assert_eq!(job.root_hint.as_deref(), Some("./src"));
                assert!(job.include_semantic);
            }
            _ => panic!("unexpected decoded request"),
        }
    }

    #[test]
    fn search_response_roundtrips_through_bincode() {
        let result = JobResult::Search(SearchResponse {
            request_id: "search_1".into(),
            elapsed_ms: 12,
            mode_used: SearchMode::Locate,
            hits: vec![SearchHit {
                result_id: "file:7".into(),
                file_id: 7,
                path: "/tmp/docker-compose.yml".into(),
                title: "docker-compose".into(),
                kind: "code".into(),
                score: 42.0,
                source: SearchHitSource::FileName,
                snippet: None,
                modified_at: None,
                size_bytes: Some(128),
                requires_confirmation: false,
            }],
            diagnostics: Some(SearchDiagnostics {
                indexed_files: 1,
                indexed_chunks: 0,
                roots: vec!["/tmp".into()],
                message: None,
            }),
        });
        let payload = encode_message_payload(&result).expect("encode search response");
        let decoded: JobResult = decode_message_payload(&payload).expect("decode search response");

        match decoded {
            JobResult::Search(response) => {
                assert_eq!(response.request_id, "search_1");
                assert_eq!(response.hits[0].source, SearchHitSource::FileName);
                assert_eq!(response.diagnostics.unwrap().indexed_files, 1);
            }
            _ => panic!("unexpected decoded response"),
        }
    }

    fn encoded_variant_tag(request: &VisionRequest) -> u8 {
        *bincode::serde::encode_to_vec(request, bincode::config::standard())
            .expect("encode request")
            .first()
            .expect("encoded request has variant tag")
    }
}
