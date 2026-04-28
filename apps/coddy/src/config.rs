use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{env, ffi::OsString, fs, path::PathBuf};
use visionclip_voice_input::VoiceInputConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoddyRuntimeConfig {
    #[serde(default)]
    pub general: CoddyGeneralConfig,
    #[serde(default)]
    pub voice: VoiceInputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoddyGeneralConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl CoddyRuntimeConfig {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read Coddy config {}", path.display()))?;
        Self::from_toml(&raw)
            .with_context(|| format!("failed to parse Coddy config {}", path.display()))
    }

    pub fn from_toml(raw: &str) -> Result<Self> {
        Ok(toml::from_str(raw)?)
    }

    pub fn config_path() -> Result<PathBuf> {
        if let Some(path) = explicit_config_path_from_env() {
            return Ok(path);
        }

        let visionclip_path = project_config_path("io", "4ethyr", "visionclip")?;
        let legacy_path = project_config_path("io", "openai", "ai-snap")?;

        if visionclip_path.exists() {
            Ok(visionclip_path)
        } else if legacy_path.exists() {
            Ok(legacy_path)
        } else {
            Ok(visionclip_path)
        }
    }

    pub fn socket_path(&self) -> Result<PathBuf> {
        if let Some(path) = env::var_os("CODDY_DAEMON_SOCKET").map(PathBuf::from) {
            return Ok(path);
        }

        let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .context("XDG_RUNTIME_DIR is not set")?;
        socket_path_from_runtime_dir(runtime_dir)
    }

    pub fn log_level(&self) -> &str {
        self.general.log_level.as_str()
    }
}

impl Default for CoddyGeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
        }
    }
}

fn explicit_config_path_from_env() -> Option<PathBuf> {
    explicit_config_path(
        env::var_os("CODDY_CONFIG"),
        env::var_os("VISIONCLIP_CONFIG"),
        env::var_os("AI_SNAP_CONFIG"),
    )
}

fn explicit_config_path(
    coddy_path: Option<OsString>,
    visionclip_path: Option<OsString>,
    legacy_path: Option<OsString>,
) -> Option<PathBuf> {
    coddy_path
        .or(visionclip_path)
        .or(legacy_path)
        .map(PathBuf::from)
}

fn project_config_path(qualifier: &str, organization: &str, application: &str) -> Result<PathBuf> {
    let dirs = ProjectDirs::from(qualifier, organization, application)
        .context("failed to resolve Coddy config directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn socket_path_from_runtime_dir(runtime_dir: PathBuf) -> Result<PathBuf> {
    let socket_dir = runtime_dir.join("visionclip");
    fs::create_dir_all(&socket_dir)
        .with_context(|| format!("failed to create socket dir {}", socket_dir.display()))?;
    Ok(socket_dir.join("daemon.sock"))
}

fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_matches_daemon_socket_contract() {
        let config = CoddyRuntimeConfig::default();

        assert_eq!(config.log_level(), "info");
        assert_eq!(config.voice.backend, "auto");
        assert_eq!(config.voice.record_duration_ms, 4_000);
    }

    #[test]
    fn parses_voice_section_from_existing_visionclip_toml() {
        let config = CoddyRuntimeConfig::from_toml(
            r#"
            [general]
            log_level = "debug"

            [voice]
            enabled = true
            backend = "pw-record"
            record_duration_ms = 2500
            sample_rate_hz = 48000
            channels = 2
            transcribe_command = "whisper {wav_path}"
            "#,
        )
        .expect("parse config");

        assert_eq!(config.log_level(), "debug");
        assert!(config.voice.enabled);
        assert_eq!(config.voice.backend, "pw-record");
        assert_eq!(config.voice.record_duration_ms, 2_500);
        assert_eq!(config.voice.sample_rate_hz, 48_000);
        assert_eq!(config.voice.channels, 2);
        assert_eq!(config.voice.transcribe_command, "whisper {wav_path}");
    }

    #[test]
    fn socket_path_uses_visionclip_daemon_location() {
        let runtime_dir = unique_runtime_dir();
        let socket_path = socket_path_from_runtime_dir(runtime_dir.clone()).expect("socket path");

        assert_eq!(
            socket_path,
            runtime_dir.join("visionclip").join("daemon.sock")
        );
        assert!(runtime_dir.join("visionclip").exists());

        let _ = fs::remove_dir_all(runtime_dir);
    }

    #[test]
    fn coddy_config_env_takes_precedence_over_visionclip_config() {
        assert_eq!(
            explicit_config_path(
                Some(OsString::from("/home/demo/.config/coddy/config.toml")),
                Some(OsString::from("/home/demo/.config/visionclip/config.toml")),
                Some(OsString::from("/home/demo/.config/ai-snap/config.toml")),
            ),
            Some(PathBuf::from("/home/demo/.config/coddy/config.toml"))
        );
    }

    fn unique_runtime_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        env::temp_dir().join(format!("coddy-runtime-{suffix}"))
    }
}
