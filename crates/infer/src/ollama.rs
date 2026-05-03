use crate::backend::{
    EmbeddingBackend, EmbeddingInput, EmbeddingOutput, InferenceBackend, InferenceInput,
    InferenceOutput,
};
use crate::prompts::{
    policy_for_action, repl_agent_system_prompt, repl_agent_user_prompt,
    search_answer_system_prompt, search_answer_user_prompt, system_prompt, user_prompt,
    user_prompt_from_text,
};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::json;
use std::time::Instant;
use tracing::info;
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

    pub fn has_ocr_model(&self) -> bool {
        !self.config.ocr_model.trim().is_empty()
    }

    pub fn has_embedding_model(&self) -> bool {
        !self.config.embedding_model.trim().is_empty()
    }

    pub async fn embed_texts(
        &self,
        request_id: String,
        texts: Vec<String>,
    ) -> Result<EmbeddingOutput> {
        self.embed(EmbeddingInput { request_id, texts }).await
    }

    pub async fn infer_with_ocr_model(
        &self,
        request_id: String,
        action: visionclip_common::ipc::Action,
        image_bytes: Vec<u8>,
        mime_type: String,
    ) -> Result<InferenceOutput> {
        let input = InferenceInput {
            request_id,
            action,
            source_app: None,
            image_bytes,
            mime_type,
        };
        let model = if self.has_ocr_model() {
            self.config.ocr_model.as_str()
        } else {
            self.config.model.as_str()
        };

        self.infer_image_with_model(&input, model).await
    }

    pub async fn infer_from_text(
        &self,
        request_id: String,
        action: visionclip_common::ipc::Action,
        source_app: Option<String>,
        ocr_text: String,
    ) -> Result<InferenceOutput> {
        let model = self.config.model.as_str();
        let payload = self.text_chat_payload(model, &action, source_app.as_deref(), &ocr_text);
        self.send_chat_request(&request_id, &action, model, "text", payload)
            .await
    }

    pub async fn translate_document_text(
        &self,
        request_id: String,
        target_language: &str,
        source_text: &str,
    ) -> Result<InferenceOutput> {
        let target_language = target_language.trim();
        if target_language.is_empty() {
            return Err(anyhow!(
                "document translation target language cannot be empty"
            ));
        }
        let source_text = source_text.trim();
        if source_text.is_empty() {
            return Err(anyhow!("document translation source text cannot be empty"));
        }

        let action = visionclip_common::ipc::Action::TranslatePtBr;
        let model = self.config.model.as_str();
        let payload = json!({
            "model": model,
            "stream": false,
            "keep_alive": self.config.keep_alive,
            "options": ollama_options(
                &self.config,
                document_translation_num_predict(source_text)
            ),
            "messages": [
                {
                    "role": "system",
                    "content": document_translation_system_prompt()
                },
                {
                    "role": "user",
                    "content": document_translation_user_prompt(target_language, source_text)
                }
            ]
        });

        self.send_chat_request(&request_id, &action, model, "document_text", payload)
            .await
    }

    pub async fn answer_search_from_context(
        &self,
        request_id: String,
        query: &str,
        source_label: &str,
        ai_overview_text: &str,
        supporting_sources: &str,
    ) -> Result<InferenceOutput> {
        let action = visionclip_common::ipc::Action::SearchWeb;
        let model = self.config.model.as_str();
        let payload = self.search_answer_chat_payload(
            model,
            query,
            source_label,
            ai_overview_text,
            supporting_sources,
        );
        self.send_chat_request(&request_id, &action, model, "search_answer", payload)
            .await
    }

    pub async fn answer_repl_turn(
        &self,
        request_id: String,
        user_message: &str,
    ) -> Result<InferenceOutput> {
        let action = visionclip_common::ipc::Action::Explain;
        let model = self.config.model.as_str();
        let payload = self.repl_agent_chat_payload(model, user_message);
        self.send_chat_request(&request_id, &action, model, "repl_agent", payload)
            .await
    }

    fn image_chat_payload(&self, model: &str, input: &InferenceInput) -> serde_json::Value {
        let policy = policy_for_action(&input.action);
        let image_b64 = STANDARD.encode(&input.image_bytes);
        json!({
            "model": model,
            "stream": false,
            "keep_alive": self.config.keep_alive,
            "options": ollama_options(
                &self.config,
                num_predict_for_action(&input.action, "image")
            ),
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt(policy)
                },
                {
                    "role": "user",
                    "content": user_prompt(&input.action, input.source_app.as_deref()),
                    "images": [image_b64]
                }
            ]
        })
    }

    fn text_chat_payload(
        &self,
        model: &str,
        action: &visionclip_common::ipc::Action,
        source_app: Option<&str>,
        ocr_text: &str,
    ) -> serde_json::Value {
        let policy = policy_for_action(action);

        json!({
            "model": model,
            "stream": false,
            "keep_alive": self.config.keep_alive,
            "options": ollama_options(
                &self.config,
                num_predict_for_action(action, "text")
            ),
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt(policy)
                },
                {
                    "role": "user",
                    "content": user_prompt_from_text(action, source_app, ocr_text)
                }
            ]
        })
    }

    fn search_answer_chat_payload(
        &self,
        model: &str,
        query: &str,
        source_label: &str,
        ai_overview_text: &str,
        supporting_sources: &str,
    ) -> serde_json::Value {
        json!({
            "model": model,
            "stream": false,
            "keep_alive": self.config.keep_alive,
            "options": ollama_options(&self.config, 420),
            "messages": [
                {
                    "role": "system",
                    "content": search_answer_system_prompt()
                },
                {
                    "role": "user",
                    "content": search_answer_user_prompt(
                        query,
                        source_label,
                        ai_overview_text,
                        supporting_sources
                    )
                }
            ]
        })
    }

    fn repl_agent_chat_payload(&self, model: &str, user_message: &str) -> serde_json::Value {
        json!({
            "model": model,
            "stream": false,
            "keep_alive": self.config.keep_alive,
            "options": ollama_options(&self.config, 640),
            "messages": [
                {
                    "role": "system",
                    "content": repl_agent_system_prompt()
                },
                {
                    "role": "user",
                    "content": repl_agent_user_prompt(user_message)
                }
            ]
        })
    }

    async fn infer_image_with_model(
        &self,
        input: &InferenceInput,
        model: &str,
    ) -> Result<InferenceOutput> {
        let payload = self.image_chat_payload(model, input);
        self.send_chat_request(&input.request_id, &input.action, model, "image", payload)
            .await
    }

    async fn send_chat_request(
        &self,
        request_id: &str,
        action: &visionclip_common::ipc::Action,
        model: &str,
        input_mode: &str,
        payload_template: serde_json::Value,
    ) -> Result<InferenceOutput> {
        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));
        let mut think_value = default_think_value(&self.config.thinking_default);
        let request_started_at = Instant::now();
        let mut attempts = 0_u32;

        let response = loop {
            attempts += 1;
            let mut payload = payload_template.clone();
            if let Some(value) = think_value.clone() {
                payload["think"] = value;
            }

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

            let thinking_unsupported = think_value.is_some()
                && status == StatusCode::BAD_REQUEST
                && body.to_ascii_lowercase().contains("think");

            if thinking_unsupported {
                think_value = None;
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
        let request_ms = elapsed_ms(request_started_at);

        let thinking_chars = decoded
            .message
            .as_ref()
            .map(|message| message.thinking.chars().count())
            .unwrap_or_default();

        let content = decoded
            .message
            .map(|message| message.content)
            .or(decoded.response)
            .ok_or_else(|| anyhow!("Ollama response did not contain text content"))?;

        info!(
            request_id = %request_id,
            action = action.as_str(),
            model,
            input_mode,
            attempts,
            request_ms,
            content_chars = content.chars().count(),
            thinking_chars,
            ollama_total_ms = duration_ms(decoded.total_duration),
            ollama_load_ms = duration_ms(decoded.load_duration),
            prompt_eval_count = decoded.prompt_eval_count.unwrap_or_default(),
            prompt_eval_ms = duration_ms(decoded.prompt_eval_duration),
            eval_count = decoded.eval_count.unwrap_or_default(),
            eval_ms = duration_ms(decoded.eval_duration),
            "ollama inference completed"
        );

        Ok(InferenceOutput { text: content })
    }

    async fn send_embed_request(&self, input: EmbeddingInput) -> Result<EmbeddingOutput> {
        let EmbeddingInput { request_id, texts } = input;
        let model = self.config.embedding_model.trim();
        if model.is_empty() {
            return Err(anyhow!("Ollama embedding model is not configured"));
        }

        if texts.is_empty() {
            return Err(anyhow!("embedding input must contain at least one text"));
        }

        if texts.iter().any(|text| text.trim().is_empty()) {
            return Err(anyhow!("embedding input cannot contain blank texts"));
        }

        let url = format!("{}/api/embed", self.config.base_url.trim_end_matches('/'));
        let expected_vectors = texts.len();
        let payload = json!({
            "model": model,
            "input": texts,
            "keep_alive": self.config.keep_alive,
        });
        let request_started_at = Instant::now();

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("failed to call Ollama embeddings")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read Ollama embeddings error body".to_string());
            let body = body.trim();
            if body.is_empty() {
                return Err(anyhow!("Ollama embeddings returned {}", status));
            }

            return Err(anyhow!("Ollama embeddings returned {}: {}", status, body));
        }

        let decoded: OllamaEmbedResponse = response
            .json()
            .await
            .context("failed to parse Ollama embeddings response")?;

        if decoded.embeddings.len() != expected_vectors {
            return Err(anyhow!(
                "Ollama embeddings returned {} vectors for {} inputs",
                decoded.embeddings.len(),
                expected_vectors
            ));
        }

        if decoded.embeddings.iter().any(Vec::is_empty) {
            return Err(anyhow!("Ollama embeddings returned an empty vector"));
        }

        let request_ms = elapsed_ms(request_started_at);
        let vector_dims = decoded.embeddings.first().map(Vec::len).unwrap_or_default();

        info!(
            request_id = %request_id,
            model,
            input_count = expected_vectors,
            vector_dims,
            request_ms,
            ollama_total_ms = duration_ms(decoded.total_duration),
            ollama_load_ms = duration_ms(decoded.load_duration),
            prompt_eval_count = decoded.prompt_eval_count.unwrap_or_default(),
            "ollama embeddings completed"
        );

        Ok(EmbeddingOutput {
            vectors: decoded.embeddings,
        })
    }
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: Option<OllamaMessage>,
    response: Option<String>,
    #[serde(default)]
    total_duration: Option<u64>,
    #[serde(default)]
    load_duration: Option<u64>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    prompt_eval_duration: Option<u64>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    eval_duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
    #[serde(default)]
    thinking: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    total_duration: Option<u64>,
    #[serde(default)]
    load_duration: Option<u64>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
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
        self.infer_image_with_model(&input, &self.config.model)
            .await
    }
}

#[async_trait]
impl EmbeddingBackend for OllamaBackend {
    async fn embed(&self, input: EmbeddingInput) -> Result<EmbeddingOutput> {
        self.send_embed_request(input).await
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
}

fn duration_ms(duration_ns: Option<u64>) -> u64 {
    duration_ns
        .map(|value| value / 1_000_000)
        .unwrap_or_default()
}

fn default_think_value(thinking_default: &str) -> Option<serde_json::Value> {
    let trimmed = thinking_default.trim();
    if trimmed.is_empty() {
        Some(json!(false))
    } else {
        Some(json!(trimmed))
    }
}

fn num_predict_for_action(action: &visionclip_common::ipc::Action, input_mode: &str) -> u32 {
    match (action, input_mode) {
        (visionclip_common::ipc::Action::SearchWeb, _) => 32,
        (visionclip_common::ipc::Action::Explain, "text") => 360,
        (visionclip_common::ipc::Action::Explain, _) => 480,
        (visionclip_common::ipc::Action::TranslatePtBr, "text") => 300,
        (visionclip_common::ipc::Action::TranslatePtBr, _) => 360,
        (visionclip_common::ipc::Action::CopyText, _) => 1024,
        (visionclip_common::ipc::Action::ExtractCode, _) => 1024,
    }
}

fn document_translation_num_predict(source_text: &str) -> u32 {
    ((source_text.chars().count() as u32) / 2).clamp(512, 3072)
}

fn document_translation_system_prompt() -> &'static str {
    "Voce traduz trechos de documentos para o idioma solicitado. Responda somente com a traducao final. Preserve significado, ordem dos paragrafos, listas simples e tom do texto. Preserve literalmente comandos, codigo, caminhos, URLs, nomes de arquivo, APIs, flags, identificadores e numeros. Nao explique, nao resuma, nao mencione OCR, prompt, tarefa ou instrucoes."
}

fn document_translation_user_prompt(target_language: &str, source_text: &str) -> String {
    format!(
        "Idioma alvo: {}\n\nTexto fonte:\n<<<DOCUMENT\n{}\nDOCUMENT>>>\n\nTraduza o texto fonte para o idioma alvo. Retorne somente a traducao final.",
        target_language.trim(),
        source_text.trim()
    )
}

fn ollama_options(config: &InferConfig, num_predict: u32) -> serde_json::Value {
    let mut options = json!({
        "temperature": config.temperature,
        "num_predict": num_predict
    });

    if config.context_window_tokens > 0 {
        options["num_ctx"] = json!(config.context_window_tokens);
    }

    options
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
                request_id: "req-1".into(),
                action: Action::Explain,
                source_app: None,
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
        let prompt = json["messages"][1]["content"]
            .as_str()
            .expect("user prompt string");
        assert!(prompt.contains("Explique tecnicamente o que aparece nesta captura."));
        assert!(prompt.contains("Se for terminal ou log"));
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
                request_id: "req-2".into(),
                action: Action::Explain,
                source_app: None,
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
    async fn infer_disables_thinking_by_default() {
        let server = TestServer::spawn(
            r#"{"message":{"content":"resposta final","thinking":"rascunho interno"}}"#,
        );
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            model: "gemma4:test".into(),
            thinking_default: String::new(),
            ..InferConfig::default()
        });

        let output = backend
            .infer(InferenceInput {
                request_id: "req-3".into(),
                action: Action::TranslatePtBr,
                source_app: None,
                image_bytes: b"PNG".to_vec(),
                mime_type: "image/png".into(),
            })
            .await
            .unwrap();

        let requests = server.finish();
        let request_json: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();

        assert_eq!(request_json["think"], json!(false));
        assert_eq!(output.text, "resposta final");
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

    #[tokio::test]
    async fn embed_posts_batch_payload_to_ollama() {
        let server = TestServer::spawn(
            r#"{"embeddings":[[0.1,0.2,0.3],[0.4,0.5,0.6]],"total_duration":12000000,"load_duration":3000000,"prompt_eval_count":2}"#,
        );
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            embedding_model: "embeddinggemma".into(),
            keep_alive: "5m".into(),
            ..InferConfig::default()
        });

        let output = backend
            .embed_texts(
                "req-embed".into(),
                vec!["primeiro trecho".into(), "segundo trecho".into()],
            )
            .await
            .unwrap();

        let requests = server.finish();
        let (headers, body) = &requests[0];
        assert!(headers.starts_with("POST /api/embed HTTP/1.1"));
        assert_eq!(
            output,
            EmbeddingOutput {
                vectors: vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]]
            }
        );

        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["model"], "embeddinggemma");
        assert_eq!(json["input"][0], "primeiro trecho");
        assert_eq!(json["input"][1], "segundo trecho");
        assert_eq!(json["keep_alive"], "5m");
    }

    #[tokio::test]
    async fn document_translation_uses_target_language_prompt() {
        let server = TestServer::spawn(r#"{"message":{"content":"Hola mundo"}}"#);
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            model: "gemma4:test".into(),
            keep_alive: "5m".into(),
            temperature: 0.1,
            ..InferConfig::default()
        });

        let output = backend
            .translate_document_text(
                "req-doc-translate".into(),
                "Spanish",
                "Hello world.\nPreserve cargo test.",
            )
            .await
            .unwrap();

        let requests = server.finish();
        let (headers, body) = &requests[0];
        assert!(headers.starts_with("POST /api/chat HTTP/1.1"));
        assert_eq!(output.text, "Hola mundo");

        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        let user_prompt = json["messages"][1]["content"]
            .as_str()
            .expect("document translation user prompt");
        assert_eq!(json["model"], "gemma4:test");
        assert_eq!(json["options"]["num_predict"], 512);
        assert!(user_prompt.contains("Idioma alvo: Spanish"));
        assert!(user_prompt.contains("Preserve cargo test."));
    }

    #[tokio::test]
    async fn embed_requires_configured_model() {
        let backend = OllamaBackend::new(InferConfig {
            embedding_model: String::new(),
            ..InferConfig::default()
        });

        let error = backend
            .embed_texts("req-embed".into(), vec!["texto".into()])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("not configured"));
    }

    #[tokio::test]
    async fn embed_rejects_mismatched_vector_count() {
        let server = TestServer::spawn(r#"{"embeddings":[[0.1,0.2,0.3]]}"#);
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            embedding_model: "embeddinggemma".into(),
            ..InferConfig::default()
        });

        let error = backend
            .embed_texts("req-embed".into(), vec!["um".into(), "dois".into()])
            .await
            .unwrap_err();

        let requests = server.finish();
        assert!(requests[0].0.starts_with("POST /api/embed HTTP/1.1"));
        assert!(error.to_string().contains("1 vectors for 2 inputs"));
    }

    #[test]
    fn payload_sets_action_specific_num_predict() {
        let backend = OllamaBackend::new(InferConfig::default());
        let image_payload = backend.image_chat_payload(
            "gemma4:e2b",
            &InferenceInput {
                request_id: "req-3".into(),
                action: Action::SearchWeb,
                source_app: None,
                image_bytes: b"PNG".to_vec(),
                mime_type: "image/png".into(),
            },
        );
        let text_payload =
            backend.text_chat_payload("gemma4:e2b", &Action::Explain, None, "erro ao abrir");

        assert_eq!(image_payload["options"]["num_predict"], json!(32));
        assert_eq!(text_payload["options"]["num_predict"], json!(360));
        assert_eq!(text_payload["options"]["num_ctx"], json!(8192));
    }

    #[tokio::test]
    async fn search_answer_uses_google_ai_overview_context_payload() {
        let server = TestServer::spawn(
            r#"{"message":{"content":"JavaScript é uma linguagem usada para criar interatividade na web."}}"#,
        );
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            model: "gemma4:test".into(),
            keep_alive: "5m".into(),
            temperature: 0.1,
            thinking_default: String::new(),
            ..InferConfig::default()
        });

        let output = backend
            .answer_search_from_context(
                "req-search-answer".into(),
                "O que é JavaScript?",
                "Visão geral criada por IA renderizada no Google",
                "JavaScript é uma linguagem de programação de alto nível para web.",
                "MDN: JavaScript permite páginas interativas.",
            )
            .await
            .unwrap();

        let requests = server.finish();
        let json: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
        assert_eq!(
            output.text,
            "JavaScript é uma linguagem usada para criar interatividade na web."
        );
        assert_eq!(json["model"], "gemma4:test");
        assert_eq!(json["options"]["num_predict"], json!(420));
        assert_eq!(json["options"]["num_ctx"], json!(8192));
        assert_eq!(json["messages"][0]["role"], "system");
        assert!(json["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("somente o contexto de busca fornecido"));
        assert!(json["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("GOOGLE_AI_OVERVIEW"));
    }

    #[tokio::test]
    async fn repl_agent_answer_uses_cli_agent_prompt_payload() {
        let server =
            TestServer::spawn(r#"{"message":{"content":"Olá. Como posso ajudar no código?"}}"#);
        let backend = OllamaBackend::new(InferConfig {
            base_url: server.base_url.clone(),
            model: "gemma4:test".into(),
            keep_alive: "5m".into(),
            temperature: 0.1,
            thinking_default: String::new(),
            ..InferConfig::default()
        });

        let output = backend
            .answer_repl_turn("req-repl-agent".into(), "olá")
            .await
            .unwrap();

        let requests = server.finish();
        let json: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
        assert_eq!(output.text, "Olá. Como posso ajudar no código?");
        assert_eq!(json["model"], "gemma4:test");
        assert_eq!(json["options"]["num_predict"], json!(640));
        assert!(json["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Nao trate toda mensagem como pesquisa web"));
        assert!(json["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Mensagem do usuario no REPL"));
    }

    #[test]
    fn default_think_value_uses_false_when_not_configured() {
        assert_eq!(default_think_value(""), Some(json!(false)));
        assert_eq!(default_think_value("low"), Some(json!("low")));
    }
}
