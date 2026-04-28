use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

pub const CODDY_PROTOCOL_VERSION: u16 = 1;
pub const CODDY_PROTOCOL_MAGIC: [u8; 4] = *b"CDDY";

pub type CoddyIpcResult<T> = Result<T, CoddyIpcError>;

#[derive(Debug, Error)]
pub enum CoddyIpcError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bincode encode error: {0}")]
    BincodeEncode(#[from] bincode::error::EncodeError),

    #[error("bincode decode error: {0}")]
    BincodeDecode(#[from] bincode::error::DecodeError),

    #[error("incompatible Coddy protocol version: expected {expected}, got {actual}")]
    IncompatibleProtocolVersion { expected: u16, actual: u16 },

    #[error("invalid Coddy protocol magic: {actual:?}")]
    InvalidMagic { actual: [u8; 4] },

    #[error("trailing bytes after Coddy payload: decoded {decoded} of {total} bytes")]
    TrailingBytes { decoded: usize, total: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoddyEnvelope<T> {
    pub protocol_version: u16,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoddyWireRequest {
    pub magic: [u8; 4],
    pub protocol_version: u16,
    pub request: CoddyRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoddyWireResult {
    pub magic: [u8; 4],
    pub protocol_version: u16,
    pub result: CoddyResult,
}

impl<T> CoddyEnvelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            protocol_version: CODDY_PROTOCOL_VERSION,
            payload,
        }
    }

    pub fn is_compatible(&self) -> bool {
        self.protocol_version == CODDY_PROTOCOL_VERSION
    }

    pub fn ensure_compatible(&self) -> CoddyIpcResult<()> {
        if self.is_compatible() {
            Ok(())
        } else {
            Err(CoddyIpcError::IncompatibleProtocolVersion {
                expected: CODDY_PROTOCOL_VERSION,
                actual: self.protocol_version,
            })
        }
    }
}

impl CoddyWireRequest {
    pub fn new(request: CoddyRequest) -> Self {
        Self {
            magic: CODDY_PROTOCOL_MAGIC,
            protocol_version: CODDY_PROTOCOL_VERSION,
            request,
        }
    }

    pub fn ensure_compatible(&self) -> CoddyIpcResult<()> {
        ensure_wire_compatible(self.magic, self.protocol_version)
    }
}

impl CoddyWireResult {
    pub fn new(result: CoddyResult) -> Self {
        Self {
            magic: CODDY_PROTOCOL_MAGIC,
            protocol_version: CODDY_PROTOCOL_VERSION,
            result,
        }
    }

    pub fn ensure_compatible(&self) -> CoddyIpcResult<()> {
        ensure_wire_compatible(self.magic, self.protocol_version)
    }
}

fn ensure_wire_compatible(magic: [u8; 4], protocol_version: u16) -> CoddyIpcResult<()> {
    if magic != CODDY_PROTOCOL_MAGIC {
        return Err(CoddyIpcError::InvalidMagic { actual: magic });
    }
    if protocol_version != CODDY_PROTOCOL_VERSION {
        return Err(CoddyIpcError::IncompatibleProtocolVersion {
            expected: CODDY_PROTOCOL_VERSION,
            actual: protocol_version,
        });
    }
    Ok(())
}

pub async fn write_frame<W, T>(writer: &mut W, value: &T) -> CoddyIpcResult<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    write_frame_payload(writer, &encode_payload(value)?).await
}

pub async fn write_frame_payload<W>(writer: &mut W, payload: &[u8]) -> CoddyIpcResult<()>
where
    W: AsyncWrite + Unpin,
{
    let length = payload.len() as u32;
    writer.write_u32(length).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R, T>(reader: &mut R) -> CoddyIpcResult<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    decode_payload(&read_frame_payload(reader).await?)
}

pub async fn read_frame_payload<R>(reader: &mut R) -> CoddyIpcResult<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let length = reader.read_u32().await? as usize;
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

pub fn encode_payload<T>(value: &T) -> CoddyIpcResult<Vec<u8>>
where
    T: Serialize,
{
    Ok(bincode::serde::encode_to_vec(
        value,
        bincode::config::standard(),
    )?)
}

pub fn decode_payload<T>(payload: &[u8]) -> CoddyIpcResult<T>
where
    T: DeserializeOwned,
{
    let (value, decoded): (T, usize) =
        bincode::serde::decode_from_slice(payload, bincode::config::standard())?;
    ensure_fully_decoded(decoded, payload.len())?;
    Ok(value)
}

pub fn decode_wire_request_payload(payload: &[u8]) -> CoddyIpcResult<Option<CoddyRequest>> {
    if let Ok((request, decoded)) = bincode::serde::decode_from_slice::<CoddyWireRequest, _>(
        payload,
        bincode::config::standard(),
    ) {
        if request.magic == CODDY_PROTOCOL_MAGIC {
            ensure_fully_decoded(decoded, payload.len())?;
            request.ensure_compatible()?;
            return Ok(Some(request.request));
        }
    }

    Ok(None)
}

pub fn decode_wire_result_payload(payload: &[u8]) -> CoddyIpcResult<Option<CoddyResult>> {
    if let Ok((result, decoded)) = bincode::serde::decode_from_slice::<CoddyWireResult, _>(
        payload,
        bincode::config::standard(),
    ) {
        if result.magic == CODDY_PROTOCOL_MAGIC {
            ensure_fully_decoded(decoded, payload.len())?;
            result.ensure_compatible()?;
            return Ok(Some(result.result));
        }
    }

    Ok(None)
}

fn ensure_fully_decoded(decoded: usize, total: usize) -> CoddyIpcResult<()> {
    if decoded == total {
        Ok(())
    } else {
        Err(CoddyIpcError::TrailingBytes { decoded, total })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplCommandJob {
    pub request_id: Uuid,
    pub command: coddy_core::ReplCommand,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSessionSnapshotJob {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplEventsJob {
    pub request_id: Uuid,
    pub after_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplEventStreamJob {
    pub request_id: Uuid,
    pub after_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CoddyRequest {
    Command(ReplCommandJob),
    SessionSnapshot(ReplSessionSnapshotJob),
    Events(ReplEventsJob),
    EventStream(ReplEventStreamJob),
}

impl CoddyRequest {
    pub fn request_id(&self) -> Uuid {
        match self {
            Self::Command(job) => job.request_id,
            Self::SessionSnapshot(job) => job.request_id,
            Self::Events(job) => job.request_id,
            Self::EventStream(job) => job.request_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CoddyResult {
    Text {
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
    ReplEvents {
        request_id: Uuid,
        events: Vec<coddy_core::ReplEventEnvelope>,
        last_sequence: u64,
    },
}

impl CoddyResult {
    pub fn request_id(&self) -> Uuid {
        match self {
            Self::Text { request_id, .. }
            | Self::BrowserQuery { request_id, .. }
            | Self::ActionStatus { request_id, .. }
            | Self::Error { request_id, .. }
            | Self::ReplSessionSnapshot { request_id, .. }
            | Self::ReplEvents { request_id, .. } => *request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_uses_current_protocol_version() {
        let envelope = CoddyEnvelope::new("payload");

        assert_eq!(envelope.protocol_version, CODDY_PROTOCOL_VERSION);
        assert!(envelope.is_compatible());
    }

    #[test]
    fn detects_incompatible_protocol_version() {
        let envelope = CoddyEnvelope {
            protocol_version: CODDY_PROTOCOL_VERSION + 1,
            payload: "payload",
        };

        assert!(!envelope.is_compatible());
        assert!(matches!(
            envelope.ensure_compatible(),
            Err(CoddyIpcError::IncompatibleProtocolVersion { .. })
        ));
    }

    #[test]
    fn repl_jobs_keep_request_ids_and_sequences() {
        let request_id = Uuid::new_v4();

        let events = ReplEventsJob {
            request_id,
            after_sequence: 42,
        };
        let stream = ReplEventStreamJob {
            request_id,
            after_sequence: 43,
        };

        assert_eq!(events.request_id, request_id);
        assert_eq!(events.after_sequence, 42);
        assert_eq!(stream.request_id, request_id);
        assert_eq!(stream.after_sequence, 43);
    }

    #[test]
    fn coddy_request_exposes_request_id_for_all_variants() {
        let request_id = Uuid::new_v4();

        let requests = [
            CoddyRequest::Command(ReplCommandJob {
                request_id,
                command: coddy_core::ReplCommand::StopSpeaking,
                speak: false,
            }),
            CoddyRequest::SessionSnapshot(ReplSessionSnapshotJob { request_id }),
            CoddyRequest::Events(ReplEventsJob {
                request_id,
                after_sequence: 1,
            }),
            CoddyRequest::EventStream(ReplEventStreamJob {
                request_id,
                after_sequence: 1,
            }),
        ];

        for request in requests {
            assert_eq!(request.request_id(), request_id);
        }
    }

    #[test]
    fn repl_command_job_roundtrips_through_bincode() {
        let request_id = Uuid::new_v4();
        let job = ReplCommandJob {
            request_id,
            command: coddy_core::ReplCommand::StopActiveRun,
            speak: true,
        };

        let payload =
            bincode::serde::encode_to_vec(&job, bincode::config::standard()).expect("encode job");
        let (decoded, _): (ReplCommandJob, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode job");

        assert_eq!(decoded, job);
    }

    #[test]
    fn protocol_envelope_roundtrips_through_bincode() {
        let envelope = CoddyEnvelope::new(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 7,
        });

        let payload = bincode::serde::encode_to_vec(&envelope, bincode::config::standard())
            .expect("encode envelope");
        let (decoded, _): (CoddyEnvelope<ReplEventsJob>, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode envelope");

        assert_eq!(decoded, envelope);
        assert!(decoded.is_compatible());
    }

    #[test]
    fn coddy_request_roundtrips_through_bincode() {
        let request = CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: coddy_core::ReplCommand::StopSpeaking,
            speak: false,
        });

        let payload = bincode::serde::encode_to_vec(&request, bincode::config::standard())
            .expect("encode request");
        let (decoded, _): (CoddyRequest, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode request");

        assert_eq!(decoded, request);
    }

    #[test]
    fn coddy_wire_request_validates_magic_and_version() {
        let request = CoddyWireRequest::new(CoddyRequest::Events(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 0,
        }));

        assert_eq!(request.magic, CODDY_PROTOCOL_MAGIC);
        assert_eq!(request.protocol_version, CODDY_PROTOCOL_VERSION);
        assert!(request.ensure_compatible().is_ok());
    }

    #[test]
    fn coddy_wire_result_rejects_invalid_magic() {
        let mut result = CoddyWireResult::new(CoddyResult::ActionStatus {
            request_id: Uuid::new_v4(),
            message: "ok".to_string(),
            spoken: false,
        });
        result.magic = *b"NOPE";

        assert!(matches!(
            result.ensure_compatible(),
            Err(CoddyIpcError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn decode_wire_request_payload_returns_request_for_coddy_magic() {
        let request_id = Uuid::new_v4();
        let payload = encode_payload(&CoddyWireRequest::new(CoddyRequest::Events(
            ReplEventsJob {
                request_id,
                after_sequence: 9,
            },
        )))
        .expect("encode wire request");

        let decoded = decode_wire_request_payload(&payload)
            .expect("decode wire request")
            .expect("coddy request");

        let CoddyRequest::Events(job) = decoded else {
            panic!("unexpected coddy request")
        };
        assert_eq!(job.request_id, request_id);
        assert_eq!(job.after_sequence, 9);
    }

    #[test]
    fn decode_wire_request_payload_ignores_non_wire_payloads() {
        let legacy_like_payload = encode_payload(&CoddyRequest::Events(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 9,
        }))
        .expect("encode non-wire request");

        assert!(decode_wire_request_payload(&legacy_like_payload)
            .expect("decode non-wire payload")
            .is_none());
    }

    #[test]
    fn decode_wire_request_payload_rejects_incompatible_version() {
        let mut request = CoddyWireRequest::new(CoddyRequest::Events(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 9,
        }));
        request.protocol_version += 1;
        let payload = encode_payload(&request).expect("encode wire request");

        assert!(matches!(
            decode_wire_request_payload(&payload),
            Err(CoddyIpcError::IncompatibleProtocolVersion { .. })
        ));
    }

    #[test]
    fn decode_wire_result_payload_returns_result_for_coddy_magic() {
        let request_id = Uuid::new_v4();
        let payload = encode_payload(&CoddyWireResult::new(CoddyResult::ActionStatus {
            request_id,
            message: "ok".to_string(),
            spoken: false,
        }))
        .expect("encode wire result");

        let decoded = decode_wire_result_payload(&payload)
            .expect("decode wire result")
            .expect("coddy result");

        let CoddyResult::ActionStatus {
            request_id: decoded_id,
            message,
            spoken,
        } = decoded
        else {
            panic!("unexpected coddy result")
        };
        assert_eq!(decoded_id, request_id);
        assert_eq!(message, "ok");
        assert!(!spoken);
    }

    #[test]
    fn coddy_result_roundtrips_through_bincode() {
        let result = CoddyResult::ActionStatus {
            request_id: Uuid::new_v4(),
            message: "ok".to_string(),
            spoken: false,
        };

        let payload = bincode::serde::encode_to_vec(&result, bincode::config::standard())
            .expect("encode result");
        let (decoded, _): (CoddyResult, usize) =
            bincode::serde::decode_from_slice(&payload, bincode::config::standard())
                .expect("decode result");

        assert_eq!(decoded, result);
    }

    #[test]
    fn coddy_result_exposes_request_id_for_all_variants() {
        let request_id = Uuid::new_v4();
        let session = coddy_core::ReplSession::new(
            coddy_core::ReplMode::FloatingTerminal,
            coddy_core::ModelRef {
                provider: "ollama".to_string(),
                name: "gemma4:e2b".to_string(),
            },
        );
        let event = coddy_core::ReplEventEnvelope::new(
            1,
            session.id,
            None,
            1_775_000_000_000,
            coddy_core::ReplEvent::VoiceListeningStarted,
        );

        let results = [
            CoddyResult::Text {
                request_id,
                text: "ok".to_string(),
                spoken: false,
            },
            CoddyResult::BrowserQuery {
                request_id,
                query: "rust".to_string(),
                summary: None,
                spoken: false,
            },
            CoddyResult::ActionStatus {
                request_id,
                message: "ok".to_string(),
                spoken: false,
            },
            CoddyResult::Error {
                request_id,
                code: "error".to_string(),
                message: "boom".to_string(),
            },
            CoddyResult::ReplSessionSnapshot {
                request_id,
                snapshot: Box::new(coddy_core::ReplSessionSnapshot {
                    session,
                    last_sequence: 1,
                }),
            },
            CoddyResult::ReplEvents {
                request_id,
                events: vec![event],
                last_sequence: 1,
            },
        ];

        for result in results {
            assert_eq!(result.request_id(), request_id);
        }
    }

    #[test]
    fn decode_payload_rejects_trailing_bytes() {
        let request = CoddyRequest::Events(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 9,
        });
        let mut payload = encode_payload(&request).expect("encode request");
        payload.extend_from_slice(&[0, 1, 2]);

        assert!(matches!(
            decode_payload::<CoddyRequest>(&payload),
            Err(CoddyIpcError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn decode_wire_request_payload_rejects_trailing_bytes_for_coddy_magic() {
        let request = CoddyWireRequest::new(CoddyRequest::Events(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 9,
        }));
        let mut payload = encode_payload(&request).expect("encode wire request");
        payload.extend_from_slice(&[0, 1, 2]);

        assert!(matches!(
            decode_wire_request_payload(&payload),
            Err(CoddyIpcError::TrailingBytes { .. })
        ));
    }

    #[tokio::test]
    async fn frame_helpers_roundtrip_values() {
        let mut buffer = Vec::new();
        let request = CoddyWireRequest::new(CoddyRequest::Events(ReplEventsJob {
            request_id: Uuid::new_v4(),
            after_sequence: 9,
        }));

        write_frame(&mut buffer, &request)
            .await
            .expect("write frame");
        let decoded: CoddyWireRequest = read_frame(&mut buffer.as_slice())
            .await
            .expect("read frame");

        assert_eq!(decoded, request);
        assert!(decoded.ensure_compatible().is_ok());
    }
}
