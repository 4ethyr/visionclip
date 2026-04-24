use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use tracing::warn;
use uuid::Uuid;
use visionclip_common::config::AudioConfig;
use which::which;

#[derive(Debug, Clone)]
pub struct PiperHttpClient {
    client: Client,
    config: AudioConfig,
}

#[derive(Debug, Serialize)]
struct PiperSynthesisRequest<'a> {
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice: Option<&'a str>,
}

impl PiperHttpClient {
    pub fn new(config: AudioConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    pub async fn synthesize(&self, text: &str, voice_override: Option<&str>) -> Result<Vec<u8>> {
        let url = self.config.base_url.trim_end_matches('/').to_string();
        let request = PiperSynthesisRequest {
            text,
            voice: voice_override.filter(|voice| !voice.is_empty()),
        };

        let response = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await
            .context("failed to call Piper HTTP server")?
            .error_for_status()
            .context("Piper HTTP server returned an error status")?;

        let bytes = response.bytes().await.context("failed to read WAV bytes")?;
        Ok(bytes.to_vec())
    }

    pub fn play_wav(&self, wav_bytes: &[u8]) -> Result<()> {
        let temp_path = temp_wav_path()?;
        fs::write(&temp_path, wav_bytes).context("failed to write temporary WAV file")?;

        let player = preferred_player(&self.config);

        let status = Command::new(&player)
            .arg(&temp_path)
            .status()
            .with_context(|| format!("failed to execute audio player `{player}`"))?;

        if !status.success() {
            warn!(
                player,
                ?status,
                "audio player exited with non-success status"
            );
        }

        let _ = fs::remove_file(&temp_path);
        Ok(())
    }
}

fn temp_wav_path() -> Result<PathBuf> {
    let base = env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir());
    Ok(base.join(format!("visionclip-{}.wav", Uuid::new_v4())))
}

fn preferred_player(config: &AudioConfig) -> String {
    let mut candidates = Vec::new();
    let configured = config.player_command.trim();

    if !configured.is_empty() {
        candidates.push(configured.to_string());
    }

    for candidate in ["paplay", "pw-play", "aplay"] {
        if candidates.iter().all(|entry| entry != candidate) {
            candidates.push(candidate.to_string());
        }
    }

    candidates
        .into_iter()
        .find(|candidate| command_exists(candidate))
        .unwrap_or_else(|| {
            if configured.is_empty() {
                "paplay".to_string()
            } else {
                configured.to_string()
            }
        })
}

fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        Path::new(command).exists()
    } else {
        which(command).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };

    struct TestServer {
        base_url: String,
        request_rx: mpsc::Receiver<(String, String)>,
        handle: thread::JoinHandle<()>,
    }

    impl TestServer {
        fn spawn(response_body: &'static [u8], content_type: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (request_tx, request_rx) = mpsc::channel();

            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];

                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);

                    if header_end(&request).is_some() {
                        break;
                    }
                }

                let header_end = header_end(&request).unwrap();
                let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
                let content_length = content_length(&headers);
                let mut body = request[header_end + 4..].to_vec();

                while body.len() < content_length {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    body.extend_from_slice(&buffer[..read]);
                }

                let body = String::from_utf8(body).unwrap();
                request_tx.send((headers, body)).unwrap();

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    content_type,
                    response_body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.write_all(response_body).unwrap();
                stream.flush().unwrap();
            });

            Self {
                base_url: format!("http://{}", address),
                request_rx,
                handle,
            }
        }

        fn finish(self) -> (String, String) {
            let request = self
                .request_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            self.handle.join().unwrap();
            request
        }
    }

    fn header_end(request: &[u8]) -> Option<usize> {
        request.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn content_length(headers: &str) -> usize {
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn synthesize_posts_text_to_piper() {
        let server = TestServer::spawn(b"WAVDATA", "audio/wav");
        let client = PiperHttpClient::new(AudioConfig {
            base_url: server.base_url.clone(),
            ..AudioConfig::default()
        });

        let wav = client
            .synthesize("teste de audio", Some("pt-br"))
            .await
            .unwrap();
        let (headers, body) = server.finish();

        assert!(headers.starts_with("POST / HTTP/1.1"));
        assert_eq!(wav, b"WAVDATA");

        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["text"], "teste de audio");
        assert_eq!(json["voice"], "pt-br");
    }

    #[test]
    fn play_wav_uses_configured_player() {
        let client = PiperHttpClient::new(AudioConfig {
            player_command: "true".into(),
            ..AudioConfig::default()
        });

        client.play_wav(b"RIFF....WAVE").unwrap();
    }

    #[test]
    fn preferred_player_falls_back_to_available_binary() {
        let selected = preferred_player(&AudioConfig {
            player_command: "definitely-not-installed-player".into(),
            ..AudioConfig::default()
        });

        assert!(!selected.is_empty());
    }
}
