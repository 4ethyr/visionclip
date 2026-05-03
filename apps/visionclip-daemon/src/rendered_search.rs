use crate::search::{parse_rendered_google_search_text, SearchEnrichment};
use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{
    process::Command,
    time::{sleep, timeout, Duration, Instant},
};
use tracing::{debug, warn};
use uuid::Uuid;
use visionclip_common::{
    config::SearchConfig, discover_rendered_capture_backends,
    likely_gnome_shell_screenshot_available, Action, CaptureBackendKind, SessionType,
};
use visionclip_infer::{
    postprocess::sanitize_output, AiTask, ProviderMode, ProviderRouteRequest, ProviderRouter,
    VisionRequest as ProviderVisionRequest,
};
use which::which;

const RENDERED_CAPTURE_TIMEOUT_MS: u64 = 5_000;
const MIN_RENDERED_POLL_INTERVAL_MS: u64 = 750;

#[derive(Clone)]
pub struct RenderedSearchJob {
    pub request_id: Uuid,
    pub query: String,
    pub search: SearchConfig,
    pub provider_router: Arc<ProviderRouter>,
    pub sensitive_provider_mode: ProviderMode,
    pub response_language: String,
    pub tts_voice_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderedOverviewResult {
    pub enrichment: SearchEnrichment,
    pub attempts: usize,
    pub ocr_chars: usize,
    pub capture_backend: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedCaptureBackend {
    GnomeShellScreenshot,
    GnomeScreenshot,
    Grim,
    Maim,
}

impl RenderedCaptureBackend {
    fn from_kind(kind: CaptureBackendKind) -> Option<Self> {
        match kind {
            CaptureBackendKind::GnomeShellScreenshot => Some(Self::GnomeShellScreenshot),
            CaptureBackendKind::GnomeScreenshot => Some(Self::GnomeScreenshot),
            CaptureBackendKind::Grim => Some(Self::Grim),
            CaptureBackendKind::Maim => Some(Self::Maim),
            _ => None,
        }
    }

    fn program(self) -> &'static str {
        match self {
            Self::GnomeShellScreenshot => "gnome-shell-screenshot",
            Self::GnomeScreenshot => "gnome-screenshot",
            Self::Grim => "grim",
            Self::Maim => "maim",
        }
    }

    fn args(self, path: &Path) -> Vec<String> {
        match self {
            Self::GnomeShellScreenshot => Vec::new(),
            Self::GnomeScreenshot => vec!["-f".into(), path.display().to_string()],
            Self::Grim | Self::Maim => vec![path.display().to_string()],
        }
    }

    fn command_path(self) -> PathBuf {
        let program = match self {
            Self::GnomeShellScreenshot => "gdbus",
            _ => self.program(),
        };
        command_path(program).unwrap_or_else(|| PathBuf::from(program))
    }
}

pub async fn wait_for_rendered_ai_overview(
    job: &RenderedSearchJob,
) -> Result<Option<RenderedOverviewResult>> {
    if !job.search.rendered_ai_overview_listener || job.search.rendered_ai_overview_wait_ms == 0 {
        return Ok(None);
    }

    let backends = resolve_capture_backends().await;
    if backends.is_empty() {
        debug!(
            request_id = %job.request_id,
            "rendered AI overview listener skipped because no screenshot backend is available"
        );
        return Ok(None);
    };

    let wait_for = Duration::from_millis(job.search.rendered_ai_overview_wait_ms);
    let poll_interval = Duration::from_millis(
        job.search
            .rendered_ai_overview_poll_interval_ms
            .max(MIN_RENDERED_POLL_INTERVAL_MS),
    );
    let deadline = Instant::now() + wait_for;
    let mut attempts = 0_usize;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        sleep(poll_interval.min(remaining)).await;
        attempts += 1;

        let capture = match capture_visible_screen(job.request_id, attempts, &backends).await {
            Ok(capture) => capture,
            Err(error) => {
                warn!(
                    ?error,
                    request_id = %job.request_id,
                    attempt = attempts,
                    "rendered AI overview screenshot capture failed"
                );
                continue;
            }
        };

        let provider = match job
            .provider_router
            .route(ProviderRouteRequest {
                task: AiTask::Ocr,
                mode: job.sensitive_provider_mode,
                sensitive: true,
            })
            .await
        {
            Ok(provider) => provider,
            Err(error) => {
                warn!(
                    ?error,
                    request_id = %job.request_id,
                    attempt = attempts,
                    "rendered AI overview OCR provider routing failed"
                );
                continue;
            }
        };

        let inference = match provider
            .provider
            .ocr(ProviderVisionRequest {
                request_id: format!("{}-rendered-ai-overview-{attempts}", job.request_id),
                action: Action::CopyText,
                source_app: Some("rendered_search".to_string()),
                response_language: None,
                image_bytes: capture.bytes,
                mime_type: "image/png".to_string(),
            })
            .await
        {
            Ok(output) => output,
            Err(error) => {
                warn!(
                    ?error,
                    request_id = %job.request_id,
                    attempt = attempts,
                    "rendered AI overview OCR failed"
                );
                continue;
            }
        };

        let visible_text = sanitize_output(&Action::CopyText, &inference.text);
        let ocr_chars = visible_text.chars().count();
        let enrichment = parse_rendered_google_search_text(&visible_text);
        if enrichment.ai_overview.is_some() {
            return Ok(Some(RenderedOverviewResult {
                enrichment,
                attempts,
                ocr_chars,
                capture_backend: capture.backend.program(),
            }));
        }

        debug!(
            request_id = %job.request_id,
            attempt = attempts,
            ocr_chars,
            "rendered AI overview not visible yet"
        );
    }

    Ok(None)
}

async fn capture_visible_screen(
    request_id: Uuid,
    attempt: usize,
    backends: &[RenderedCaptureBackend],
) -> Result<RenderedCapture> {
    let mut errors = Vec::new();

    for backend in backends {
        match capture_visible_screen_with_backend(request_id, attempt, *backend).await {
            Ok(bytes) => {
                return Ok(RenderedCapture {
                    bytes,
                    backend: *backend,
                });
            }
            Err(error) => errors.push(format!("{}: {error}", backend.program())),
        }
    }

    anyhow::bail!(
        "all rendered screenshot backends failed: {}",
        errors.join(" | ")
    )
}

struct RenderedCapture {
    bytes: Vec<u8>,
    backend: RenderedCaptureBackend,
}

async fn capture_visible_screen_with_backend(
    request_id: Uuid,
    attempt: usize,
    backend: RenderedCaptureBackend,
) -> Result<Vec<u8>> {
    let path = rendered_capture_path(request_id, attempt)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    if backend == RenderedCaptureBackend::GnomeShellScreenshot {
        return capture_visible_screen_with_gnome_shell(&path).await;
    }

    let args = backend.args(&path);
    let command_path = backend.command_path();
    let output = timeout(
        Duration::from_millis(RENDERED_CAPTURE_TIMEOUT_MS),
        Command::new(&command_path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .with_context(|| {
        format!(
            "rendered screenshot command `{}` timed out",
            backend.program()
        )
    })?
    .with_context(|| {
        format!(
            "failed to run rendered screenshot command `{}`",
            backend.program()
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&path);
        anyhow::bail!(
            "rendered screenshot command `{}` failed: {}",
            backend.program(),
            stderr.trim()
        );
    }

    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let _ = fs::remove_file(&path);
    if bytes.is_empty() {
        anyhow::bail!("rendered screenshot command produced an empty image");
    }

    Ok(bytes)
}

async fn capture_visible_screen_with_gnome_shell(path: &Path) -> Result<Vec<u8>> {
    let args = vec![
        "call".to_string(),
        "--session".to_string(),
        "--dest".to_string(),
        "org.gnome.Shell.Screenshot".to_string(),
        "--object-path".to_string(),
        "/org/gnome/Shell/Screenshot".to_string(),
        "--method".to_string(),
        "org.gnome.Shell.Screenshot.Screenshot".to_string(),
        "false".to_string(),
        "false".to_string(),
        path.display().to_string(),
    ];

    let command_path = command_path("gdbus").unwrap_or_else(|| PathBuf::from("gdbus"));
    let output = timeout(
        Duration::from_millis(RENDERED_CAPTURE_TIMEOUT_MS),
        Command::new(&command_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("GNOME Shell screenshot D-Bus call timed out")?
    .context("failed to run GNOME Shell screenshot D-Bus call")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(path);
        anyhow::bail!(
            "GNOME Shell screenshot D-Bus call failed: {}",
            stderr.trim()
        );
    }

    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let _ = fs::remove_file(path);
    if bytes.is_empty() {
        anyhow::bail!("GNOME Shell screenshot D-Bus call produced an empty image");
    }

    Ok(bytes)
}

fn rendered_capture_path(request_id: Uuid, attempt: usize) -> Result<PathBuf> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .context("XDG_RUNTIME_DIR is not set for rendered search listener")?;
    Ok(runtime_dir
        .join("visionclip")
        .join("rendered-search")
        .join(format!("{request_id}-{attempt}.png")))
}

async fn resolve_capture_backends() -> Vec<RenderedCaptureBackend> {
    let gnome_shell_screenshot_usable = gnome_shell_screenshot_usable().await;
    resolve_capture_backends_with(
        current_session_type(),
        |program| command_path(program).is_some(),
        gnome_shell_screenshot_usable,
    )
}

async fn gnome_shell_screenshot_usable() -> bool {
    if !likely_gnome_shell_screenshot_available(|program| command_path(program).is_some()) {
        return false;
    }

    let Ok(path) = rendered_capture_path(Uuid::new_v4(), 0) else {
        return false;
    };
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }

    let Some(command_path) = command_path("gdbus") else {
        return false;
    };
    let args = vec![
        "call".to_string(),
        "--session".to_string(),
        "--dest".to_string(),
        "org.gnome.Shell.Screenshot".to_string(),
        "--object-path".to_string(),
        "/org/gnome/Shell/Screenshot".to_string(),
        "--method".to_string(),
        "org.gnome.Shell.Screenshot.Screenshot".to_string(),
        "false".to_string(),
        "false".to_string(),
        path.display().to_string(),
    ];

    let output = timeout(
        Duration::from_millis(1_500),
        Command::new(&command_path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await;

    let usable = matches!(output, Ok(Ok(output)) if output.status.success())
        && fs::metadata(&path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false);
    let _ = fs::remove_file(path);
    usable
}

fn resolve_capture_backends_with<F>(
    session_type: SessionType,
    command_exists: F,
    gnome_shell_screenshot_available: bool,
) -> Vec<RenderedCaptureBackend>
where
    F: Fn(&str) -> bool,
{
    discover_rendered_capture_backends(
        session_type,
        command_exists,
        gnome_shell_screenshot_available,
    )
    .into_iter()
    .filter_map(|backend| RenderedCaptureBackend::from_kind(backend.kind))
    .collect()
}

fn current_session_type() -> SessionType {
    match env::var("XDG_SESSION_TYPE") {
        Ok(value) if value.eq_ignore_ascii_case("wayland") => SessionType::Wayland,
        Ok(value) if value.eq_ignore_ascii_case("x11") => SessionType::X11,
        _ => SessionType::Unknown,
    }
}

fn command_path(program: &str) -> Option<PathBuf> {
    which(program).ok().or_else(|| {
        ["/usr/local/bin", "/usr/bin", "/bin"]
            .into_iter()
            .map(|dir| PathBuf::from(dir).join(program))
            .find(|path| path.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_backend_prefers_gnome_screenshot_for_gnome_hosts() {
        let backends = resolve_capture_backends_with(
            SessionType::Wayland,
            |program| program == "gnome-screenshot",
            false,
        );
        assert_eq!(backends, vec![RenderedCaptureBackend::GnomeScreenshot]);
    }

    #[test]
    fn capture_backend_detects_gnome_shell_dbus_for_gnome_hosts() {
        let backends = resolve_capture_backends_with(SessionType::Wayland, |_| false, true);
        assert_eq!(backends, vec![RenderedCaptureBackend::GnomeShellScreenshot]);
    }

    #[test]
    fn capture_backend_falls_back_to_grim_then_maim() {
        let backend =
            resolve_capture_backends_with(SessionType::Wayland, |program| program == "grim", false);
        assert_eq!(backend, vec![RenderedCaptureBackend::Grim]);

        let backend =
            resolve_capture_backends_with(SessionType::X11, |program| program == "maim", false);
        assert_eq!(backend, vec![RenderedCaptureBackend::Maim]);
    }
}
