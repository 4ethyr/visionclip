use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info, warn};
use visionclip_common::{read_message, write_message, Action, AppConfig, CaptureJob, JobResult};
use visionclip_infer::{
    postprocess::sanitize_output, InferenceBackend, InferenceInput, OllamaBackend,
};
use visionclip_output::{notify, open_search_query, ClipboardOwner};
use visionclip_tts::PiperHttpClient;

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::load()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| config.general.log_level.clone()),
        )
        .init();

    let socket_path = config.socket_path()?;
    cleanup_existing_socket(&socket_path)?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind socket at {}", socket_path.display()))?;

    info!(socket = %socket_path.display(), "visionclip-daemon listening");

    let state = Arc::new(AppState {
        config: config.clone(),
        clipboard: ClipboardOwner::new().context("failed to initialize clipboard owner")?,
        infer: OllamaBackend::new(config.infer.clone()),
        piper: if config.audio.enabled {
            Some(PiperHttpClient::new(config.audio.clone()))
        } else {
            None
        },
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, state).await {
                error!(?error, "failed to process launcher request");
            }
        });
    }
}

struct AppState {
    config: AppConfig,
    clipboard: ClipboardOwner,
    infer: OllamaBackend,
    piper: Option<PiperHttpClient>,
}

async fn handle_connection(mut stream: UnixStream, state: Arc<AppState>) -> Result<()> {
    let job: CaptureJob = read_message(&mut stream).await?;

    let response = match process_job(&state, job).await {
        Ok(result) => result,
        Err(error) => {
            error!(?error, "job processing failed");
            JobResult::Error {
                request_id: uuid::Uuid::new_v4(),
                code: "processing_error".into(),
                message: error.to_string(),
            }
        }
    };

    write_message(&mut stream, &response).await?;
    Ok(())
}

async fn process_job(state: &AppState, job: CaptureJob) -> Result<JobResult> {
    let inference = state
        .infer
        .infer(InferenceInput {
            action: job.action.clone(),
            image_bytes: job.image_bytes.clone(),
            mime_type: job.mime_type.clone(),
        })
        .await?;

    let cleaned = sanitize_output(&job.action, &inference.text);
    let spoken = state
        .config
        .action_should_speak(job.action.as_str(), job.speak);

    match job.action {
        Action::SearchWeb => {
            open_search_query(&cleaned)?;
            let _ = notify("VisionClip", "Consulta aberta no navegador.");

            if spoken {
                if let Some(piper) = &state.piper {
                    let wav = piper.synthesize(&cleaned, None).await?;
                    if let Err(error) = piper.play_wav(&wav) {
                        warn!(?error, "failed to play synthesized audio");
                    }
                }
            }

            Ok(JobResult::BrowserQuery {
                request_id: job.request_id,
                query: cleaned,
            })
        }
        _ => {
            state.clipboard.set_text(&cleaned)?;
            let _ = notify(
                "VisionClip",
                "Resultado copiado para a área de transferência.",
            );

            if spoken {
                if let Some(piper) = &state.piper {
                    let wav = piper.synthesize(&cleaned, None).await?;
                    if let Err(error) = piper.play_wav(&wav) {
                        warn!(?error, "failed to play synthesized audio");
                    }
                }
            }

            Ok(JobResult::ClipboardText {
                request_id: job.request_id,
                text: cleaned,
                spoken,
            })
        }
    }
}

fn cleanup_existing_socket(socket_path: &PathBuf) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .with_context(|| format!("failed to remove stale socket {}", socket_path.display()))?;
    }
    Ok(())
}
