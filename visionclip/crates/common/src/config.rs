use crate::error::{AppError, AppResult};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub infer: InferConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_action")]
    pub default_action: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_true")]
    pub prefer_portal: bool,
    #[serde(default = "default_capture_timeout_ms")]
    pub capture_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferConfig {
    #[serde(default = "default_infer_backend")]
    pub backend: String,
    #[serde(default = "default_ollama_base_url")]
    pub base_url: String,
    #[serde(default = "default_ollama_model")]
    pub model: String,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_thinking")]
    pub thinking_default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_audio_backend")]
    pub backend: String,
    #[serde(default = "default_piper_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub default_voice: String,
    #[serde(default = "default_speak_actions")]
    pub speak_actions: Vec<String>,
    #[serde(default = "default_player_command")]
    pub player_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_overlay")]
    pub overlay: String,
    #[serde(default = "default_true")]
    pub show_notification: bool,
}

impl AppConfig {
    pub fn load() -> AppResult<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)?;
        let parsed: AppConfig = toml::from_str(&raw)?;
        Ok(parsed)
    }

    pub fn config_path() -> AppResult<PathBuf> {
        if let Some(path) = explicit_config_path_from_env() {
            return Ok(path);
        }

        let visionclip_path = visionclip_config_path()?;
        let legacy_path = legacy_config_path()?;

        if visionclip_path.exists() {
            Ok(visionclip_path)
        } else if legacy_path.exists() {
            Ok(legacy_path)
        } else {
            Ok(visionclip_path)
        }
    }

    pub fn ensure_default_config() -> AppResult<PathBuf> {
        if let Some(path) = explicit_config_path_from_env() {
            return ensure_config_file(path);
        }

        let path = visionclip_config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if path.exists() {
            return Ok(path);
        }

        let legacy_path = legacy_config_path()?;
        if legacy_path.exists() {
            fs::copy(&legacy_path, &path)?;
            return Ok(path);
        }

        fs::write(&path, toml::to_string_pretty(&Self::default())?)?;
        Ok(path)
    }

    pub fn socket_path(&self) -> AppResult<PathBuf> {
        let runtime_dir = env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .map_err(|_| AppError::Config("XDG_RUNTIME_DIR is not set".into()))?;
        let socket_dir = runtime_dir.join("visionclip");
        fs::create_dir_all(&socket_dir)?;
        Ok(socket_dir.join("daemon.sock"))
    }

    pub fn action_should_speak(&self, action: &str, requested: bool) -> bool {
        if !self.audio.enabled || !requested {
            return false;
        }
        self.audio
            .speak_actions
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(action))
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            capture: CaptureConfig::default(),
            infer: InferConfig::default(),
            audio: AudioConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_action: default_action(),
            log_level: default_log_level(),
        }
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            prefer_portal: default_true(),
            capture_timeout_ms: default_capture_timeout_ms(),
        }
    }
}

impl Default for InferConfig {
    fn default() -> Self {
        Self {
            backend: default_infer_backend(),
            base_url: default_ollama_base_url(),
            model: default_ollama_model(),
            keep_alive: default_keep_alive(),
            temperature: default_temperature(),
            thinking_default: default_thinking(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: default_audio_backend(),
            base_url: default_piper_base_url(),
            default_voice: String::new(),
            speak_actions: default_speak_actions(),
            player_command: default_player_command(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            overlay: default_overlay(),
            show_notification: default_true(),
        }
    }
}

fn default_action() -> String {
    "translate_ptbr".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_backend() -> String {
    "auto".to_string()
}

fn default_true() -> bool {
    true
}

fn default_capture_timeout_ms() -> u64 {
    10_000
}

fn default_infer_backend() -> String {
    "ollama".to_string()
}

fn default_ollama_base_url() -> String {
    "http://127.0.0.1:11434".to_string()
}

fn default_ollama_model() -> String {
    "gemma4:e2b".to_string()
}

fn default_keep_alive() -> String {
    "15m".to_string()
}

fn default_temperature() -> f32 {
    0.1
}

fn default_thinking() -> String {
    String::new()
}

fn default_audio_backend() -> String {
    "piper_http".to_string()
}

fn default_piper_base_url() -> String {
    "http://127.0.0.1:5000".to_string()
}

fn default_speak_actions() -> Vec<String> {
    vec![
        "TranslatePtBr".to_string(),
        "Explain".to_string(),
        "SearchWeb".to_string(),
    ]
}

fn default_player_command() -> String {
    "paplay".to_string()
}

fn default_overlay() -> String {
    "compact".to_string()
}

fn explicit_config_path_from_env() -> Option<PathBuf> {
    explicit_config_path(
        env::var("VISIONCLIP_CONFIG").ok(),
        env::var("AI_SNAP_CONFIG").ok(),
    )
}

fn explicit_config_path(
    visionclip_path: Option<String>,
    legacy_path: Option<String>,
) -> Option<PathBuf> {
    visionclip_path.or(legacy_path).map(PathBuf::from)
}

fn visionclip_config_path() -> AppResult<PathBuf> {
    project_config_path("io", "4ethyr", "visionclip")
}

fn legacy_config_path() -> AppResult<PathBuf> {
    project_config_path("io", "openai", "ai-snap")
}

fn project_config_path(
    qualifier: &str,
    organization: &str,
    application: &str,
) -> AppResult<PathBuf> {
    let dirs = ProjectDirs::from(qualifier, organization, application)
        .ok_or_else(|| AppError::Config("failed to resolve config directory".into()))?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn ensure_config_file(path: PathBuf) -> AppResult<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if !path.exists() {
        fs::write(&path, toml::to_string_pretty(&AppConfig::default())?)?;
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_config_has_expected_model() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.infer.model, "gemma4:e2b");
        assert!(cfg.infer.thinking_default.is_empty());
        assert!(cfg.audio.enabled);
    }

    #[test]
    fn action_should_speak_respects_action_list() {
        let cfg = AppConfig::default();
        assert!(cfg.action_should_speak("Explain", true));
        assert!(cfg.action_should_speak("SearchWeb", true));
        assert!(!cfg.action_should_speak("CopyText", true));
        assert!(!cfg.action_should_speak("Explain", false));
    }

    #[test]
    fn explicit_config_path_supports_legacy_override() {
        assert_eq!(
            explicit_config_path(None, Some("/tmp/legacy-ai-snap.toml".into())),
            Some(PathBuf::from("/tmp/legacy-ai-snap.toml"))
        );
    }

    #[test]
    fn explicit_config_path_prefers_visionclip_override() {
        assert_eq!(
            explicit_config_path(
                Some("/tmp/visionclip.toml".into()),
                Some("/tmp/legacy-ai-snap.toml".into())
            ),
            Some(PathBuf::from("/tmp/visionclip.toml"))
        );
    }
}
