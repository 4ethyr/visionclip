use crate::backend::{InferenceBackend, InferenceInput, InferenceOutput};
use crate::prompts::{policy_for_action, system_prompt, user_prompt};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::json;
use visionclip_common::config::InferConfig;

#[derive(Debug, Clone)]
pub struct OllamaBackend {
    client: Client,
    config: InferConfig,
}

impl OllamaBackend {
    pub fn new(config: InferConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    fn chat_payload(&self, input: &InferenceInput, include_thinking: bool) -> serde_json::Value {
        let policy = policy_for_action(&input.action);
        let image_b64 = STANDARD.encode(&input.image_bytes);
        let mut payload = json!({
            "model": self.config.model,
            "stream": false,
            "keep_alive": self.config.keep_alive,
            "options": {
                "temperature": self.config.temperature
            },
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt(policy)
                },
                {
                    "role": "user",
                    "content": user_prompt(&input.action),
                    "images": [image_b64]
                }
            ]
        });

        if include_thinking && !self.config.thinking_default.trim().is_empty() {
            payload["think"] = json!(self.config.thinking_default);
        }

        payload
    }
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: Option<OllamaMessage>,
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OllamaModelSummary {
    pub name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub modified_at: String,
    pub size: u64,
    #[serde(default)]
    pub digest: String,
    #[serde(default)]
    pub details: OllamaModelDetails,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct OllamaModelDetails {
    #[serde(default)]
    pub parent_model: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub families: Vec<String>,
    #[serde(default)]
    pub parameter_size: String,
    #[serde(default)]
    pub quantization_level: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModelSummary>,
}

#[async_trait]
impl InferenceBackend for OllamaBackend {
    async fn infer(&self, input: InferenceInput) -> Result<InferenceOutput> {
        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));
        let mut include_thinking = !self.config.thinking_default.trim().is_empty();

        let response = loop {
            let payload = self.chat_payload(&input, include_thinking);
            let response = self
                .client
                .post(&url)
                .json(&payload)
                .send()
                .await
                .context("failed to call Ollama")?;

            if response.status().is_success() {
                break response;
            }

            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read Ollama error body".to_string());

            let thinking_unsupported = include_thinking
                && status == StatusCode::BAD_REQUEST
                && body.contains("does not support thinking");

            if thinking_unsupported {
                include_thinking = false;
                continue;
            }

            let body = body.trim();
            if body.is_empty() {
                return Err(anyhow!("Ollama returned {}", status));
            }

            return Err(anyhow!("Ollama returned {}: {}", status, body));
        };

        let decoded: OllamaResponse = response
            .json()
            .await
            .context("failed to parse Ollama response")?;

        let content = decoded
            .message
            .map(|message| message.content)
            .or(decoded.response)
            .ok_or_else(|| anyhow!("Ollama response did not contain text content"))?;

        Ok(InferenceOutput { text: content })
    }
}

pub async fn list_models(base_url: &str) -> Result<Vec<OllamaModelSummary>> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let response = Client::new()
        .get(url)
        .send()
        .await
        .context("failed to call Ollama model listing endpoint")?
        .error_for_status()
        .context("Ollama model listing returned an error status")?;

    let decoded: OllamaTagsResponse = response
        .json()
        .await
        .context("failed to parse Ollama model listing response")?;

    Ok(decoded.models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };
    use visionclip_common::ipc::Action;

    #[derive(Clone, Copy)]
    struct TestResponse {
        status_line: &'static str,
        body: &'static str,
    }

    struct TestServer {
        base_url: String,
        request_rx: mpsc::Receiver<Vec<(String, String)>>,
        handle: thread::JoinHandle<()>,
    }

    impl TestServer {
        fn spawn(response_body: &'static str) -> Self {
            Self::spawn_sequence(vec![TestResponse {
                status_line: "200 OK",
                body: response_body,
            }])
        }

        fn spawn_sequence(responses: Vec<TestResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (request_tx, request_rx) = mpsc::channel();

            let handle = thread::spawn(move || {
                let mut requests = Vec::new();

                for response in responses {
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
                    requests.push((headers, body));

                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.status_line,
                        response.body.len(),
                        response.body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                    stream.flush().unwrap();
                }

                request_tx.send(requests).unwrap();
            });

            Self {
                base_url: format!("http://{}", address),
                request_rx,
                handle,
            }
        }

        fn finish(self) -> Vec<(String, String)> {
            let request = self.request_rx.recv().unwrap();
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
    async fn infer_posts_image_payload_to_ollama() {
        let server = TestServer::spawn(r#"{"message":{"content":"resultado final"}}"#);
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            model: "gemma4:test".into(),
            keep_alive: "5m".into(),
            temperature: 0.2,
            thinking_default: "low".into(),
            ..InferConfig::default()
        });

        let output = backend
            .infer(InferenceInput {
                action: Action::Explain,
                image_bytes: b"PNG".to_vec(),
                mime_type: "image/png".into(),
            })
            .await
            .unwrap();

        let requests = server.finish();
        let (headers, body) = &requests[0];
        assert!(headers.starts_with("POST /api/chat HTTP/1.1"));
        assert_eq!(output.text, "resultado final");

        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["model"], "gemma4:test");
        assert_eq!(json["messages"][1]["images"][0], "UE5H");
        assert_eq!(
            json["messages"][1]["content"],
            "Explique tecnicamente o conteúdo desta captura."
        );
    }

    #[tokio::test]
    async fn infer_retries_without_thinking_when_model_rejects_it() {
        let server = TestServer::spawn_sequence(vec![
            TestResponse {
                status_line: "400 Bad Request",
                body: r#"{"error":"\"gemma4:test\" does not support thinking"}"#,
            },
            TestResponse {
                status_line: "200 OK",
                body: r#"{"message":{"content":"sem thinking"}}"#,
            },
        ]);
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            model: "gemma4:test".into(),
            thinking_default: "low".into(),
            ..InferConfig::default()
        });

        let output = backend
            .infer(InferenceInput {
                action: Action::Explain,
                image_bytes: b"PNG".to_vec(),
                mime_type: "image/png".into(),
            })
            .await
            .unwrap();

        let requests = server.finish();
        assert_eq!(requests.len(), 2);

        let first_json: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
        let second_json: serde_json::Value = serde_json::from_str(&requests[1].1).unwrap();
        assert_eq!(first_json["think"], "low");
        assert!(second_json.get("think").is_none());
        assert_eq!(output.text, "sem thinking");
    }

    #[tokio::test]
    async fn list_models_reads_tags_endpoint() {
        let server = TestServer::spawn(
            r#"{"models":[{"name":"gemma4:test","model":"gemma4:test","modified_at":"2026-04-23T22:34:41-03:00","size":42,"digest":"abc","details":{"format":"gguf","family":"gemma4","families":["gemma4"],"parameter_size":"4.65B","quantization_level":"Q6_K_P"}}]}"#,
        );

        let models = list_models(&server.base_url).await.unwrap();
        let requests = server.finish();
        let (headers, _) = &requests[0];

        assert!(headers.starts_with("GET /api/tags HTTP/1.1"));
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "gemma4:test");
        assert_eq!(models[0].details.family, "gemma4");
    }
}
