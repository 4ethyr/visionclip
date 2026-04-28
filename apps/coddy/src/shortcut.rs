use anyhow::{Context, Result};
use std::{
    env, fmt, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_BINDING_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/coddy-voice-search/";
const CUSTOM_BINDING_SCHEMA: &str =
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/coddy-voice-search/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutEnvironment {
    pub socket_path: PathBuf,
    pub socket_exists: bool,
    pub display: Option<String>,
    pub wayland_display: Option<String>,
    pub xdg_runtime_dir: Option<PathBuf>,
    pub dbus_session_bus_address: Option<String>,
}

impl ShortcutEnvironment {
    pub fn detect(socket_path: PathBuf) -> Self {
        Self {
            socket_exists: socket_path.exists(),
            socket_path,
            display: env::var("DISPLAY").ok(),
            wayland_display: env::var("WAYLAND_DISPLAY").ok(),
            xdg_runtime_dir: env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
            dbus_session_bus_address: env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
        }
    }

    pub fn has_graphical_session(&self) -> bool {
        self.display.is_some() || self.wayland_display.is_some()
    }

    pub fn validate_for_shortcut(&self) -> Result<()> {
        if !self.socket_exists {
            anyhow::bail!(
                "daemon socket not found at {}; start or restart visionclip-daemon",
                self.socket_path.display()
            );
        }
        if !self.has_graphical_session() {
            anyhow::bail!("no graphical DISPLAY or WAYLAND_DISPLAY found for shortcut execution");
        }
        if self.xdg_runtime_dir.is_none() {
            anyhow::bail!("XDG_RUNTIME_DIR is required for Coddy shortcut locking");
        }
        Ok(())
    }

    pub fn lock_path(&self) -> Result<PathBuf> {
        let runtime_dir = self
            .xdg_runtime_dir
            .as_ref()
            .context("XDG_RUNTIME_DIR is required for Coddy shortcut locking")?;
        Ok(runtime_dir.join("visionclip").join("coddy-voice.lock"))
    }
}

impl fmt::Display for ShortcutEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "Coddy shortcut doctor")?;
        writeln!(formatter, "socket: {}", self.socket_path.display())?;
        writeln!(formatter, "socket_exists: {}", self.socket_exists)?;
        writeln!(
            formatter,
            "DISPLAY: {}",
            self.display.as_deref().unwrap_or("<unset>")
        )?;
        writeln!(
            formatter,
            "WAYLAND_DISPLAY: {}",
            self.wayland_display.as_deref().unwrap_or("<unset>")
        )?;
        writeln!(
            formatter,
            "XDG_RUNTIME_DIR: {}",
            self.xdg_runtime_dir
                .as_deref()
                .map(Path::display)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<unset>".to_string())
        )?;
        writeln!(
            formatter,
            "DBUS_SESSION_BUS_ADDRESS: {}",
            self.dbus_session_bus_address
                .as_deref()
                .unwrap_or("<unset>")
        )
    }
}

#[derive(Debug)]
pub struct VoiceShortcutLock {
    path: PathBuf,
}

impl VoiceShortcutLock {
    pub fn acquire(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create Coddy lock directory {}", parent.display())
            })?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "Coddy voice shortcut is already active or lock is stale at {}",
                    path.display()
                )
            })?;
        writeln!(file, "pid={}", std::process::id())?;

        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for VoiceShortcutLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutInstallPlan {
    pub name: String,
    pub requested_binding: String,
    pub resolved_binding: String,
    pub wrapper_path: PathBuf,
    pub coddy_bin: PathBuf,
}

impl ShortcutInstallPlan {
    pub fn new(binding: impl Into<String>, coddy_bin: PathBuf) -> Result<Self> {
        let requested_binding = binding.into();
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is required to install Coddy shortcut wrapper")?;
        Ok(Self {
            name: "Coddy Voice".to_string(),
            resolved_binding: normalize_binding(&requested_binding),
            requested_binding,
            wrapper_path: home.join(".local/bin/coddy-voice-search"),
            coddy_bin,
        })
    }

    pub fn wrapper_script(&self) -> String {
        format!(
            r#"#!/usr/bin/env bash
set -euo pipefail

LOG_DIR="${{HOME}}/.local/state/visionclip"
LOG_FILE="${{LOG_DIR}}/coddy-shortcut.log"
mkdir -p "$LOG_DIR"

import_user_env_var() {{
    local key="$1"
    if [[ -n "${{!key:-}}" ]]; then
        return
    fi
    if ! command -v systemctl >/dev/null 2>&1; then
        return
    fi

    local line
    while IFS= read -r line; do
        case "$line" in
            "${{key}}="*)
                export "$line"
                return
                ;;
        esac
    done < <(systemctl --user show-environment 2>/dev/null || true)
}}

for key in DISPLAY WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS; do
    import_user_env_var "$key"
done

export PATH="${{PATH:-${{HOME}}/.local/bin:/usr/local/bin:/usr/bin:/bin}}"

{{
    printf '%s coddy voice shortcut invoked\n' "$(date --iso-8601=seconds)"
    printf 'binary=%s\n' "{bin}"
    printf 'env DISPLAY=%s WAYLAND_DISPLAY=%s XDG_SESSION_TYPE=%s XDG_CURRENT_DESKTOP=%s XDG_RUNTIME_DIR=%s\n' "${{DISPLAY:-}}" "${{WAYLAND_DISPLAY:-}}" "${{XDG_SESSION_TYPE:-}}" "${{XDG_CURRENT_DESKTOP:-}}" "${{XDG_RUNTIME_DIR:-}}"
}} >>"$LOG_FILE"

if [[ -t 1 || -t 2 ]]; then
    exec "{bin}" --speak voice --overlay "$@"
else
    exec "{bin}" --speak voice --overlay "$@" >>"$LOG_FILE" 2>&1
fi
"#,
            bin = self.coddy_bin.display()
        )
    }
}

pub fn install_gnome_shortcut(plan: &ShortcutInstallPlan, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }

    ensure_command_available("gsettings")?;
    write_wrapper(plan)?;

    let current_bindings = command_output(
        Command::new("gsettings")
            .arg("get")
            .arg(MEDIA_KEYS_SCHEMA)
            .arg("custom-keybindings"),
    )?;
    let updated_bindings =
        append_custom_keybinding_path(current_bindings.trim(), CUSTOM_BINDING_PATH);

    run_command(
        Command::new("gsettings")
            .arg("set")
            .arg(MEDIA_KEYS_SCHEMA)
            .arg("custom-keybindings")
            .arg(updated_bindings),
    )?;
    run_command(
        Command::new("gsettings")
            .arg("set")
            .arg(CUSTOM_BINDING_SCHEMA)
            .arg("name")
            .arg(&plan.name),
    )?;
    run_command(
        Command::new("gsettings")
            .arg("set")
            .arg(CUSTOM_BINDING_SCHEMA)
            .arg("command")
            .arg(plan.wrapper_path.to_string_lossy().as_ref()),
    )?;
    run_command(
        Command::new("gsettings")
            .arg("set")
            .arg(CUSTOM_BINDING_SCHEMA)
            .arg("binding")
            .arg(&plan.resolved_binding),
    )?;

    let _ = Command::new("systemctl")
        .arg("--user")
        .arg("import-environment")
        .arg("DISPLAY")
        .arg("WAYLAND_DISPLAY")
        .arg("XDG_CURRENT_DESKTOP")
        .arg("XDG_SESSION_TYPE")
        .arg("XDG_RUNTIME_DIR")
        .arg("DBUS_SESSION_BUS_ADDRESS")
        .arg("PATH")
        .status();

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GnomeShortcutStatus {
    pub gsettings_available: bool,
    pub custom_keybindings: Option<String>,
    pub binding: Option<String>,
    pub command: Option<String>,
    pub wrapper_exists: bool,
}

impl GnomeShortcutStatus {
    pub fn detect(wrapper_path: &Path) -> Self {
        if ensure_command_available("gsettings").is_err() {
            return Self {
                gsettings_available: false,
                custom_keybindings: None,
                binding: None,
                command: None,
                wrapper_exists: wrapper_path.exists(),
            };
        }

        Self {
            gsettings_available: true,
            custom_keybindings: gsettings_get(MEDIA_KEYS_SCHEMA, "custom-keybindings"),
            binding: gsettings_get(CUSTOM_BINDING_SCHEMA, "binding").map(strip_gsettings_quotes),
            command: gsettings_get(CUSTOM_BINDING_SCHEMA, "command").map(strip_gsettings_quotes),
            wrapper_exists: wrapper_path.exists(),
        }
    }
}

impl fmt::Display for GnomeShortcutStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "GNOME shortcut status")?;
        writeln!(
            formatter,
            "gsettings_available: {}",
            self.gsettings_available
        )?;
        writeln!(
            formatter,
            "custom_keybindings: {}",
            self.custom_keybindings
                .as_deref()
                .unwrap_or("<unavailable>")
        )?;
        writeln!(
            formatter,
            "binding: {}",
            self.binding.as_deref().unwrap_or("<unavailable>")
        )?;
        writeln!(
            formatter,
            "command: {}",
            self.command.as_deref().unwrap_or("<unavailable>")
        )?;
        writeln!(formatter, "wrapper_exists: {}", self.wrapper_exists)
    }
}

pub fn normalize_binding(value: &str) -> String {
    let lowered = value.to_ascii_lowercase().replace(' ', "");
    match lowered.as_str() {
        "/+f12" | "slash+f12" => "<Mod4>F12".to_string(),
        "shift+capslk" | "shift+capslock" | "shift+caps_lock" => "<Shift>Caps_Lock".to_string(),
        _ => value.replace("<Super>", "<Mod4>"),
    }
}

pub fn append_custom_keybinding_path(current: &str, path: &str) -> String {
    if current == "@as []" || current == "[]" || current.is_empty() {
        return format!("['{path}']");
    }
    if current.contains(&format!("'{path}'")) {
        return current.to_string();
    }

    let trimmed = current.trim_end_matches(']');
    format!("{trimmed}, '{path}']")
}

pub fn default_wrapper_path() -> Result<PathBuf> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is required to resolve Coddy shortcut wrapper")?;
    Ok(home.join(".local/bin/coddy-voice-search"))
}

fn write_wrapper(plan: &ShortcutInstallPlan) -> Result<()> {
    if let Some(parent) = plan.wrapper_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create wrapper dir {}", parent.display()))?;
    }
    fs::write(&plan.wrapper_path, plan.wrapper_script())
        .with_context(|| format!("failed to write wrapper {}", plan.wrapper_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&plan.wrapper_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

fn ensure_command_available(command: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command} >/dev/null 2>&1"))
        .status()
        .with_context(|| format!("failed to probe command {command}"))?;
    if !status.success() {
        anyhow::bail!("required command not found: {command}");
    }
    Ok(())
}

fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    command_output(Command::new("gsettings").arg("get").arg(schema).arg(key))
        .ok()
        .map(|value| value.trim().to_string())
}

fn strip_gsettings_quotes(value: String) -> String {
    value
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .to_string()
}

fn command_output(command: &mut Command) -> Result<String> {
    let output = command.output().context("failed to run command")?;
    if !output.status.success() {
        anyhow::bail!(
            "command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_command(command: &mut Command) -> Result<()> {
    let output = command.output().context("failed to run command")?;
    if !output.status.success() {
        anyhow::bail!(
            "command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn graphical_session_accepts_wayland_or_x11() {
        let mut environment = test_environment();
        environment.display = None;
        environment.wayland_display = Some("wayland-0".to_string());
        assert!(environment.has_graphical_session());

        environment.display = Some(":0".to_string());
        environment.wayland_display = None;
        assert!(environment.has_graphical_session());

        environment.display = None;
        assert!(!environment.has_graphical_session());
    }

    #[test]
    fn validation_requires_socket_graphical_session_and_runtime_dir() {
        let mut environment = test_environment();
        assert!(environment.validate_for_shortcut().is_ok());

        environment.socket_exists = false;
        assert!(environment.validate_for_shortcut().is_err());

        environment = test_environment();
        environment.display = None;
        environment.wayland_display = None;
        assert!(environment.validate_for_shortcut().is_err());

        environment = test_environment();
        environment.xdg_runtime_dir = None;
        assert!(environment.validate_for_shortcut().is_err());
    }

    #[test]
    fn lock_is_exclusive_and_removed_on_drop() {
        let path = unique_lock_path();
        let lock = VoiceShortcutLock::acquire(path.clone()).expect("acquire first lock");
        assert_eq!(lock.path(), path.as_path());
        assert!(path.exists());

        assert!(VoiceShortcutLock::acquire(path.clone()).is_err());

        drop(lock);
        assert!(!path.exists());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn normalizes_gnome_bindings() {
        assert_eq!(normalize_binding("Shift+CapsLk"), "<Shift>Caps_Lock");
        assert_eq!(normalize_binding("/+F12"), "<Mod4>F12");
        assert_eq!(normalize_binding("<Super>F12"), "<Mod4>F12");
    }

    #[test]
    fn appends_custom_keybinding_path_without_duplicates() {
        let path = "/org/example/";
        assert_eq!(
            append_custom_keybinding_path("@as []", path),
            "['/org/example/']"
        );
        assert_eq!(
            append_custom_keybinding_path("['/org/old/']", path),
            "['/org/old/', '/org/example/']"
        );
        assert_eq!(
            append_custom_keybinding_path("['/org/example/']", path),
            "['/org/example/']"
        );
    }

    #[test]
    fn install_plan_wrapper_calls_coddy_overlay() {
        let plan = ShortcutInstallPlan {
            name: "Coddy Voice".to_string(),
            requested_binding: "Shift+CapsLk".to_string(),
            resolved_binding: "<Shift>Caps_Lock".to_string(),
            wrapper_path: PathBuf::from("/tmp/coddy-voice-search"),
            coddy_bin: PathBuf::from("/home/user/.local/bin/coddy"),
        };

        let wrapper = plan.wrapper_script();

        assert!(wrapper.contains("coddy voice shortcut invoked"));
        assert!(wrapper.contains("/home/user/.local/bin/coddy"));
        assert!(wrapper.contains("--speak voice --overlay"));
    }

    #[test]
    fn strips_gsettings_quotes() {
        assert_eq!(
            strip_gsettings_quotes("'/home/user/.local/bin/coddy-voice-search'".to_string()),
            "/home/user/.local/bin/coddy-voice-search"
        );
        assert_eq!(
            strip_gsettings_quotes("\"<Shift>Caps_Lock\"".to_string()),
            "<Shift>Caps_Lock"
        );
    }

    #[test]
    fn gnome_status_formats_unavailable_values() {
        let status = GnomeShortcutStatus {
            gsettings_available: false,
            custom_keybindings: None,
            binding: None,
            command: None,
            wrapper_exists: false,
        };

        let rendered = status.to_string();

        assert!(rendered.contains("gsettings_available: false"));
        assert!(rendered.contains("binding: <unavailable>"));
        assert!(rendered.contains("wrapper_exists: false"));
    }

    fn test_environment() -> ShortcutEnvironment {
        ShortcutEnvironment {
            socket_path: PathBuf::from("/run/user/1000/visionclip/daemon.sock"),
            socket_exists: true,
            display: Some(":0".to_string()),
            wayland_display: None,
            xdg_runtime_dir: Some(PathBuf::from("/run/user/1000")),
            dbus_session_bus_address: Some("unix:path=/run/user/1000/bus".to_string()),
        }
    }

    fn unique_lock_path() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        env::temp_dir().join(format!("coddy-voice-{suffix}.lock"))
    }
}
