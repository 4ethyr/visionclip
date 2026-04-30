use crate::error::{AppError, AppResult};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub infer: InferConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub voice: VoiceConfig,
    #[serde(default)]
    pub documents: DocumentsConfig,
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
    #[serde(default = "default_ollama_ocr_model")]
    pub ocr_model: String,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_thinking")]
    pub thinking_default: String,
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_search_base_url")]
    pub base_url: String,
    #[serde(default = "default_true")]
    pub fallback_enabled: bool,
    #[serde(default = "default_search_fallback_base_url")]
    pub fallback_base_url: String,
    #[serde(default = "default_search_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_search_max_results")]
    pub max_results: usize,
    #[serde(default = "default_true")]
    pub open_browser: bool,
    #[serde(default = "default_true")]
    pub rendered_ai_overview_listener: bool,
    #[serde(default = "default_rendered_ai_overview_wait_ms")]
    pub rendered_ai_overview_wait_ms: u64,
    #[serde(default = "default_rendered_ai_overview_poll_interval_ms")]
    pub rendered_ai_overview_poll_interval_ms: u64,
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
    #[serde(default = "default_tts_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_tts_playback_timeout_ms")]
    pub playback_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_voice_backend")]
    pub backend: String,
    #[serde(default)]
    pub target: String,
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
    #[serde(default = "default_voice_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_voice_record_duration_ms")]
    pub record_duration_ms: u64,
    #[serde(default = "default_voice_sample_rate_hz")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_voice_channels")]
    pub channels: u16,
    #[serde(default)]
    pub record_command: String,
    #[serde(default)]
    pub transcribe_command: String,
    #[serde(default = "default_voice_transcribe_timeout_ms")]
    pub transcribe_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_document_chunk_chars")]
    pub chunk_chars: usize,
    #[serde(default = "default_document_chunk_overlap_chars")]
    pub chunk_overlap_chars: usize,
    #[serde(default = "default_true")]
    pub cache_translations: bool,
    #[serde(default = "default_true")]
    pub cache_audio: bool,
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

    pub fn data_dir() -> AppResult<PathBuf> {
        project_data_dir("io", "4ethyr", "visionclip")
    }

    pub fn documents_store_path(&self) -> AppResult<PathBuf> {
        Ok(Self::data_dir()?.join("documents-store.json"))
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
            ocr_model: default_ollama_ocr_model(),
            keep_alive: default_keep_alive(),
            temperature: default_temperature(),
            thinking_default: default_thinking(),
            context_window_tokens: default_context_window_tokens(),
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
            request_timeout_ms: default_tts_request_timeout_ms(),
            playback_timeout_ms: default_tts_playback_timeout_ms(),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            base_url: default_search_base_url(),
            fallback_enabled: default_true(),
            fallback_base_url: default_search_fallback_base_url(),
            request_timeout_ms: default_search_request_timeout_ms(),
            max_results: default_search_max_results(),
            open_browser: default_true(),
            rendered_ai_overview_listener: default_true(),
            rendered_ai_overview_wait_ms: default_rendered_ai_overview_wait_ms(),
            rendered_ai_overview_poll_interval_ms: default_rendered_ai_overview_poll_interval_ms(),
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: default_voice_backend(),
            target: String::new(),
            overlay_enabled: default_true(),
            shortcut: default_voice_shortcut(),
            record_duration_ms: default_voice_record_duration_ms(),
            sample_rate_hz: default_voice_sample_rate_hz(),
            channels: default_voice_channels(),
            record_command: String::new(),
            transcribe_command: String::new(),
            transcribe_timeout_ms: default_voice_transcribe_timeout_ms(),
        }
    }
}

impl Default for DocumentsConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            chunk_chars: default_document_chunk_chars(),
            chunk_overlap_chars: default_document_chunk_overlap_chars(),
            cache_translations: default_true(),
            cache_audio: default_true(),
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
    30_000
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

fn default_ollama_ocr_model() -> String {
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

fn default_context_window_tokens() -> u32 {
    8192
}

fn default_search_base_url() -> String {
    "https://www.google.com/search".to_string()
}

fn default_search_fallback_base_url() -> String {
    "https://html.duckduckgo.com/html/".to_string()
}

fn default_search_request_timeout_ms() -> u64 {
    10_000
}

fn default_search_max_results() -> usize {
    3
}

fn default_rendered_ai_overview_wait_ms() -> u64 {
    12_000
}

fn default_rendered_ai_overview_poll_interval_ms() -> u64 {
    3_000
}

fn default_audio_backend() -> String {
    "piper_http".to_string()
}

fn default_voice_backend() -> String {
    "auto".to_string()
}

fn default_voice_shortcut() -> String {
    "<Super>F12".to_string()
}

fn default_voice_record_duration_ms() -> u64 {
    4_000
}

fn default_voice_sample_rate_hz() -> u32 {
    16_000
}

fn default_voice_channels() -> u16 {
    1
}

fn default_voice_transcribe_timeout_ms() -> u64 {
    60_000
}

fn default_document_chunk_chars() -> usize {
    3_200
}

fn default_document_chunk_overlap_chars() -> usize {
    320
}

fn default_piper_base_url() -> String {
    "http://127.0.0.1:5000".to_string()
}

fn default_speak_actions() -> Vec<String> {
    vec![
        "TranslatePtBr".to_string(),
        "Explain".to_string(),
        "SearchWeb".to_string(),
        "OpenApplication".to_string(),
        "OpenUrl".to_string(),
    ]
}

fn default_player_command() -> String {
    "paplay".to_string()
}

fn default_tts_request_timeout_ms() -> u64 {
    60_000
}

fn default_tts_playback_timeout_ms() -> u64 {
    120_000
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

fn project_data_dir(qualifier: &str, organization: &str, application: &str) -> AppResult<PathBuf> {
    let dirs = ProjectDirs::from(qualifier, organization, application)
        .ok_or_else(|| AppError::Config("failed to resolve data directory".into()))?;
    Ok(dirs.data_dir().to_path_buf())
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
        assert_eq!(cfg.infer.ocr_model, "gemma4:e2b");
        assert!(cfg.infer.thinking_default.is_empty());
        assert_eq!(cfg.infer.context_window_tokens, 8192);
        assert!(cfg.audio.enabled);
        assert_eq!(cfg.audio.request_timeout_ms, 60_000);
        assert_eq!(cfg.audio.playback_timeout_ms, 120_000);
        assert!(!cfg.voice.enabled);
        assert_eq!(cfg.voice.backend, "auto");
        assert!(cfg.voice.overlay_enabled);
        assert_eq!(cfg.voice.shortcut, "<Super>F12");
        assert_eq!(cfg.voice.record_duration_ms, 4_000);
        assert!(cfg.documents.enabled);
        assert_eq!(cfg.documents.chunk_chars, 3_200);
        assert_eq!(cfg.documents.chunk_overlap_chars, 320);
    }

    #[test]
    fn action_should_speak_respects_action_list() {
        let cfg = AppConfig::default();
        assert!(cfg.action_should_speak("Explain", true));
        assert!(cfg.action_should_speak("SearchWeb", true));
        assert!(cfg.action_should_speak("OpenUrl", true));
        assert!(!cfg.action_should_speak("CopyText", true));
        assert!(!cfg.action_should_speak("Explain", false));
    }

    #[test]
    fn explicit_config_path_supports_legacy_override() {
        assert_eq!(
            explicit_config_path(None, Some("/home/demo/.config/ai-snap/config.toml".into())),
            Some(PathBuf::from("/home/demo/.config/ai-snap/config.toml"))
        );
    }

    #[test]
    fn explicit_config_path_prefers_visionclip_override() {
        assert_eq!(
            explicit_config_path(
                Some("/home/demo/.config/visionclip/config.toml".into()),
                Some("/home/demo/.config/ai-snap/config.toml".into())
            ),
            Some(PathBuf::from("/home/demo/.config/visionclip/config.toml"))
        );
    }

    #[test]
    fn documents_store_path_uses_data_directory() {
        let path = AppConfig::default().documents_store_path().unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("documents-store.json")
        );
    }
}
