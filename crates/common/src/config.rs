use crate::error::{AppError, AppResult};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub infer: InferConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
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
    #[serde(default)]
    pub embedding_model: String,
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
pub struct ProvidersConfig {
    #[serde(default = "default_provider_route_mode")]
    pub route_mode: String,
    #[serde(default = "default_sensitive_provider_mode")]
    pub sensitive_data_mode: String,
    #[serde(default = "default_true")]
    pub ollama_enabled: bool,
    #[serde(default)]
    pub cloud_enabled: bool,
}

impl ProvidersConfig {
    pub fn validate(&self) -> AppResult<()> {
        validate_provider_mode("providers.route_mode", &self.route_mode)?;
        validate_provider_mode("providers.sensitive_data_mode", &self.sensitive_data_mode)?;

        if !self.ollama_enabled && !self.cloud_enabled {
            return Err(AppError::Config(
                "at least one provider family must be enabled".into(),
            ));
        }

        if self.cloud_enabled && self.sensitive_data_mode_normalized() == "cloud_allowed" {
            return Err(AppError::Config(
                "providers.sensitive_data_mode cannot be cloud_allowed".into(),
            ));
        }

        Ok(())
    }

    pub fn route_mode_normalized(&self) -> String {
        normalize_provider_mode(&self.route_mode)
    }

    pub fn sensitive_data_mode_normalized(&self) -> String {
        normalize_provider_mode(&self.sensitive_data_mode)
    }
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
    #[serde(default)]
    pub voices: HashMap<String, String>,
    #[serde(default = "default_speak_actions")]
    pub speak_actions: Vec<String>,
    #[serde(default = "default_player_command")]
    pub player_command: String,
    #[serde(default = "default_tts_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_tts_playback_timeout_ms")]
    pub playback_timeout_ms: u64,
}

impl AudioConfig {
    pub fn configured_voice_ids(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut voices = Vec::new();
        for voice in std::iter::once(&self.default_voice).chain(self.voices.values()) {
            let voice = voice.trim();
            if voice.is_empty() || !seen.insert(voice.to_string()) {
                continue;
            }
            voices.push(voice.to_string());
        }
        voices.sort();
        voices
    }

    pub fn missing_configured_voice_ids<I, S>(&self, available: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let available = available
            .into_iter()
            .map(|voice| voice.as_ref().trim().to_string())
            .collect::<HashSet<_>>();
        self.configured_voice_ids()
            .into_iter()
            .filter(|voice| !available.contains(voice))
            .collect()
    }

    pub fn voice_for_language(&self, language: &str) -> Option<String> {
        let requested = normalize_audio_language_key(language);

        if let Some(requested) = requested.as_deref() {
            for (language, voice) in &self.voices {
                let Some(configured) = normalize_audio_language_key(language) else {
                    continue;
                };
                if configured == requested {
                    let voice = voice.trim();
                    if !voice.is_empty() {
                        return Some(voice.to_string());
                    }
                }
            }
        }

        let default_voice = self.default_voice.trim();
        if default_voice.is_empty() {
            None
        } else {
            Some(default_voice.to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_voice_backend")]
    pub backend: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
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
        parsed.validate()?;
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

    pub fn documents_sqlite_path(&self) -> AppResult<PathBuf> {
        Ok(Self::data_dir()?.join("documents.sqlite3"))
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

    pub fn validate(&self) -> AppResult<()> {
        self.providers.validate()
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
            embedding_model: String::new(),
            keep_alive: default_keep_alive(),
            temperature: default_temperature(),
            thinking_default: default_thinking(),
            context_window_tokens: default_context_window_tokens(),
        }
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            route_mode: default_provider_route_mode(),
            sensitive_data_mode: default_sensitive_provider_mode(),
            ollama_enabled: default_true(),
            cloud_enabled: false,
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
            voices: HashMap::new(),
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
            overlay_enabled: false,
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

fn default_provider_route_mode() -> String {
    "local_first".to_string()
}

fn default_sensitive_provider_mode() -> String {
    "local_only".to_string()
}

fn normalize_provider_mode(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn validate_provider_mode(field: &str, value: &str) -> AppResult<()> {
    match normalize_provider_mode(value).as_str() {
        "local_only" | "local_first" | "cloud_allowed" => Ok(()),
        _ => Err(AppError::Config(format!(
            "{field} must be one of local_only, local_first, cloud_allowed"
        ))),
    }
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

fn normalize_audio_language_key(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    if normalized.is_empty() {
        return None;
    }
    let compact = normalized
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let canonical = match compact.as_str() {
        "pt"
        | "pt-br"
        | "pt-brazil"
        | "portuguese"
        | "portuguese-brazil"
        | "brazilian-portuguese" => "pt-BR",
        "en" | "en-us" | "en-gb" | "english" => "en",
        "es" | "es-es" | "es-mx" | "spanish" | "espanol" | "castellano" => "es",
        "zh" | "zh-cn" | "zh-hans" | "chinese" | "mandarin" => "zh",
        "ru" | "ru-ru" | "russian" => "ru",
        "ja" | "ja-jp" | "jp" | "japanese" => "ja",
        "ko" | "ko-kr" | "kr" | "korean" => "ko",
        "hi" | "hi-in" | "hindi" | "indian" => "hi",
        _ => return Some(compact),
    };
    Some(canonical.to_string())
}

fn default_voice_backend() -> String {
    "auto".to_string()
}

fn default_voice_shortcut() -> String {
    "<Super>space".to_string()
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
        "OpenDocument".to_string(),
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
    "panel".to_string()
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
        assert!(cfg.infer.embedding_model.is_empty());
        assert!(cfg.infer.thinking_default.is_empty());
        assert_eq!(cfg.infer.context_window_tokens, 8192);
        assert_eq!(cfg.providers.route_mode_normalized(), "local_first");
        assert_eq!(cfg.providers.sensitive_data_mode_normalized(), "local_only");
        assert!(cfg.providers.ollama_enabled);
        assert!(!cfg.providers.cloud_enabled);
        assert!(cfg.audio.enabled);
        assert_eq!(cfg.audio.request_timeout_ms, 60_000);
        assert_eq!(cfg.audio.playback_timeout_ms, 120_000);
        assert!(cfg.audio.voices.is_empty());
        assert!(!cfg.voice.enabled);
        assert_eq!(cfg.voice.backend, "auto");
        assert!(!cfg.voice.overlay_enabled);
        assert_eq!(cfg.voice.shortcut, "<Super>space");
        assert_eq!(cfg.voice.record_duration_ms, 4_000);
        assert!(cfg.documents.enabled);
        assert_eq!(cfg.documents.chunk_chars, 3_200);
        assert_eq!(cfg.documents.chunk_overlap_chars, 320);
        assert_eq!(cfg.ui.overlay, "panel");
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
    fn provider_config_rejects_invalid_route_mode() {
        let mut cfg = AppConfig::default();
        cfg.providers.route_mode = "wide_open".into();

        let error = cfg.validate().unwrap_err();

        assert!(error.to_string().contains("providers.route_mode"));
    }

    #[test]
    fn provider_config_rejects_cloud_for_sensitive_data() {
        let mut cfg = AppConfig::default();
        cfg.providers.cloud_enabled = true;
        cfg.providers.sensitive_data_mode = "cloud-allowed".into();

        let error = cfg.validate().unwrap_err();

        assert!(error
            .to_string()
            .contains("sensitive_data_mode cannot be cloud_allowed"));
    }

    #[test]
    fn audio_voice_lookup_accepts_language_aliases() {
        let mut audio = AudioConfig {
            default_voice: "pt_BR-fallback".into(),
            ..AudioConfig::default()
        };
        audio.voices.insert("pt-BR".into(), "dii_pt-BR".into());
        audio
            .voices
            .insert("english".into(), "en_US-lessac-medium".into());
        audio
            .voices
            .insert("zh_CN".into(), "zh_CN-huayan-medium".into());

        assert_eq!(
            audio.voice_for_language("Portuguese (Brazil)").as_deref(),
            Some("dii_pt-BR")
        );
        assert_eq!(
            audio.voice_for_language("en-US").as_deref(),
            Some("en_US-lessac-medium")
        );
        assert_eq!(
            audio.voice_for_language("zh").as_deref(),
            Some("zh_CN-huayan-medium")
        );
        assert_eq!(
            audio.voice_for_language("klingon").as_deref(),
            Some("pt_BR-fallback")
        );
    }

    #[test]
    fn audio_voice_inventory_is_deduplicated_and_reports_missing() {
        let mut audio = AudioConfig {
            default_voice: "dii_pt-BR".into(),
            ..AudioConfig::default()
        };
        audio.voices.insert("pt-BR".into(), "dii_pt-BR".into());
        audio
            .voices
            .insert("en".into(), "en_US-lessac-medium".into());
        audio
            .voices
            .insert("zh".into(), "zh_CN-huayan-medium".into());

        assert_eq!(
            audio.configured_voice_ids(),
            vec![
                "dii_pt-BR".to_string(),
                "en_US-lessac-medium".to_string(),
                "zh_CN-huayan-medium".to_string(),
            ]
        );
        assert_eq!(
            audio.missing_configured_voice_ids(["dii_pt-BR", "zh_CN-huayan-medium"]),
            vec!["en_US-lessac-medium".to_string()]
        );
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

    #[test]
    fn documents_sqlite_path_uses_data_directory() {
        let path = AppConfig::default().documents_sqlite_path().unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("documents.sqlite3")
        );
    }
}
