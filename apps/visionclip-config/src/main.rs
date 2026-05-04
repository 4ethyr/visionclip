use anyhow::Result;
use clap::{Parser, Subcommand};
use reqwest::{Client, StatusCode};
use serde_json::json;
use std::env;
use tokio::time::{timeout, Duration};
use visionclip_common::{
    current_desktops, screenshot_portal_backends_for_current_desktop, summarize_portal_backends,
    AppConfig,
};
use visionclip_infer::{list_ollama_models, OllamaModelSummary};

const OLLAMA_PROBE_TIMEOUT_MS: u64 = 180_000;
const GTK_OVERLAY_ENABLED: bool = cfg!(feature = "gtk-overlay");

#[derive(Debug, Parser)]
#[command(name = "visionclip-config")]
#[command(about = "Ferramenta de configuração e diagnóstico do VisionClip")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Doctor,
    Models,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let path = AppConfig::ensure_default_config()?;
            println!("Configuração criada em {}", path.display());
        }
        Commands::Doctor => run_doctor().await?,
        Commands::Models => run_models().await?,
    }

    Ok(())
}

async fn run_doctor() -> Result<()> {
    let config = AppConfig::load()?;

    println!("Config path: {}", AppConfig::config_path()?.display());
    println!("Socket: {}", config.socket_path()?.display());
    println!("Session: {}", detect_session_type());
    println!("Desktop: {}", detect_desktop_environment());
    println!("Default action: {}", config.general.default_action);
    println!("Capture backend: {}", config.capture.backend);
    println!("Capture timeout: {} ms", config.capture.capture_timeout_ms);
    println!(
        "Portal screenshot backends: {}",
        summarize_portal_backends(&screenshot_portal_backends_for_current_desktop())
    );
    println!("Voice input enabled: {}", yes_no(config.voice.enabled));
    println!("Voice backend: {}", config.voice.backend);
    println!(
        "Legacy voice overlay enabled: {}",
        yes_no(config.voice.overlay_enabled)
    );
    println!(
        "Legacy voice overlay runtime: {}",
        if GTK_OVERLAY_ENABLED {
            "gtk-overlay compiled"
        } else {
            "gtk-overlay not compiled"
        }
    );
    println!("Voice shortcut: {}", config.voice.shortcut);
    println!(
        "Voice record duration: {} ms",
        config.voice.record_duration_ms
    );
    println!(
        "Voice transcribe command: {}",
        if config.voice.transcribe_command.trim().is_empty() {
            "not configured"
        } else {
            "configured"
        }
    );
    println!("Configured model: {}", config.infer.model);
    println!(
        "Provider route mode: {}",
        config.providers.route_mode_normalized()
    );
    println!(
        "Sensitive data provider mode: {}",
        config.providers.sensitive_data_mode_normalized()
    );
    println!(
        "Ollama provider enabled: {}",
        yes_no(config.providers.ollama_enabled)
    );
    println!(
        "Cloud providers enabled: {}",
        yes_no(config.providers.cloud_enabled)
    );
    if config.infer.ocr_model.trim().is_empty() {
        println!("Configured OCR model: disabled");
    } else {
        println!("Configured OCR model: {}", config.infer.ocr_model);
    }
    if config.infer.embedding_model.trim().is_empty() {
        println!("Configured embedding model: disabled");
    } else {
        println!(
            "Configured embedding model: {}",
            config.infer.embedding_model
        );
    }
    println!("Ollama URL: {}", config.infer.base_url);

    println!("Piper URL: {}", config.audio.base_url);
    let piper_voices_url = piper_voices_url(&config.audio.base_url);
    match probe_http(&piper_voices_url).await {
        Ok(status) => {
            println!("Piper endpoint: reachable ({status})");
            report_piper_voices(&config.audio).await;
        }
        Err(error) => println!("Piper endpoint: unavailable ({error})"),
    }

    match list_ollama_models(&config.infer.base_url).await {
        Ok(models) => {
            let available = model_available(&models, &config.infer.model);
            let ocr_available = !config.infer.ocr_model.trim().is_empty()
                && model_available(&models, &config.infer.ocr_model);
            let embedding_available = !config.infer.embedding_model.trim().is_empty()
                && model_available(&models, &config.infer.embedding_model);
            println!("Ollama API: ok");
            println!("Ollama models: {}", models.len());
            println!("Configured model available: {}", yes_no(available));
            if !config.infer.ocr_model.trim().is_empty() {
                println!("Configured OCR model available: {}", yes_no(ocr_available));
            }
            if !config.infer.embedding_model.trim().is_empty() {
                println!(
                    "Configured embedding model available: {}",
                    yes_no(embedding_available)
                );
            }

            if available {
                match probe_ollama_model_with_timeout(
                    &config.infer.base_url,
                    &config.infer.model,
                    &config.infer.thinking_default,
                    OLLAMA_PROBE_TIMEOUT_MS,
                )
                .await
                {
                    Ok(()) => println!("Configured model probe: ok"),
                    Err(error) => println!("Configured model probe: failed ({error})"),
                }
            }
            if ocr_available {
                match probe_ollama_model_with_timeout(
                    &config.infer.base_url,
                    &config.infer.ocr_model,
                    "",
                    OLLAMA_PROBE_TIMEOUT_MS,
                )
                .await
                {
                    Ok(()) => println!("Configured OCR model probe: ok"),
                    Err(error) => println!("Configured OCR model probe: failed ({error})"),
                }
            }
        }
        Err(error) => {
            println!("Ollama API: unavailable ({error})");
        }
    }

    println!("Desktop integration:");
    for tool in [
        "ollama",
        "pdftotext",
        "notify-send",
        "xdg-open",
        "gsettings",
        "gdbus",
        "busctl",
        "wl-copy",
        "wl-paste",
        "xclip",
        "paplay",
        "pw-play",
        "pw-record",
        "aplay",
        "arecord",
        "gnome-screenshot",
        "grim",
        "slurp",
        "maim",
        "spectacle",
        "flameshot",
        "scrot",
        "import",
    ] {
        println!("  {tool}: {}", tool_status(tool));
    }

    Ok(())
}

async fn run_models() -> Result<()> {
    let config = AppConfig::load()?;
    let models = list_ollama_models(&config.infer.base_url).await?;

    if models.is_empty() {
        println!("Nenhum modelo encontrado em {}", config.infer.base_url);
        return Ok(());
    }

    println!("Modelos disponíveis em {}:", config.infer.base_url);
    for model in models {
        println!(
            "- {} | {} | family={} | format={} | quant={} | modified={}",
            model.name,
            human_size(model.size),
            value_or_dash(&model.details.family),
            value_or_dash(&model.details.format),
            value_or_dash(&model.details.quantization_level),
            value_or_dash(&model.modified_at),
        );
    }

    Ok(())
}

async fn probe_http(base_url: &str) -> Result<reqwest::StatusCode> {
    let response = Client::new().get(base_url).send().await?;
    Ok(response.status())
}

async fn report_piper_voices(audio: &visionclip_common::config::AudioConfig) {
    let configured = audio.configured_voice_ids();
    if configured.is_empty() {
        println!("Configured Piper voices: none (server default will be used)");
        return;
    }

    println!("Configured Piper voices: {}", configured.join(", "));
    match list_piper_voices(&audio.base_url).await {
        Ok(available) => {
            let missing = audio.missing_configured_voice_ids(available.iter().map(String::as_str));
            if missing.is_empty() {
                println!("Configured Piper voices available: yes");
            } else {
                println!(
                    "Configured Piper voices available: no (missing: {})",
                    missing.join(", ")
                );
            }
        }
        Err(error) => {
            println!("Configured Piper voices available: unknown ({error})");
        }
    }
}

async fn list_piper_voices(base_url: &str) -> Result<Vec<String>> {
    let url = piper_voices_url(base_url);
    let response = Client::new().get(url).send().await?.error_for_status()?;
    let value: serde_json::Value = response.json().await?;
    let Some(object) = value.as_object() else {
        anyhow::bail!("Piper /voices did not return a JSON object");
    };
    let mut voices = object.keys().cloned().collect::<Vec<_>>();
    voices.sort();
    Ok(voices)
}

fn piper_voices_url(base_url: &str) -> String {
    format!("{}/voices", base_url.trim_end_matches('/'))
}

async fn probe_ollama_model_with_timeout(
    base_url: &str,
    model: &str,
    thinking_default: &str,
    timeout_ms: u64,
) -> Result<()> {
    timeout(
        Duration::from_millis(timeout_ms),
        probe_ollama_model_request(base_url, model, thinking_default),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out after {timeout_ms} ms"))?
}

async fn probe_ollama_model_request(
    base_url: &str,
    model: &str,
    thinking_default: &str,
) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
    let mut include_thinking = !thinking_default.trim().is_empty();

    loop {
        let mut payload = json!({
            "model": model,
            "stream": false,
            "keep_alive": "0s",
            "options": {
                "temperature": 0.0,
                "num_predict": 1
            },
            "messages": [
                {
                    "role": "user",
                    "content": "Reply with OK."
                }
            ]
        });

        if include_thinking {
            payload["think"] = json!(thinking_default);
        }

        let response = client.post(&url).json(&payload).send().await?;
        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let unsupported_thinking = include_thinking
            && status == StatusCode::BAD_REQUEST
            && body.contains("does not support thinking");

        if unsupported_thinking {
            include_thinking = false;
            continue;
        }

        let body = body.trim();
        if body.is_empty() {
            anyhow::bail!("Ollama returned {}", status);
        }

        anyhow::bail!("Ollama returned {}: {}", status, body);
    }
}

fn model_available(models: &[OllamaModelSummary], configured_model: &str) -> bool {
    models.iter().any(|model| {
        model.name.eq_ignore_ascii_case(configured_model)
            || model.model.eq_ignore_ascii_case(configured_model)
    })
}

fn detect_session_type() -> &'static str {
    match env::var("XDG_SESSION_TYPE") {
        Ok(value) if value.eq_ignore_ascii_case("wayland") => "wayland",
        Ok(value) if value.eq_ignore_ascii_case("x11") => "x11",
        Ok(_) => "other",
        Err(_) => "unknown",
    }
}

fn detect_desktop_environment() -> String {
    let desktops = current_desktops();
    if desktops.is_empty() {
        "unknown".into()
    } else {
        desktops.join(":")
    }
}

fn tool_status(name: &str) -> String {
    which::which(name)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "não encontrado".into())
}

fn human_size(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn value_or_dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
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
    };

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

                    requests.push((headers, String::from_utf8(body).unwrap()));

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
            let requests = self.request_rx.recv().unwrap();
            self.handle.join().unwrap();
            requests
        }

        fn spawn_hanging() -> Self {
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

                request_tx
                    .send(vec![(headers, String::from_utf8(body).unwrap())])
                    .unwrap();
                thread::sleep(std::time::Duration::from_millis(200));
            });

            Self {
                base_url: format!("http://{}", address),
                request_rx,
                handle,
            }
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
    async fn probe_ollama_model_retries_without_thinking() {
        let server = TestServer::spawn_sequence(vec![
            TestResponse {
                status_line: "400 Bad Request",
                body: r#"{"error":"\"gemma4:test\" does not support thinking"}"#,
            },
            TestResponse {
                status_line: "200 OK",
                body: r#"{"message":{"content":"OK"}}"#,
            },
        ]);

        probe_ollama_model_with_timeout(&server.base_url, "gemma4:test", "low", 1_000)
            .await
            .unwrap();

        let requests = server.finish();
        assert_eq!(requests.len(), 2);
        let first_json: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
        let second_json: serde_json::Value = serde_json::from_str(&requests[1].1).unwrap();
        assert_eq!(first_json["think"], "low");
        assert!(second_json.get("think").is_none());
    }

    #[tokio::test]
    async fn probe_ollama_model_surfaces_runtime_error() {
        let server = TestServer::spawn_sequence(vec![TestResponse {
            status_line: "500 Internal Server Error",
            body: r#"{"error":"unable to load model"}"#,
        }]);

        let error = probe_ollama_model_with_timeout(&server.base_url, "gemma4:test", "", 1_000)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("500 Internal Server Error"));
        assert!(error.to_string().contains("unable to load model"));
    }

    #[tokio::test]
    async fn probe_ollama_model_times_out() {
        let server = TestServer::spawn_hanging();

        let error = probe_ollama_model_with_timeout(&server.base_url, "gemma4:test", "", 20)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("timed out"));
        let _ = server.finish();
    }
}
