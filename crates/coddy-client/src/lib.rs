use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use coddy_core::{ReplCommand, ReplEventEnvelope, ReplSessionSnapshot};
use coddy_ipc::{
    read_frame, write_frame, CoddyIpcError, CoddyRequest, CoddyResult, CoddyWireRequest,
    CoddyWireResult, ReplCommandJob, ReplEventStreamJob, ReplEventsJob, ReplSessionSnapshotJob,
};
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CoddyClient {
    socket_path: PathBuf,
    options: CoddyClientOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoddyClientOptions {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub reconnect_initial_delay: Duration,
    pub reconnect_max_delay: Duration,
}

impl Default for CoddyClientOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(180),
            reconnect_initial_delay: Duration::from_millis(50),
            reconnect_max_delay: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplEventsBatch {
    pub request_id: Uuid,
    pub events: Vec<ReplEventEnvelope>,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplEventStreamFrame {
    pub request_id: Uuid,
    pub event: ReplEventEnvelope,
    pub last_sequence: u64,
}

pub struct ReplEventStream {
    stream: UnixStream,
    request_id: Uuid,
    last_sequence: u64,
}

pub struct ReplEventWatcher {
    client: CoddyClient,
    stream: Option<ReplEventStream>,
    last_sequence: u64,
    reconnect_delay: Duration,
}

impl CoddyClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            options: CoddyClientOptions::default(),
        }
    }

    pub fn with_options(socket_path: impl Into<PathBuf>, options: CoddyClientOptions) -> Self {
        Self {
            socket_path: socket_path.into(),
            options,
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn send_command(&self, command: ReplCommand, speak: bool) -> Result<CoddyResult> {
        let request_id = Uuid::new_v4();
        self.roundtrip(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command,
            speak,
        }))
        .await
    }

    pub async fn stop_speaking(&self) -> Result<CoddyResult> {
        self.send_command(ReplCommand::StopSpeaking, false).await
    }

    pub async fn stop_active_run(&self) -> Result<CoddyResult> {
        self.send_command(ReplCommand::StopActiveRun, false).await
    }

    pub async fn snapshot(&self) -> Result<ReplSessionSnapshot> {
        let request_id = Uuid::new_v4();
        match self
            .roundtrip(CoddyRequest::SessionSnapshot(ReplSessionSnapshotJob {
                request_id,
            }))
            .await?
        {
            CoddyResult::ReplSessionSnapshot { snapshot, .. } => Ok(*snapshot),
            CoddyResult::Error { code, message, .. } => {
                bail!("daemon returned error {code}: {message}")
            }
            _ => bail!("daemon returned unexpected response for REPL session snapshot"),
        }
    }

    pub async fn events_after(&self, after_sequence: u64) -> Result<ReplEventsBatch> {
        let request_id = Uuid::new_v4();
        match self
            .roundtrip(CoddyRequest::Events(ReplEventsJob {
                request_id,
                after_sequence,
            }))
            .await?
        {
            CoddyResult::ReplEvents {
                request_id,
                events,
                last_sequence,
            } => Ok(ReplEventsBatch {
                request_id,
                events,
                last_sequence,
            }),
            CoddyResult::Error { code, message, .. } => {
                bail!("daemon returned error {code}: {message}")
            }
            _ => bail!("daemon returned unexpected response for REPL session events"),
        }
    }

    pub async fn event_stream(&self, after_sequence: u64) -> Result<ReplEventStream> {
        let request_id = Uuid::new_v4();
        let mut stream = self.connect().await?;
        let request = CoddyWireRequest::new(CoddyRequest::EventStream(ReplEventStreamJob {
            request_id,
            after_sequence,
        }));
        self.with_request_timeout(write_frame(&mut stream, &request), "open REPL event stream")
            .await?;

        Ok(ReplEventStream {
            stream,
            request_id,
            last_sequence: after_sequence,
        })
    }

    pub async fn event_watcher(&self, after_sequence: u64) -> Result<ReplEventWatcher> {
        Ok(ReplEventWatcher {
            client: self.clone(),
            stream: Some(self.event_stream(after_sequence).await?),
            last_sequence: after_sequence,
            reconnect_delay: self.options.reconnect_initial_delay,
        })
    }

    async fn roundtrip(&self, request: CoddyRequest) -> Result<CoddyResult> {
        let expected_request_id = request.request_id();
        let mut stream = self.connect().await?;
        let response: CoddyWireResult = self
            .with_request_timeout(
                async {
                    write_frame(&mut stream, &CoddyWireRequest::new(request)).await?;
                    read_frame(&mut stream).await
                },
                "Coddy daemon request",
            )
            .await?;
        response.ensure_compatible()?;
        let result = response.result;
        ensure_response_request_id(expected_request_id, result.request_id())?;
        Ok(result)
    }

    async fn connect(&self) -> Result<UnixStream> {
        timeout(
            self.options.connect_timeout,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "timed out connecting to daemon socket {} after {} ms",
                self.socket_path.display(),
                self.options.connect_timeout.as_millis()
            )
        })?
        .with_context(|| {
            format!(
                "failed to connect to daemon socket {}",
                self.socket_path.display()
            )
        })
    }

    async fn with_request_timeout<F, T>(&self, operation: F, label: &str) -> Result<T>
    where
        F: std::future::Future<Output = coddy_ipc::CoddyIpcResult<T>>,
    {
        timeout(self.options.request_timeout, operation)
            .await
            .map_err(|_| {
                anyhow!(
                    "{label} timed out after {} ms",
                    self.options.request_timeout.as_millis()
                )
            })?
            .map_err(Into::into)
    }
}

impl ReplEventStream {
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub async fn next(&mut self) -> Result<Option<ReplEventStreamFrame>> {
        match read_frame::<_, CoddyWireResult>(&mut self.stream).await {
            Ok(response) => {
                response.ensure_compatible()?;
                match response.result {
                    CoddyResult::ReplEvents {
                        request_id,
                        events,
                        last_sequence,
                    } => {
                        ensure_response_request_id(self.request_id, request_id)?;
                        if events.len() != 1 {
                            bail!(
                                "daemon returned invalid REPL event stream frame with {} events",
                                events.len()
                            );
                        }
                        let event = events
                            .into_iter()
                            .next()
                            .context("daemon returned empty REPL event stream frame")?;
                        ensure_advancing_sequence(self.last_sequence, event.sequence)?;
                        ensure_stream_last_sequence(event.sequence, last_sequence)?;
                        self.last_sequence = last_sequence;
                        Ok(Some(ReplEventStreamFrame {
                            request_id,
                            event,
                            last_sequence,
                        }))
                    }
                    CoddyResult::Error { code, message, .. } => {
                        bail!("daemon returned error {code}: {message}")
                    }
                    _ => bail!("daemon returned unexpected response for REPL event stream"),
                }
            }
            Err(CoddyIpcError::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl ReplEventWatcher {
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    pub async fn next(&mut self) -> Result<ReplEventStreamFrame> {
        loop {
            if self.stream.is_none() {
                self.reconnect_after_delay().await?;
            }

            let stream = self
                .stream
                .as_mut()
                .context("REPL event watcher stream was not initialized")?;

            match stream.next().await? {
                Some(frame) => {
                    self.last_sequence = frame.last_sequence;
                    self.reconnect_delay = self.client.options.reconnect_initial_delay;
                    return Ok(frame);
                }
                None => {
                    self.stream = None;
                }
            }
        }
    }

    async fn reconnect_after_delay(&mut self) -> Result<()> {
        loop {
            if !self.reconnect_delay.is_zero() {
                sleep(self.reconnect_delay).await;
            }

            match self.client.event_stream(self.last_sequence).await {
                Ok(stream) => {
                    self.stream = Some(stream);
                    self.reconnect_delay = next_reconnect_delay(
                        self.reconnect_delay,
                        self.client.options.reconnect_initial_delay,
                        self.client.options.reconnect_max_delay,
                    );
                    return Ok(());
                }
                Err(_) => {
                    self.reconnect_delay = next_reconnect_delay(
                        self.reconnect_delay,
                        self.client.options.reconnect_initial_delay,
                        self.client.options.reconnect_max_delay,
                    );
                }
            }
        }
    }
}

fn next_reconnect_delay(current: Duration, initial: Duration, max: Duration) -> Duration {
    if current.is_zero() {
        return initial.min(max);
    }

    current.saturating_mul(2).min(max)
}

fn ensure_response_request_id(expected: Uuid, actual: Uuid) -> Result<()> {
    if expected == actual {
        Ok(())
    } else {
        bail!("daemon returned response for request {actual}, expected {expected}")
    }
}

fn ensure_advancing_sequence(previous: u64, next: u64) -> Result<()> {
    if next > previous {
        Ok(())
    } else {
        bail!("daemon returned non-advancing REPL event sequence {next} after {previous}")
    }
}

fn ensure_stream_last_sequence(event_sequence: u64, last_sequence: u64) -> Result<()> {
    if event_sequence == last_sequence {
        Ok(())
    } else {
        bail!(
            "daemon returned REPL stream last_sequence {last_sequence}, expected event sequence {event_sequence}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn client_sends_repl_command_request() {
        let socket_path = test_socket_path("command");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::Command(job) = request.request else {
                panic!("unexpected request")
            };
            assert!(matches!(job.command, ReplCommand::StopSpeaking));
            assert!(job.speak);
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ActionStatus {
                    request_id: job.request_id,
                    message: "ok".to_string(),
                    spoken: false,
                }),
            )
            .await
            .expect("write response");
        });

        let client = CoddyClient::new(&socket_path);
        let result = client
            .send_command(ReplCommand::StopSpeaking, true)
            .await
            .expect("send command");

        assert!(matches!(result, CoddyResult::ActionStatus { .. }));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn event_stream_reads_single_frame() {
        let socket_path = test_socket_path("stream");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::EventStream(job) = request.request else {
                panic!("unexpected request")
            };
            assert_eq!(job.after_sequence, 7);
            let event = ReplEventEnvelope::new(
                8,
                Uuid::new_v4(),
                None,
                1_775_000_000_000,
                coddy_core::ReplEvent::VoiceListeningStarted,
            );
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: job.request_id,
                    events: vec![event],
                    last_sequence: 8,
                }),
            )
            .await
            .expect("write stream frame");
        });

        let client = CoddyClient::new(&socket_path);
        let mut stream = client.event_stream(7).await.expect("open stream");
        let frame = stream
            .next()
            .await
            .expect("read frame")
            .expect("stream frame");

        assert_eq!(frame.event.sequence, 8);
        assert_eq!(frame.last_sequence, 8);
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn event_stream_rejects_mismatched_request_id() {
        let socket_path = test_socket_path("stream-mismatched-request-id");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::EventStream(job) = request.request else {
                panic!("unexpected request")
            };
            let event = ReplEventEnvelope::new(
                8,
                Uuid::new_v4(),
                None,
                1_775_000_000_000,
                coddy_core::ReplEvent::VoiceListeningStarted,
            );
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: Uuid::new_v4(),
                    events: vec![event],
                    last_sequence: job.after_sequence + 1,
                }),
            )
            .await
            .expect("write stream frame");
        });

        let client = CoddyClient::new(&socket_path);
        let mut stream = client.event_stream(7).await.expect("open stream");
        let error = stream.next().await.expect_err("mismatched request id");

        assert!(error.to_string().contains("expected"));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn event_stream_rejects_non_advancing_sequence() {
        let socket_path = test_socket_path("stream-non-advancing-sequence");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::EventStream(job) = request.request else {
                panic!("unexpected request")
            };
            let event = ReplEventEnvelope::new(
                job.after_sequence,
                Uuid::new_v4(),
                None,
                1_775_000_000_000,
                coddy_core::ReplEvent::VoiceListeningStarted,
            );
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: job.request_id,
                    events: vec![event],
                    last_sequence: job.after_sequence,
                }),
            )
            .await
            .expect("write stream frame");
        });

        let client = CoddyClient::new(&socket_path);
        let mut stream = client.event_stream(7).await.expect("open stream");
        let error = stream.next().await.expect_err("non-advancing sequence");

        assert!(error.to_string().contains("non-advancing"));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn event_watcher_reconnects_after_stream_eof() {
        let socket_path = test_socket_path("watcher-reconnect");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut first_stream, _) = listener.accept().await.expect("accept first client");
            let first_request: CoddyWireRequest = read_frame(&mut first_stream)
                .await
                .expect("read first request");
            first_request
                .ensure_compatible()
                .expect("compatible first request");
            let CoddyRequest::EventStream(first_job) = first_request.request else {
                panic!("unexpected first request")
            };
            assert_eq!(first_job.after_sequence, 7);
            drop(first_stream);

            let (mut second_stream, _) = listener.accept().await.expect("accept second client");
            let second_request: CoddyWireRequest = read_frame(&mut second_stream)
                .await
                .expect("read second request");
            second_request
                .ensure_compatible()
                .expect("compatible second request");
            let CoddyRequest::EventStream(second_job) = second_request.request else {
                panic!("unexpected second request")
            };
            assert_eq!(second_job.after_sequence, 7);
            let event = ReplEventEnvelope::new(
                8,
                Uuid::new_v4(),
                None,
                1_775_000_000_000,
                coddy_core::ReplEvent::VoiceListeningStarted,
            );
            write_frame(
                &mut second_stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: second_job.request_id,
                    events: vec![event],
                    last_sequence: 8,
                }),
            )
            .await
            .expect("write reconnected stream frame");
        });

        let client = CoddyClient::with_options(
            &socket_path,
            CoddyClientOptions {
                connect_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
                reconnect_initial_delay: Duration::ZERO,
                reconnect_max_delay: Duration::ZERO,
            },
        );
        let mut watcher = client.event_watcher(7).await.expect("open watcher");
        let frame = watcher.next().await.expect("watcher frame after reconnect");

        assert_eq!(frame.last_sequence, 8);
        assert_eq!(watcher.last_sequence(), 8);
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn event_watcher_retries_failed_reconnect_until_socket_returns() {
        let socket_path = test_socket_path("watcher-retry-reconnect");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let first_socket_path = socket_path.clone();
        let (first_closed_tx, first_closed_rx) = tokio::sync::oneshot::channel();
        let first_server = tokio::spawn(async move {
            let (mut first_stream, _) = listener.accept().await.expect("accept first client");
            let first_request: CoddyWireRequest = read_frame(&mut first_stream)
                .await
                .expect("read first request");
            first_request
                .ensure_compatible()
                .expect("compatible first request");
            let CoddyRequest::EventStream(first_job) = first_request.request else {
                panic!("unexpected first request")
            };
            assert_eq!(first_job.after_sequence, 7);
            drop(first_stream);
            let _ = std::fs::remove_file(&first_socket_path);
            first_closed_tx.send(()).expect("notify first close");
        });

        let client = CoddyClient::with_options(
            &socket_path,
            CoddyClientOptions {
                connect_timeout: Duration::from_millis(10),
                request_timeout: Duration::from_secs(1),
                reconnect_initial_delay: Duration::from_millis(5),
                reconnect_max_delay: Duration::from_millis(20),
            },
        );
        let mut watcher = client.event_watcher(7).await.expect("open watcher");
        first_closed_rx.await.expect("first server closed");

        let rebound_socket_path = socket_path.clone();
        let rebound_server = tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            let listener = UnixListener::bind(&rebound_socket_path).expect("rebind test socket");
            let (mut stream, _) = listener.accept().await.expect("accept reconnected client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::EventStream(job) = request.request else {
                panic!("unexpected request")
            };
            assert_eq!(job.after_sequence, 7);
            let event = ReplEventEnvelope::new(
                8,
                Uuid::new_v4(),
                None,
                1_775_000_000_000,
                coddy_core::ReplEvent::VoiceListeningStarted,
            );
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: job.request_id,
                    events: vec![event],
                    last_sequence: 8,
                }),
            )
            .await
            .expect("write reconnected stream frame");
        });

        let frame = tokio::time::timeout(Duration::from_secs(1), watcher.next())
            .await
            .expect("watcher retried until socket returned")
            .expect("watcher frame");

        assert_eq!(frame.last_sequence, 8);
        assert_eq!(watcher.last_sequence(), 8);
        first_server.await.expect("first server task");
        rebound_server.await.expect("rebound server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn client_reads_snapshot_response() {
        let socket_path = test_socket_path("snapshot");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::SessionSnapshot(job) = request.request else {
                panic!("unexpected request")
            };
            let session = coddy_core::ReplSession::new(
                coddy_core::ReplMode::FloatingTerminal,
                coddy_core::ModelRef {
                    provider: "ollama".to_string(),
                    name: "gemma4-e2b".to_string(),
                },
            );
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ReplSessionSnapshot {
                    request_id: job.request_id,
                    snapshot: Box::new(ReplSessionSnapshot {
                        session,
                        last_sequence: 3,
                    }),
                }),
            )
            .await
            .expect("write snapshot");
        });

        let client = CoddyClient::new(&socket_path);
        let snapshot = client.snapshot().await.expect("snapshot");

        assert_eq!(snapshot.last_sequence, 3);
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn client_reads_incremental_events_response() {
        let socket_path = test_socket_path("events");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            let CoddyRequest::Events(job) = request.request else {
                panic!("unexpected request")
            };
            assert_eq!(job.after_sequence, 4);
            let event = ReplEventEnvelope::new(
                5,
                Uuid::new_v4(),
                None,
                1_775_000_000_000,
                coddy_core::ReplEvent::VoiceListeningStarted,
            );
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: job.request_id,
                    events: vec![event],
                    last_sequence: 5,
                }),
            )
            .await
            .expect("write events");
        });

        let client = CoddyClient::new(&socket_path);
        let batch = client.events_after(4).await.expect("events");

        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.last_sequence, 5);
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn client_rejects_mismatched_roundtrip_request_id() {
        let socket_path = test_socket_path("mismatched-request-id");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let request: CoddyWireRequest = read_frame(&mut stream).await.expect("read request");
            request.ensure_compatible().expect("compatible request");
            write_frame(
                &mut stream,
                &CoddyWireResult::new(CoddyResult::ActionStatus {
                    request_id: Uuid::new_v4(),
                    message: "wrong request".to_string(),
                    spoken: false,
                }),
            )
            .await
            .expect("write response");
        });

        let client = CoddyClient::new(&socket_path);
        let error = client
            .send_command(ReplCommand::StopSpeaking, false)
            .await
            .expect_err("mismatched request id");

        assert!(error.to_string().contains("expected"));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn client_times_out_waiting_for_roundtrip_response() {
        let socket_path = test_socket_path("timeout");
        let listener = UnixListener::bind(&socket_path).expect("bind test socket");
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept client");
            sleep(Duration::from_millis(75)).await;
        });

        let client = CoddyClient::with_options(
            &socket_path,
            CoddyClientOptions {
                connect_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_millis(10),
                reconnect_initial_delay: Duration::ZERO,
                reconnect_max_delay: Duration::ZERO,
            },
        );
        let error = client
            .send_command(ReplCommand::StopSpeaking, false)
            .await
            .expect_err("timeout error");

        assert!(error.to_string().contains("timed out"));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    fn test_socket_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("coddy-client-{name}-{}.sock", Uuid::new_v4()))
    }

    #[test]
    fn reconnect_delay_backs_off_until_max() {
        assert_eq!(
            next_reconnect_delay(
                Duration::ZERO,
                Duration::from_millis(50),
                Duration::from_millis(500),
            ),
            Duration::from_millis(50)
        );
        assert_eq!(
            next_reconnect_delay(
                Duration::from_millis(50),
                Duration::from_millis(50),
                Duration::from_millis(500),
            ),
            Duration::from_millis(100)
        );
        assert_eq!(
            next_reconnect_delay(
                Duration::from_millis(400),
                Duration::from_millis(50),
                Duration::from_millis(500),
            ),
            Duration::from_millis(500)
        );
    }
}
