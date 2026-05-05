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
    #[serde(default = "default_false")]
    pub rendered_ai_overview_listener: bool,
    #[serde(default = "default_rendered_ai_overview_wait_ms")]
    pub rendered_ai_overview_wait_ms: u64,
    #[serde(default = "default_rendered_ai_overview_poll_interval_ms")]
    pub rendered_ai_overview_poll_interval_ms: u64,
    #[serde(default = "default_true")]
    pub index_on_startup: bool,
    #[serde(default = "default_true")]
    pub watch_enabled: bool,
    #[serde(default = "default_search_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_search_max_file_size_mb")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_search_max_text_bytes")]
    pub max_text_bytes: usize,
    #[serde(default = "default_search_max_workers")]
    pub max_workers: usize,
    #[serde(default = "default_true")]
    pub content_index: bool,
    #[serde(default)]
    pub semantic_index: bool,
    #[serde(default)]
    pub ocr_index: bool,
    #[serde(default = "default_search_vector_backend")]
    pub vector_backend: String,
    #[serde(default = "default_local_search_roots")]
    pub roots: Vec<String>,
    #[serde(default = "default_search_exclude_dirs")]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_search_sensitive_dirs")]
    pub exclude_sensitive_dirs: Vec<String>,
    #[serde(default = "default_search_exclude_globs")]
    pub exclude_globs: Vec<String>,
    #[serde(default)]
    pub ranking: SearchRankingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRankingConfig {
    #[serde(default = "default_true")]
    pub prefer_filename_for_short_queries: bool,
    #[serde(default = "default_true")]
    pub recency_boost: bool,
    #[serde(default = "default_true")]
    pub frecency_boost: bool,
    #[serde(default = "default_search_hybrid_fusion")]
    pub hybrid_fusion: String,
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
    #[serde(default)]
    pub wake_word_enabled: bool,
    #[serde(default = "default_true")]
    pub wake_block_during_playback: bool,
    #[serde(default)]
    pub speaker_verification_enabled: bool,
    #[serde(default = "default_speaker_verification_threshold")]
    pub speaker_verification_threshold: f32,
    #[serde(default = "default_speaker_verification_min_samples")]
    pub speaker_verification_min_samples: usize,
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
    #[serde(default = "default_wake_record_duration_ms")]
    pub wake_record_duration_ms: u64,
    #[serde(default = "default_wake_idle_sleep_ms")]
    pub wake_idle_sleep_ms: u64,
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
    #[serde(default)]
    pub search_overlay: SearchOverlayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOverlayConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_search_overlay_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_true")]
    pub liquid_glass_enabled: bool,
    /// Glass visual preset. Use one of SEARCH_OVERLAY_GLASS_STYLES.
    #[serde(default = "default_overlay_glass_style")]
    pub glass_style: String,
    #[serde(default = "default_overlay_blur_radius_px")]
    pub blur_radius_px: u16,
    #[serde(default = "default_overlay_panel_opacity")]
    pub panel_opacity: f32,
    #[serde(default = "default_overlay_corner_radius_px")]
    pub corner_radius_px: u16,
    #[serde(default = "default_overlay_border_opacity")]
    pub border_opacity: f32,
    #[serde(default = "default_overlay_shadow_intensity")]
    pub shadow_intensity: f32,
    #[serde(default = "default_overlay_highlight_intensity")]
    pub highlight_intensity: f32,
    #[serde(default = "default_overlay_saturation")]
    pub saturation: f32,
    #[serde(default = "default_overlay_contrast")]
    pub contrast: f32,
    #[serde(default = "default_overlay_brightness")]
    pub brightness: f32,
    #[serde(default = "default_overlay_refraction_strength")]
    pub refraction_strength: f32,
    #[serde(default = "default_overlay_chromatic_aberration")]
    pub chromatic_aberration: f32,
    #[serde(default = "default_overlay_liquid_noise")]
    pub liquid_noise: f32,
    #[serde(default = "default_overlay_background")]
    pub background: String,
    #[serde(default = "default_overlay_surface")]
    pub surface: String,
    #[serde(default = "default_overlay_text_primary")]
    pub text_primary: String,
    #[serde(default = "default_overlay_text_secondary")]
    pub text_secondary: String,
    #[serde(default = "default_overlay_primary")]
    pub primary: String,
    #[serde(default = "default_overlay_secondary")]
    pub secondary: String,
    #[serde(default = "default_overlay_ai_glow")]
    pub ai_glow: String,
    #[serde(default = "default_overlay_error")]
    pub error: String,
    #[serde(default = "default_true")]
    pub animations_enabled: bool,
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

    pub fn search_sqlite_path(&self) -> AppResult<PathBuf> {
        Ok(Self::data_dir()?.join("search.sqlite3"))
    }

    pub fn voice_profile_path(&self) -> AppResult<PathBuf> {
        Ok(Self::data_dir()?.join("voice-profile.json"))
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
        self.providers.validate()?;
        self.search.validate()?;
        self.voice.validate()?;
        self.ui.validate()
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
            rendered_ai_overview_listener: default_false(),
            rendered_ai_overview_wait_ms: default_rendered_ai_overview_wait_ms(),
            rendered_ai_overview_poll_interval_ms: default_rendered_ai_overview_poll_interval_ms(),
            index_on_startup: default_true(),
            watch_enabled: default_true(),
            debounce_ms: default_search_debounce_ms(),
            max_file_size_mb: default_search_max_file_size_mb(),
            max_text_bytes: default_search_max_text_bytes(),
            max_workers: default_search_max_workers(),
            content_index: default_true(),
            semantic_index: false,
            ocr_index: false,
            vector_backend: default_search_vector_backend(),
            roots: default_local_search_roots(),
            exclude_dirs: default_search_exclude_dirs(),
            exclude_sensitive_dirs: default_search_sensitive_dirs(),
            exclude_globs: default_search_exclude_globs(),
            ranking: SearchRankingConfig::default(),
        }
    }
}

impl Default for SearchRankingConfig {
    fn default() -> Self {
        Self {
            prefer_filename_for_short_queries: default_true(),
            recency_boost: default_true(),
            frecency_boost: default_true(),
            hybrid_fusion: default_search_hybrid_fusion(),
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wake_word_enabled: false,
            wake_block_during_playback: default_true(),
            speaker_verification_enabled: false,
            speaker_verification_threshold: default_speaker_verification_threshold(),
            speaker_verification_min_samples: default_speaker_verification_min_samples(),
            backend: default_voice_backend(),
            target: String::new(),
            overlay_enabled: false,
            shortcut: default_voice_shortcut(),
            record_duration_ms: default_voice_record_duration_ms(),
            wake_record_duration_ms: default_wake_record_duration_ms(),
            wake_idle_sleep_ms: default_wake_idle_sleep_ms(),
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
            search_overlay: SearchOverlayConfig::default(),
        }
    }
}

impl Default for SearchOverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            shortcut: default_search_overlay_shortcut(),
            liquid_glass_enabled: true,
            glass_style: default_overlay_glass_style(),
            blur_radius_px: default_overlay_blur_radius_px(),
            panel_opacity: default_overlay_panel_opacity(),
            corner_radius_px: default_overlay_corner_radius_px(),
            border_opacity: default_overlay_border_opacity(),
            shadow_intensity: default_overlay_shadow_intensity(),
            highlight_intensity: default_overlay_highlight_intensity(),
            saturation: default_overlay_saturation(),
            contrast: default_overlay_contrast(),
            brightness: default_overlay_brightness(),
            refraction_strength: default_overlay_refraction_strength(),
            chromatic_aberration: default_overlay_chromatic_aberration(),
            liquid_noise: default_overlay_liquid_noise(),
            background: default_overlay_background(),
            surface: default_overlay_surface(),
            text_primary: default_overlay_text_primary(),
            text_secondary: default_overlay_text_secondary(),
            primary: default_overlay_primary(),
            secondary: default_overlay_secondary(),
            ai_glow: default_overlay_ai_glow(),
            error: default_overlay_error(),
            animations_enabled: true,
        }
    }
}

impl SearchConfig {
    pub fn validate(&self) -> AppResult<()> {
        if self.max_workers == 0 {
            return Err(AppError::Config(
                "search.max_workers must be at least 1".into(),
            ));
        }
        if self.max_file_size_mb == 0 {
            return Err(AppError::Config(
                "search.max_file_size_mb must be at least 1".into(),
            ));
        }
        if self.max_text_bytes == 0 {
            return Err(AppError::Config(
                "search.max_text_bytes must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

impl VoiceConfig {
    pub fn validate(&self) -> AppResult<()> {
        if !(0.50..=0.99).contains(&self.speaker_verification_threshold) {
            return Err(AppError::Config(
                "voice.speaker_verification_threshold must be between 0.50 and 0.99".into(),
            ));
        }
        if self.speaker_verification_min_samples == 0 || self.speaker_verification_min_samples > 10
        {
            return Err(AppError::Config(
                "voice.speaker_verification_min_samples must be between 1 and 10".into(),
            ));
        }
        Ok(())
    }
}

impl UiConfig {
    pub fn validate(&self) -> AppResult<()> {
        let normalized_style =
            normalize_search_overlay_glass_style(&self.search_overlay.glass_style);
        if !is_supported_search_overlay_glass_style(&normalized_style) {
            return Err(AppError::Config(format!(
                "ui.search_overlay.glass_style must be one of: {}",
                SEARCH_OVERLAY_GLASS_STYLES.join(", ")
            )));
        }
        if self.search_overlay.blur_radius_px > 96 {
            return Err(AppError::Config(
                "ui.search_overlay.blur_radius_px must be between 0 and 96".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.search_overlay.panel_opacity) {
            return Err(AppError::Config(
                "ui.search_overlay.panel_opacity must be between 0.0 and 1.0".into(),
            ));
        }
        if !(8..=40).contains(&self.search_overlay.corner_radius_px) {
            return Err(AppError::Config(
                "ui.search_overlay.corner_radius_px must be between 8 and 40".into(),
            ));
        }
        validate_unit_float(
            "ui.search_overlay.border_opacity",
            self.search_overlay.border_opacity,
        )?;
        validate_unit_float(
            "ui.search_overlay.shadow_intensity",
            self.search_overlay.shadow_intensity,
        )?;
        validate_unit_float(
            "ui.search_overlay.highlight_intensity",
            self.search_overlay.highlight_intensity,
        )?;
        validate_unit_float(
            "ui.search_overlay.refraction_strength",
            self.search_overlay.refraction_strength,
        )?;
        validate_unit_float(
            "ui.search_overlay.chromatic_aberration",
            self.search_overlay.chromatic_aberration,
        )?;
        validate_unit_float(
            "ui.search_overlay.liquid_noise",
            self.search_overlay.liquid_noise,
        )?;
        validate_overlay_color_factor("saturation", self.search_overlay.saturation)?;
        validate_overlay_color_factor("contrast", self.search_overlay.contrast)?;
        validate_overlay_color_factor("brightness", self.search_overlay.brightness)?;
        Ok(())
    }
}

pub const SEARCH_OVERLAY_GLASS_STYLES: &[&str] = &[
    "liquid_crystal",
    "liquid_glass",
    "liquid_glass_advanced",
    "aurora_gel",
    "crystal_mist",
    "fluid_amber",
    "frost_lens",
    "ice_ripple",
    "mercury_drop",
    "molten_glass",
    "nebula_prism",
    "ocean_wave",
    "plasma_flow",
    "prisma_flow",
    "silk_veil",
    "glass",
    "glassmorphism",
    "frosted",
    "bright_overlay",
    "dark_overlay",
    "dark_glass",
    "high_contrast",
    "vibrant",
    "desaturated",
    "monochrome",
    "vintage",
    "inverted",
    "color_shifted",
    "animated_glass",
    "accessible_glass",
    "neumorphism",
    "neumorphic_pressed",
    "neumorphic_concave",
    "neumorphic_colored",
    "neumorphic_accessible",
];

pub fn normalize_search_overlay_glass_style(style: &str) -> String {
    let normalized = style.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "" => "liquid_crystal",
        "liquid" | "crystal" | "liquid_crystal_card" => "liquid_crystal",
        "liquid_glass_card" => "liquid_glass",
        "advanced_liquid_glass" | "liquid_advanced" => "liquid_glass_advanced",
        "aurora" => "aurora_gel",
        "mist" => "crystal_mist",
        "amber" => "fluid_amber",
        "frost" | "frost_leans" | "frost_lens" => "frost_lens",
        "ice" => "ice_ripple",
        "mercury" => "mercury_drop",
        "molten" | "motel_glass" => "molten_glass",
        "nebula" => "nebula_prism",
        "ocean" => "ocean_wave",
        "plasma" => "plasma_flow",
        "prisma" | "prism_flow" => "prisma_flow",
        "silk" | "silk_veil_example" => "silk_veil",
        "glass_card" => "glass",
        "glass_effect" | "glass_card_light" => "glassmorphism",
        "glass_card_dark" => "dark_glass",
        "bright" => "bright_overlay",
        "dark" => "dark_overlay",
        "contrast" => "high_contrast",
        "color_shift" | "colorshifted" => "color_shifted",
        "animated" => "animated_glass",
        "accessible" => "accessible_glass",
        "neumorphic" | "neumorphic_flat" | "neumorphic_card" => "neumorphism",
        "pressed" => "neumorphic_pressed",
        "concave" => "neumorphic_concave",
        "colored" => "neumorphic_colored",
        "neumorphic_accessibility" => "neumorphic_accessible",
        _ => normalized.as_str(),
    }
    .to_string()
}

pub fn is_supported_search_overlay_glass_style(style: &str) -> bool {
    SEARCH_OVERLAY_GLASS_STYLES.contains(&style)
}

fn validate_unit_float(field: &str, value: f32) -> AppResult<()> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(AppError::Config(format!(
            "{field} must be between 0.0 and 1.0"
        )))
    }
}

fn validate_overlay_color_factor(field: &str, value: f32) -> AppResult<()> {
    if (0.25..=2.0).contains(&value) {
        Ok(())
    } else {
        Err(AppError::Config(format!(
            "ui.search_overlay.{field} must be between 0.25 and 2.0"
        )))
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

fn default_false() -> bool {
    false
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

fn default_search_debounce_ms() -> u64 {
    800
}

fn default_search_max_file_size_mb() -> u64 {
    64
}

fn default_search_max_text_bytes() -> usize {
    4_000_000
}

fn default_search_max_workers() -> usize {
    2
}

fn default_search_vector_backend() -> String {
    "sqlite_vec".to_string()
}

fn default_local_search_roots() -> Vec<String> {
    [
        "/usr/share/applications",
        "/var/lib/flatpak/exports/share/applications",
        "~/.local/share/applications",
        "~/.local/share/flatpak/exports/share/applications",
        "~/Documents",
        "~/Downloads",
        "~/Desktop",
        "~/Pictures",
        "~/Projects",
        "~/dev",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_search_exclude_dirs() -> Vec<String> {
    [
        ".git",
        "node_modules",
        "target",
        "vendor",
        ".venv",
        "venv",
        "__pycache__",
        ".hg",
        ".svn",
        "dist",
        "build",
        ".cache",
        ".local/share/Trash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_search_sensitive_dirs() -> Vec<String> {
    [
        "~/.ssh",
        "~/.gnupg",
        "~/.local/share/keyrings",
        "~/.password-store",
        "~/.aws",
        "~/.azure",
        "~/.kube",
        "~/.docker",
        "~/.mozilla",
        "~/.config/google-chrome",
        "~/.config/chromium",
        "~/.config/BraveSoftware",
        "~/.config/Signal",
        "~/.config/discord",
        "~/.cache",
        "~/.local/share/Trash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_search_exclude_globs() -> Vec<String> {
    [
        ".env",
        ".env.*",
        "*.pem",
        "*.key",
        "*.p12",
        "*.pfx",
        "id_rsa",
        "id_ed25519",
        "*credentials*",
        "*secret*",
        "*token*",
        "*password*",
        "*.sqlite",
        "*.db",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_search_hybrid_fusion() -> String {
    "rrf".to_string()
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

fn default_wake_record_duration_ms() -> u64 {
    3_200
}

fn default_wake_idle_sleep_ms() -> u64 {
    250
}

fn default_speaker_verification_threshold() -> f32 {
    0.72
}

fn default_speaker_verification_min_samples() -> usize {
    3
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

fn default_search_overlay_shortcut() -> String {
    "<Alt>space".to_string()
}

fn default_overlay_blur_radius_px() -> u16 {
    32
}

fn default_overlay_panel_opacity() -> f32 {
    0.04
}

fn default_overlay_corner_radius_px() -> u16 {
    28
}

fn default_overlay_border_opacity() -> f32 {
    0.30
}

fn default_overlay_shadow_intensity() -> f32 {
    0.28
}

fn default_overlay_highlight_intensity() -> f32 {
    0.42
}

fn default_overlay_saturation() -> f32 {
    1.18
}

fn default_overlay_contrast() -> f32 {
    1.06
}

fn default_overlay_brightness() -> f32 {
    1.0
}

fn default_overlay_refraction_strength() -> f32 {
    0.86
}

fn default_overlay_chromatic_aberration() -> f32 {
    0.28
}

fn default_overlay_liquid_noise() -> f32 {
    0.52
}

fn default_overlay_background() -> String {
    "#16111b".to_string()
}

fn default_overlay_surface() -> String {
    "#110c15".to_string()
}

fn default_overlay_text_primary() -> String {
    "#ffffff".to_string()
}

fn default_overlay_text_secondary() -> String {
    "#e0e6ed".to_string()
}

fn default_overlay_primary() -> String {
    "#3b82f6".to_string()
}

fn default_overlay_secondary() -> String {
    "#0053db".to_string()
}

fn default_overlay_ai_glow() -> String {
    "#2fd9f4".to_string()
}

fn default_overlay_error() -> String {
    "#ffb4ab".to_string()
}

fn default_overlay_glass_style() -> String {
    "liquid_crystal".to_string()
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
        assert!(!cfg.voice.wake_word_enabled);
        assert!(cfg.voice.wake_block_during_playback);
        assert!(!cfg.voice.speaker_verification_enabled);
        assert_eq!(cfg.voice.speaker_verification_threshold, 0.72);
        assert_eq!(cfg.voice.speaker_verification_min_samples, 3);
        assert_eq!(cfg.voice.backend, "auto");
        assert!(!cfg.voice.overlay_enabled);
        assert_eq!(cfg.voice.shortcut, "<Super>space");
        assert_eq!(cfg.voice.record_duration_ms, 4_000);
        assert_eq!(cfg.voice.wake_record_duration_ms, 3_200);
        assert_eq!(cfg.voice.wake_idle_sleep_ms, 250);
        assert!(cfg.documents.enabled);
        assert_eq!(cfg.documents.chunk_chars, 3_200);
        assert_eq!(cfg.documents.chunk_overlap_chars, 320);
        assert!(cfg.search.index_on_startup);
        assert!(cfg.search.watch_enabled);
        assert_eq!(cfg.search.debounce_ms, 800);
        assert_eq!(cfg.search.max_workers, 2);
        assert!(!cfg.search.rendered_ai_overview_listener);
        assert!(!cfg.search.semantic_index);
        assert!(!cfg.search.ocr_index);
        assert_eq!(cfg.search.vector_backend, "sqlite_vec");
        assert!(cfg.search.exclude_globs.iter().any(|glob| glob == ".env"));
        assert_eq!(cfg.ui.overlay, "panel");
        assert_eq!(cfg.ui.search_overlay.shortcut, "<Alt>space");
        assert!(cfg.ui.search_overlay.liquid_glass_enabled);
        assert_eq!(cfg.ui.search_overlay.glass_style, "liquid_crystal");
        assert_eq!(cfg.ui.search_overlay.blur_radius_px, 32);
        assert_eq!(cfg.ui.search_overlay.panel_opacity, 0.04);
        assert_eq!(cfg.ui.search_overlay.corner_radius_px, 28);
        assert_eq!(cfg.ui.search_overlay.text_primary, "#ffffff");
        assert_eq!(cfg.ui.search_overlay.text_secondary, "#e0e6ed");
        assert_eq!(cfg.ui.search_overlay.primary, "#3b82f6");
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
    fn search_config_rejects_zero_workers() {
        let mut cfg = AppConfig::default();
        cfg.search.max_workers = 0;

        let error = cfg.validate().unwrap_err();

        assert!(error.to_string().contains("search.max_workers"));
    }

    #[test]
    fn voice_config_rejects_invalid_speaker_verification_threshold() {
        let mut cfg = AppConfig::default();
        cfg.voice.speaker_verification_threshold = 0.1;

        let error = cfg.validate().unwrap_err();

        assert!(error.to_string().contains("speaker_verification_threshold"));
    }

    #[test]
    fn voice_config_caps_speaker_enrollment_samples_at_ten() {
        let mut cfg = AppConfig::default();
        cfg.voice.speaker_verification_min_samples = 11;

        let error = cfg.validate().unwrap_err();

        assert!(error
            .to_string()
            .contains("speaker_verification_min_samples"));
    }

    #[test]
    fn search_overlay_accepts_aether_style_aliases() {
        assert_eq!(
            normalize_search_overlay_glass_style("liquid-glass-advanced"),
            "liquid_glass_advanced"
        );
        assert_eq!(
            normalize_search_overlay_glass_style("glass-card-dark"),
            "dark_glass"
        );
        assert_eq!(
            normalize_search_overlay_glass_style("neumorphic-flat"),
            "neumorphism"
        );
        assert!(is_supported_search_overlay_glass_style(
            &normalize_search_overlay_glass_style("accessible-glass")
        ));
        assert_eq!(
            normalize_search_overlay_glass_style("frost-leans"),
            "frost_lens"
        );
        assert_eq!(
            normalize_search_overlay_glass_style("motel-glass"),
            "molten_glass"
        );
        assert!(is_supported_search_overlay_glass_style(
            &normalize_search_overlay_glass_style("ocean-wave")
        ));
    }

    #[test]
    fn ui_config_rejects_invalid_search_overlay_style() {
        let mut cfg = AppConfig::default();
        cfg.ui.search_overlay.glass_style = "unsafe_shell_glass".into();

        let error = cfg.validate().unwrap_err();

        assert!(error.to_string().contains("ui.search_overlay.glass_style"));
    }

    #[test]
    fn ui_config_rejects_invalid_liquid_tuning_values() {
        let mut cfg = AppConfig::default();
        cfg.ui.search_overlay.refraction_strength = 1.5;

        let error = cfg.validate().unwrap_err();

        assert!(error
            .to_string()
            .contains("ui.search_overlay.refraction_strength"));
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
