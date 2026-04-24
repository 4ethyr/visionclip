use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::{sleep, timeout, Duration},
};
use urlencoding::decode;
use uuid::Uuid;
use visionclip_common::{
    config::CaptureConfig, current_desktops, screenshot_portal_backends_for_current_desktop,
    summarize_portal_backends, AppConfig, SessionType,
};
use which::which;

const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SCREENSHOT_METHOD: &str = "org.freedesktop.portal.Screenshot.Screenshot";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedCaptureBackend {
    Portal,
    Grim,
    Maim,
    GnomeScreenshot,
}

pub async fn load_image_bytes(
    image_path: Option<&PathBuf>,
    capture_command: Option<&str>,
    config: &AppConfig,
    session_type: SessionType,
) -> Result<Vec<u8>> {
    if let Some(path) = image_path {
        return fs::read(path)
            .with_context(|| format!("failed to read image at {}", path.display()));
    }

    if let Some(command) = capture_command {
        return capture_from_shell_command(command, config.capture.capture_timeout_ms).await;
    }

    capture_with_backend(&config.capture, session_type).await
}

async fn capture_with_backend(
    config: &CaptureConfig,
    session_type: SessionType,
) -> Result<Vec<u8>> {
    let backend = resolve_capture_backend(config, session_type, command_exists)?;
    let timeout_ms = config.capture_timeout_ms;

    match backend {
        ResolvedCaptureBackend::Portal => capture_with_portal(timeout_ms).await,
        ResolvedCaptureBackend::Grim => capture_with_grim(timeout_ms).await,
        ResolvedCaptureBackend::Maim => {
            capture_from_program("maim", &["-s", "-u"], timeout_ms).await
        }
        ResolvedCaptureBackend::GnomeScreenshot => capture_with_gnome_screenshot(timeout_ms).await,
    }
}

fn resolve_capture_backend<F>(
    config: &CaptureConfig,
    session_type: SessionType,
    command_exists: F,
) -> Result<ResolvedCaptureBackend>
where
    F: Fn(&str) -> bool,
{
    let backend = config.backend.trim().to_ascii_lowercase();

    match backend.as_str() {
        "auto" => resolve_auto_backend(config, session_type, command_exists),
        "portal" => require_backend(ResolvedCaptureBackend::Portal, "gdbus", command_exists),
        "grim" => require_backend(ResolvedCaptureBackend::Grim, "grim", command_exists),
        "maim" => require_backend(ResolvedCaptureBackend::Maim, "maim", command_exists),
        "gnome-screenshot" | "gnome_screenshot" => require_backend(
            ResolvedCaptureBackend::GnomeScreenshot,
            "gnome-screenshot",
            command_exists,
        ),
        other => anyhow::bail!("unsupported capture backend `{other}`"),
    }
}

fn resolve_auto_backend<F>(
    config: &CaptureConfig,
    session_type: SessionType,
    command_exists: F,
) -> Result<ResolvedCaptureBackend>
where
    F: Fn(&str) -> bool,
{
    if config.prefer_portal && command_exists("gdbus") {
        return Ok(ResolvedCaptureBackend::Portal);
    }

    match session_type {
        SessionType::Wayland => {
            if command_exists("gnome-screenshot") {
                Ok(ResolvedCaptureBackend::GnomeScreenshot)
            } else if command_exists("grim") {
                Ok(ResolvedCaptureBackend::Grim)
            } else {
                anyhow::bail!(
                    "no supported Wayland capture backend found; install gnome-screenshot or grim, or use --image/--capture-command"
                )
            }
        }
        SessionType::X11 => {
            if command_exists("maim") {
                Ok(ResolvedCaptureBackend::Maim)
            } else if command_exists("gnome-screenshot") {
                Ok(ResolvedCaptureBackend::GnomeScreenshot)
            } else if command_exists("grim") {
                Ok(ResolvedCaptureBackend::Grim)
            } else {
                anyhow::bail!(
                    "no supported X11 capture backend found; install maim or gnome-screenshot, or use --image/--capture-command"
                )
            }
        }
        SessionType::Unknown => {
            if command_exists("gnome-screenshot") {
                Ok(ResolvedCaptureBackend::GnomeScreenshot)
            } else if command_exists("maim") {
                Ok(ResolvedCaptureBackend::Maim)
            } else if command_exists("grim") {
                Ok(ResolvedCaptureBackend::Grim)
            } else {
                anyhow::bail!(
                    "no supported capture backend found; install gnome-screenshot, maim or grim, or use --image/--capture-command"
                )
            }
        }
    }
}

fn require_backend<F>(
    backend: ResolvedCaptureBackend,
    command: &str,
    command_exists: F,
) -> Result<ResolvedCaptureBackend>
where
    F: Fn(&str) -> bool,
{
    if command_exists(command) {
        Ok(backend)
    } else {
        anyhow::bail!("capture backend requires `{command}` but it is not installed")
    }
}

async fn capture_from_shell_command(command: &str, timeout_ms: u64) -> Result<Vec<u8>> {
    let output = run_command(
        "sh",
        &["-lc".to_string(), command.to_string()],
        timeout_ms,
        "capture command",
    )
    .await?;

    if output.stdout.is_empty() {
        anyhow::bail!("capture command produced no PNG bytes on stdout");
    }

    Ok(output.stdout)
}

async fn capture_from_program(program: &str, args: &[&str], timeout_ms: u64) -> Result<Vec<u8>> {
    let args = args
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let output = run_command(program, &args, timeout_ms, "capture backend").await?;

    if output.stdout.is_empty() {
        anyhow::bail!("capture backend `{program}` produced no PNG bytes on stdout");
    }

    Ok(output.stdout)
}

async fn capture_with_grim(timeout_ms: u64) -> Result<Vec<u8>> {
    if command_exists("slurp") {
        let selection = run_command("slurp", &[], timeout_ms, "grim selection helper").await?;
        let geometry = String::from_utf8_lossy(&selection.stdout)
            .trim()
            .to_string();

        if geometry.is_empty() {
            anyhow::bail!("slurp did not return a selection");
        }

        let args = vec!["-g".to_string(), geometry, "-".to_string()];
        let output = run_command("grim", &args, timeout_ms, "grim capture backend").await?;
        return Ok(output.stdout);
    }

    capture_from_program("grim", &["-"], timeout_ms).await
}

async fn capture_with_gnome_screenshot(timeout_ms: u64) -> Result<Vec<u8>> {
    let temp_path = temp_png_path();
    let args = vec![
        "-a".to_string(),
        "-f".to_string(),
        temp_path.display().to_string(),
    ];

    let _ = run_command(
        "gnome-screenshot",
        &args,
        timeout_ms,
        "gnome-screenshot capture backend",
    )
    .await?;

    let bytes = fs::read(&temp_path)
        .with_context(|| format!("failed to read screenshot file {}", temp_path.display()))?;
    let _ = fs::remove_file(&temp_path);
    Ok(bytes)
}

async fn capture_with_portal(timeout_ms: u64) -> Result<Vec<u8>> {
    let handle_token = format!("visionclip_{}", Uuid::new_v4().simple());
    let options = format!(
        "{{'handle_token': <'{}'>, 'interactive': <true>, 'modal': <true>}}",
        handle_token
    );
    let args = vec![
        "call".to_string(),
        "--session".to_string(),
        "--dest".to_string(),
        PORTAL_BUS.to_string(),
        "--object-path".to_string(),
        PORTAL_OBJECT_PATH.to_string(),
        "--method".to_string(),
        PORTAL_SCREENSHOT_METHOD.to_string(),
        "".to_string(),
        options,
    ];

    let mut monitor = start_portal_monitor()?;
    sleep(Duration::from_millis(150)).await;

    let output = run_command("gdbus", &args, timeout_ms, "portal screenshot request").await?;
    let handle = parse_object_path(&String::from_utf8_lossy(&output.stdout))
        .context("failed to parse portal request handle")?;
    let uri = wait_for_portal_response(&mut monitor, &handle, timeout_ms)
        .await
        .map_err(|error| anyhow::anyhow!("{error}. {}", portal_runtime_hint()))?;
    let path = file_uri_to_path(&uri)?;

    fs::read(&path)
        .with_context(|| format!("failed to read portal screenshot at {}", path.display()))
}

async fn wait_for_portal_response(
    monitor: &mut tokio::process::Child,
    handle: &str,
    timeout_ms: u64,
) -> Result<String> {
    let stdout = monitor
        .stdout
        .take()
        .context("failed to capture portal monitor stdout")?;
    let mut lines = BufReader::new(stdout).lines();

    let response = timeout(Duration::from_millis(timeout_ms), async {
        let mut transcript = String::new();
        let mut tracking_handle = false;

        while let Some(line) = lines.next_line().await? {
            if !tracking_handle {
                if line.contains(handle) {
                    tracking_handle = true;
                } else {
                    continue;
                }
            }

            if line.starts_with("/org/freedesktop/portal/desktop/request/")
                && !line.contains(handle)
            {
                transcript.clear();
                tracking_handle = false;
                continue;
            }

            transcript.push_str(&line);
            transcript.push('\n');

            if let Some(code) = extract_portal_response_code(&transcript) {
                if code == 0 {
                    if let Some(uri) = extract_portal_uri(&transcript) {
                        return Ok(uri);
                    }
                } else {
                    anyhow::bail!("portal screenshot canceled (response code {code})");
                }
            }
        }

        anyhow::bail!("portal response monitor exited before returning a screenshot URI")
    })
    .await
    .with_context(|| format!("portal screenshot timed out after {timeout_ms} ms"))??;

    let _ = monitor.kill().await;
    Ok(response)
}

fn start_portal_monitor() -> Result<tokio::process::Child> {
    let mut child = Command::new("gdbus");
    child
        .args(["monitor", "--session", "--dest", PORTAL_BUS])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    child
        .spawn()
        .context("failed to start gdbus monitor for portal response")
}

fn portal_runtime_hint() -> String {
    let desktops = current_desktops();
    let backends = screenshot_portal_backends_for_current_desktop();
    let desktop_label = if desktops.is_empty() {
        "unknown".into()
    } else {
        desktops.join(":")
    };

    if backends.is_empty() {
        format!(
            "No screenshot-capable xdg-desktop-portal backend was detected for desktop `{desktop_label}`"
        )
    } else {
        format!(
            "Complete the portal dialog in desktop `{desktop_label}` or review the active screenshot backends: {}",
            summarize_portal_backends(&backends)
        )
    }
}

async fn run_command(
    program: &str,
    args: &[String],
    timeout_ms: u64,
    label: &str,
) -> Result<std::process::Output> {
    let rendered = render_command(program, args);
    let mut command = Command::new(program);
    command.args(args).kill_on_drop(true);

    let output = timeout(Duration::from_millis(timeout_ms), command.output())
        .await
        .with_context(|| format!("{label} timed out after {timeout_ms} ms: `{rendered}`"))?
        .with_context(|| format!("failed to execute {label} `{rendered}`"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            anyhow::bail!("{label} failed with status {}: `{rendered}`", output.status);
        } else {
            anyhow::bail!("{label} failed with status {}: {}", output.status, stderr);
        }
    }

    Ok(output)
}

fn parse_object_path(output: &str) -> Option<String> {
    let start = output.find("/org/")?;
    let tail = &output[start..];
    let end = tail
        .find(|ch: char| matches!(ch, '\'' | '"' | ')' | ',' | '\n' | '\r' | ' '))
        .unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

fn extract_portal_response_code(text: &str) -> Option<u32> {
    if !text.contains("org.freedesktop.portal.Request.Response") {
        return None;
    }

    let marker = "uint32 ";
    let start = text.rfind(marker)? + marker.len();
    let digits = text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();

    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn extract_portal_uri(text: &str) -> Option<String> {
    let start = text.find("file://")?;
    let tail = &text[start..];
    let end = tail
        .find(|ch: char| matches!(ch, '\'' | '"' | '>' | ')' | '\n' | '\r' | ' '))
        .unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

fn file_uri_to_path(uri: &str) -> Result<PathBuf> {
    let uri = uri
        .strip_prefix("file://")
        .with_context(|| format!("unsupported screenshot URI `{uri}`"))?;
    let decoded = decode(uri).context("failed to decode screenshot file URI")?;
    Ok(Path::new(decoded.as_ref()).to_path_buf())
}

fn temp_png_path() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join(format!("visionclip-{}.png", Uuid::new_v4()))
}

fn command_exists(command: &str) -> bool {
    which(command).is_ok()
}

fn render_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_backend_prefers_portal_when_available() {
        let backend = resolve_capture_backend(
            &CaptureConfig {
                backend: "auto".into(),
                prefer_portal: true,
                capture_timeout_ms: 30_000,
            },
            SessionType::Wayland,
            |command| command == "gdbus",
        )
        .unwrap();

        assert_eq!(backend, ResolvedCaptureBackend::Portal);
    }

    #[test]
    fn auto_backend_uses_grim_on_wayland() {
        let backend = resolve_capture_backend(
            &CaptureConfig {
                backend: "auto".into(),
                prefer_portal: false,
                capture_timeout_ms: 30_000,
            },
            SessionType::Wayland,
            |command| command == "grim",
        )
        .unwrap();

        assert_eq!(backend, ResolvedCaptureBackend::Grim);
    }

    #[test]
    fn auto_backend_uses_maim_on_x11() {
        let backend = resolve_capture_backend(
            &CaptureConfig {
                backend: "auto".into(),
                prefer_portal: false,
                capture_timeout_ms: 30_000,
            },
            SessionType::X11,
            |command| command == "maim",
        )
        .unwrap();

        assert_eq!(backend, ResolvedCaptureBackend::Maim);
    }

    #[test]
    fn parse_object_path_from_gdbus_output() {
        let output =
            "(objectpath '/org/freedesktop/portal/desktop/request/1_341/visionclip_token',)\n";
        assert_eq!(
            parse_object_path(output),
            Some("/org/freedesktop/portal/desktop/request/1_341/visionclip_token".into())
        );
    }

    #[test]
    fn parse_portal_response_line() {
        let text = "/org/freedesktop/portal/desktop/request/1_341/visionclip_token: org.freedesktop.portal.Request.Response (uint32 0, {'uri': <'file:///tmp/screenshot%20one.png'>})";
        assert_eq!(extract_portal_response_code(text), Some(0));
        assert_eq!(
            extract_portal_uri(text),
            Some("file:///tmp/screenshot%20one.png".into())
        );
    }

    #[test]
    fn decode_file_uri_to_path() {
        let path = file_uri_to_path("file:///tmp/screenshot%20one.png").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/screenshot one.png"));
    }
}
