use anyhow::{Context, Result};
use std::{
    collections::VecDeque,
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::SystemTime,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::{sleep, timeout, Duration, Instant},
};
use tracing::{debug, warn};
use urlencoding::decode;
use uuid::Uuid;
use visionclip_common::{
    config::CaptureConfig, current_desktops, likely_gnome_shell_screenshot_available,
    screenshot_portal_backends_for_current_desktop, summarize_portal_backends, AppConfig,
    SessionType,
};
use which::which;

const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SCREENSHOT_METHOD: &str = "org.freedesktop.portal.Screenshot.Screenshot";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedCaptureBackend {
    Portal,
    GnomeShellScreenshot,
    Grim,
    Maim,
    GnomeScreenshot,
}

#[derive(Debug, Default)]
struct PortalMonitorTrace {
    saw_handle_line: bool,
    saw_response_signal: bool,
    response_code: Option<u32>,
    uri: Option<String>,
    recent_lines: VecDeque<String>,
}

impl PortalMonitorTrace {
    fn reset_for_other_request(&mut self) {
        self.saw_handle_line = false;
        self.saw_response_signal = false;
        self.response_code = None;
        self.uri = None;
        self.recent_lines.clear();
    }

    fn record_line(&mut self, line: &str) {
        const MAX_RECENT_LINES: usize = 4;
        const MAX_LINE_CHARS: usize = 160;

        let sanitized = if line.chars().count() > MAX_LINE_CHARS {
            let truncated = line.chars().take(MAX_LINE_CHARS).collect::<String>();
            format!("{truncated}...")
        } else {
            line.to_string()
        };

        self.recent_lines.push_back(sanitized);
        while self.recent_lines.len() > MAX_RECENT_LINES {
            self.recent_lines.pop_front();
        }
    }

    fn summary(&self) -> String {
        let recent_lines = if self.recent_lines.is_empty() {
            "none".to_string()
        } else {
            self.recent_lines
                .iter()
                .map(|line| format!("`{line}`"))
                .collect::<Vec<_>>()
                .join(" | ")
        };

        format!(
            "handle_seen={}, response_seen={}, response_code={}, uri_seen={}, recent_lines={}",
            yes_no(self.saw_handle_line),
            yes_no(self.saw_response_signal),
            self.response_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            yes_no(self.uri.is_some()),
            recent_lines
        )
    }
}

enum PortalResponseOutcome {
    Uri(String),
    Fallback(PathBuf),
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
        ResolvedCaptureBackend::GnomeShellScreenshot => {
            capture_with_gnome_shell_screenshot(timeout_ms).await
        }
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
        "gnome-shell" | "gnome_shell" | "gnome-shell-screenshot" | "gnome_shell_screenshot" => {
            require_backend(
                ResolvedCaptureBackend::GnomeShellScreenshot,
                "gdbus",
                command_exists,
            )
        }
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
            } else if likely_gnome_shell_screenshot_available(&command_exists) {
                Ok(ResolvedCaptureBackend::GnomeShellScreenshot)
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
            } else if likely_gnome_shell_screenshot_available(&command_exists) {
                Ok(ResolvedCaptureBackend::GnomeShellScreenshot)
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
            } else if likely_gnome_shell_screenshot_available(&command_exists) {
                Ok(ResolvedCaptureBackend::GnomeShellScreenshot)
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

async fn capture_with_gnome_shell_screenshot(timeout_ms: u64) -> Result<Vec<u8>> {
    let area_output = run_command(
        "gdbus",
        &[
            "call".to_string(),
            "--session".to_string(),
            "--dest".to_string(),
            "org.gnome.Shell.Screenshot".to_string(),
            "--object-path".to_string(),
            "/org/gnome/Shell/Screenshot".to_string(),
            "--method".to_string(),
            "org.gnome.Shell.Screenshot.SelectArea".to_string(),
        ],
        timeout_ms,
        "GNOME Shell screenshot area selector",
    )
    .await?;

    let (x, y, width, height) =
        parse_gnome_shell_area(&String::from_utf8_lossy(&area_output.stdout))
            .context("failed to parse GNOME Shell selected screenshot area")?;
    if width <= 0 || height <= 0 {
        anyhow::bail!("GNOME Shell selected an empty screenshot area");
    }

    let temp_path = temp_png_path();
    let args = vec![
        "call".to_string(),
        "--session".to_string(),
        "--dest".to_string(),
        "org.gnome.Shell.Screenshot".to_string(),
        "--object-path".to_string(),
        "/org/gnome/Shell/Screenshot".to_string(),
        "--method".to_string(),
        "org.gnome.Shell.Screenshot.ScreenshotArea".to_string(),
        x.to_string(),
        y.to_string(),
        width.to_string(),
        height.to_string(),
        "true".to_string(),
        temp_path.display().to_string(),
    ];

    let _ = run_command(
        "gdbus",
        &args,
        timeout_ms,
        "GNOME Shell screenshot capture backend",
    )
    .await?;

    let bytes = fs::read(&temp_path)
        .with_context(|| format!("failed to read screenshot file {}", temp_path.display()))?;
    let _ = fs::remove_file(&temp_path);
    Ok(bytes)
}

async fn capture_with_portal(timeout_ms: u64) -> Result<Vec<u8>> {
    let capture_started_at = SystemTime::now();
    let wait_started_at = Instant::now();
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

    debug!(timeout_ms, handle_token = %handle_token, "starting portal screenshot capture");
    let mut monitor = start_portal_monitor()?;
    sleep(Duration::from_millis(150)).await;

    let output = run_command("gdbus", &args, timeout_ms, "portal screenshot request").await?;
    let request_output = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!(request_output = %request_output, "portal screenshot request completed");
    let handle = parse_object_path(&String::from_utf8_lossy(&output.stdout))
        .context("failed to parse portal request handle")?;
    debug!(handle = %handle, "parsed portal screenshot request handle");
    let uri = match wait_for_portal_response(&mut monitor, &handle, timeout_ms, capture_started_at)
        .await
    {
        Ok(PortalResponseOutcome::Uri(uri)) => uri,
        Ok(PortalResponseOutcome::Fallback(path)) => {
            warn!(
                handle = %handle,
                path = %path.display(),
                fallback_wait_ms = elapsed_ms(wait_started_at),
                "portal response not observed; using recently created screenshot file fallback"
            );
            let bytes = fs::read(&path).with_context(|| {
                format!(
                    "failed to read fallback screenshot file {} after portal fallback",
                    path.display()
                )
            })?;
            debug!(
                handle = %handle,
                path = %path.display(),
                bytes = bytes.len(),
                "read fallback screenshot bytes"
            );
            return Ok(bytes);
        }
        Err(error) => {
            if let Some(path) = find_ready_recent_screenshot_file(capture_started_at).await {
                warn!(
                    handle = %handle,
                    path = %path.display(),
                    error = %error,
                    "portal returned no response; using recently created screenshot file fallback"
                );
                let bytes = fs::read(&path).with_context(|| {
                    format!(
                        "failed to read fallback screenshot file {} after portal timeout",
                        path.display()
                    )
                })?;
                debug!(
                    handle = %handle,
                    path = %path.display(),
                    bytes = bytes.len(),
                    "read fallback screenshot bytes after portal timeout"
                );
                return Ok(bytes);
            }

            return Err(anyhow::anyhow!("{error}. {}", portal_runtime_hint()));
        }
    };
    debug!(handle = %handle, uri = %uri, "portal screenshot returned URI");
    let path = file_uri_to_path(&uri)?;
    let exists = path.exists();
    let file_size = fs::metadata(&path).map(|metadata| metadata.len()).ok();
    debug!(
        handle = %handle,
        path = %path.display(),
        exists,
        file_size,
        "resolved portal screenshot file"
    );

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read portal screenshot at {}", path.display()))?;
    debug!(
        handle = %handle,
        path = %path.display(),
        bytes = bytes.len(),
        "read portal screenshot bytes"
    );
    Ok(bytes)
}

fn parse_gnome_shell_area(output: &str) -> Option<(i32, i32, i32, i32)> {
    let normalized = output.replace("int32", "").replace("uint32", "");
    let values = parse_signed_ints(&normalized);
    (values.len() >= 4).then(|| (values[0], values[1], values[2], values[3]))
}

fn parse_signed_ints(input: &str) -> Vec<i32> {
    let mut values = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if ch.is_ascii_digit() || (ch == '-' && current.is_empty()) {
            current.push(ch);
            continue;
        }

        if !current.is_empty() && current != "-" {
            if let Ok(value) = current.parse() {
                values.push(value);
            }
        }
        current.clear();
    }

    if !current.is_empty() && current != "-" {
        if let Ok(value) = current.parse() {
            values.push(value);
        }
    }

    values
}

async fn wait_for_portal_response(
    monitor: &mut tokio::process::Child,
    handle: &str,
    timeout_ms: u64,
    capture_started_at: SystemTime,
) -> Result<PortalResponseOutcome> {
    let stdout = monitor
        .stdout
        .take()
        .context("failed to capture portal monitor stdout")?;
    let mut lines = BufReader::new(stdout).lines();
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut transcript = String::new();
    let mut trace = PortalMonitorTrace::default();
    let mut tracking_handle = false;

    loop {
        let now = Instant::now();
        if now >= deadline {
            let _ = monitor.kill().await;
            anyhow::bail!(
                "portal screenshot timed out after {timeout_ms} ms ({})",
                trace.summary()
            );
        }

        let wait_for = std::cmp::min(deadline - now, Duration::from_millis(250));
        let maybe_line = match timeout(wait_for, lines.next_line()).await {
            Ok(line) => line?,
            Err(_) => {
                if let Some(path) = find_ready_recent_screenshot_file(capture_started_at).await {
                    let _ = monitor.kill().await;
                    return Ok(PortalResponseOutcome::Fallback(path));
                }
                continue;
            }
        };

        let Some(line) = maybe_line else {
            let _ = monitor.kill().await;
            anyhow::bail!(
                "portal response monitor exited before returning a screenshot URI ({})",
                trace.summary()
            );
        };

        if !tracking_handle {
            if line.contains(handle) {
                tracking_handle = true;
                trace.saw_handle_line = true;
                trace.record_line(&line);
                debug!(handle = %handle, line = %line, "portal monitor observed request handle");
            } else {
                continue;
            }
        } else if line.starts_with("/org/freedesktop/portal/desktop/request/")
            && !line.contains(handle)
        {
            debug!(handle = %handle, line = %line, "portal monitor switched to a different request");
            transcript.clear();
            tracking_handle = false;
            trace.reset_for_other_request();
            continue;
        } else {
            trace.record_line(&line);
        }

        if line.contains("org.freedesktop.portal.Request.Response") {
            trace.saw_response_signal = true;
            debug!(handle = %handle, line = %line, "portal monitor observed response signal");
        }

        transcript.push_str(&line);
        transcript.push('\n');

        if let Some(code) = extract_portal_response_code(&transcript) {
            trace.response_code = Some(code);

            if code == 0 {
                if let Some(uri) = extract_portal_uri(&transcript) {
                    trace.uri = Some(uri.clone());
                    debug!(handle = %handle, uri = %uri, "portal response included screenshot URI");
                    let _ = monitor.kill().await;
                    return Ok(PortalResponseOutcome::Uri(uri));
                }
            } else {
                warn!(
                    handle = %handle,
                    response_code = code,
                    trace = %trace.summary(),
                    "portal screenshot canceled or failed"
                );
                let _ = monitor.kill().await;
                anyhow::bail!(
                    "portal screenshot canceled (response code {code}, {})",
                    trace.summary()
                );
            }
        }
    }
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
        .find(['\'', '"', ')', ',', '\n', '\r', ' '])
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
        .find(['\'', '"', '>', ')', '\n', '\r', ' '])
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

async fn find_ready_recent_screenshot_file(started_at: SystemTime) -> Option<PathBuf> {
    find_ready_recent_screenshot_file_in_dirs(&screenshot_candidate_dirs(), started_at).await
}

async fn find_ready_recent_screenshot_file_in_dirs(
    candidate_dirs: &[PathBuf],
    cutoff: SystemTime,
) -> Option<PathBuf> {
    let path = find_recent_screenshot_file_in_dirs(candidate_dirs, cutoff)?;
    if screenshot_file_ready(&path).await {
        Some(path)
    } else {
        None
    }
}

fn find_recent_screenshot_file_in_dirs(
    candidate_dirs: &[PathBuf],
    cutoff: SystemTime,
) -> Option<PathBuf> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;

    for dir in candidate_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                debug!(
                    path = %dir.display(),
                    error = %error,
                    "skipping screenshot recovery directory"
                );
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !is_supported_screenshot_path(&path) {
                continue;
            }

            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if modified_at < cutoff {
                continue;
            }

            let replace = newest
                .as_ref()
                .map(|(current_modified_at, _)| modified_at > *current_modified_at)
                .unwrap_or(true);

            if replace {
                newest = Some((modified_at, path));
            }
        }
    }

    if let Some((modified_at, path)) = newest {
        debug!(
            path = %path.display(),
            modified_at = ?modified_at,
            "found recent screenshot recovery candidate"
        );
        Some(path)
    } else {
        None
    }
}

async fn screenshot_file_ready(path: &Path) -> bool {
    let Some(initial_size) = file_size(path) else {
        return false;
    };

    if initial_size == 0 {
        return false;
    }

    sleep(Duration::from_millis(150)).await;
    matches!(file_size(path), Some(final_size) if final_size == initial_size && final_size > 0)
}

fn file_size(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

fn screenshot_candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(pictures_dir) = pictures_dir() {
        append_screenshot_candidate_dirs(&mut dirs, &pictures_dir);
    }

    dirs
}

fn append_screenshot_candidate_dirs(paths: &mut Vec<PathBuf>, pictures_dir: &Path) {
    push_unique_path(paths, pictures_dir.join("Screenshots"));
    push_unique_path(paths, pictures_dir.to_path_buf());
}

fn pictures_dir() -> Option<PathBuf> {
    env::var("XDG_PICTURES_DIR")
        .ok()
        .and_then(|value| expand_home_path(&value))
        .or_else(read_pictures_dir_from_user_dirs)
        .or_else(|| {
            env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join("Pictures"))
        })
}

fn read_pictures_dir_from_user_dirs() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    let user_dirs_path = PathBuf::from(home).join(".config/user-dirs.dirs");
    let contents = fs::read_to_string(user_dirs_path).ok()?;

    contents.lines().find_map(|line| {
        let line = line.trim();
        if !line.starts_with("XDG_PICTURES_DIR=") {
            return None;
        }

        let (_, value) = line.split_once('=')?;
        expand_home_path(value)
    })
}

fn expand_home_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    if let Some(stripped) = trimmed.strip_prefix("$HOME/") {
        let home = env::var("HOME").ok()?;
        return Some(PathBuf::from(home).join(stripped));
    }

    if trimmed == "$HOME" {
        return env::var("HOME").ok().map(PathBuf::from);
    }

    Some(PathBuf::from(trimmed))
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if paths.iter().all(|existing| existing != &path) {
        paths.push(path);
    }
}

fn is_supported_screenshot_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
        .unwrap_or(false)
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

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
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
    fn explicit_gnome_shell_backend_uses_gdbus() {
        let backend = resolve_capture_backend(
            &CaptureConfig {
                backend: "gnome-shell".into(),
                prefer_portal: false,
                capture_timeout_ms: 30_000,
            },
            SessionType::Wayland,
            |command| command == "gdbus",
        )
        .unwrap();

        assert_eq!(backend, ResolvedCaptureBackend::GnomeShellScreenshot);
    }

    #[test]
    fn parse_gnome_shell_selected_area() {
        assert_eq!(
            parse_gnome_shell_area("(10, 20, 300, 180)"),
            Some((10, 20, 300, 180))
        );
        assert_eq!(
            parse_gnome_shell_area("(int32 10, int32 20, int32 300, int32 180)"),
            Some((10, 20, 300, 180))
        );
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
        let text = "/org/freedesktop/portal/desktop/request/1_341/visionclip_token: org.freedesktop.portal.Request.Response (uint32 0, {'uri': <'file:///home/demo/Pictures/screenshot%20one.png'>})";
        assert_eq!(extract_portal_response_code(text), Some(0));
        assert_eq!(
            extract_portal_uri(text),
            Some("file:///home/demo/Pictures/screenshot%20one.png".into())
        );
    }

    #[test]
    fn decode_file_uri_to_path() {
        let path = file_uri_to_path("file:///home/demo/Pictures/screenshot%20one.png").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/demo/Pictures/screenshot one.png")
        );
    }

    #[test]
    fn portal_monitor_trace_summary_reports_recent_lines() {
        let mut trace = PortalMonitorTrace {
            saw_handle_line: true,
            saw_response_signal: false,
            response_code: None,
            uri: None,
            recent_lines: VecDeque::new(),
        };
        trace.record_line("first line");
        trace.record_line("second line");

        let summary = trace.summary();
        assert!(summary.contains("handle_seen=yes"));
        assert!(summary.contains("response_seen=no"));
        assert!(summary.contains("`first line` | `second line`"));
    }

    #[test]
    fn find_recent_screenshot_file_prefers_newest_candidate() {
        let base = std::env::temp_dir().join(format!("visionclip-test-{}", Uuid::new_v4()));
        let screenshots = base.join("Screenshots");
        fs::create_dir_all(&screenshots).unwrap();

        let old_file = screenshots.join("old.png");
        fs::write(&old_file, b"old").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let started_at = SystemTime::now();
        std::thread::sleep(Duration::from_millis(20));

        let new_file = screenshots.join("new.png");
        fs::write(&new_file, b"new").unwrap();

        let mut found = Vec::new();
        append_screenshot_candidate_dirs(&mut found, &base);
        let selected = find_recent_screenshot_file_in_dirs(&found, started_at);

        assert_eq!(selected, Some(new_file));

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn find_ready_recent_screenshot_file_waits_for_new_file() {
        let base = std::env::temp_dir().join(format!("visionclip-test-{}", Uuid::new_v4()));
        let screenshots = base.join("Screenshots");
        fs::create_dir_all(&screenshots).unwrap();

        let started_at = SystemTime::now();
        tokio::time::sleep(Duration::from_millis(25)).await;

        let new_file = screenshots.join("fresh.png");
        fs::write(&new_file, b"png-bytes").unwrap();

        let mut found = Vec::new();
        append_screenshot_candidate_dirs(&mut found, &base);
        let selected = find_ready_recent_screenshot_file_in_dirs(&found, started_at).await;

        assert_eq!(selected, Some(new_file));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn expand_home_path_supports_home_variable() {
        let home = env::var("HOME").unwrap();
        let path = expand_home_path("$HOME/Pictures/Screenshots").unwrap();
        assert_eq!(path, PathBuf::from(home).join("Pictures/Screenshots"));
    }
}
