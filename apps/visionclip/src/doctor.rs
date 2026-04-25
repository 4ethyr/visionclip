use crate::voice_overlay;
use anyhow::Result;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use tokio::net::UnixStream;
use visionclip_common::{
    config::{AudioConfig, VoiceConfig},
    read_message, write_message, AppConfig, HealthCheckJob, JobResult, VisionRequest,
};
use which::which;

const VOICE_WRAPPER_NAME: &str = "visionclip-voice-search";
const SECONDARY_VOICE_SHORTCUT: &str = "<Super><Shift>F12";
const GNOME_MEDIA_KEYS_SERVICE: &str = "org.gnome.SettingsDaemon.MediaKeys.service";
const GNOME_MEDIA_KEYS_SCHEMA: &str =
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search/";
const GNOME_SECONDARY_MEDIA_KEYS_SCHEMA: &str =
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search-shift/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl CheckStatus {
    fn label(self) -> &'static str {
        match self {
            CheckStatus::Ok => "OK",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorCheck {
    pub name: &'static str,
    pub status: CheckStatus,
    pub message: String,
}

impl DoctorCheck {
    fn ok(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Ok,
            message: message.into(),
        }
    }

    fn warn(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            message: message.into(),
        }
    }

    fn fail(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            message: message.into(),
        }
    }
}

pub(crate) async fn run(config: &AppConfig) -> Result<bool> {
    let mut checks = Vec::new();
    checks.push(check_config_path()?);
    checks.push(check_daemon_socket(config).await);
    checks.push(check_overlay_runtime());
    checks.push(check_voice_recorder(&config.voice, command_available));
    checks.push(check_voice_transcriber(&config.voice, command_available));
    checks.push(check_tts_player(&config.audio, command_available));
    checks.push(check_voice_wrapper());
    checks.push(check_media_keys_service());
    checks.extend(check_gnome_shortcuts(&config.voice));

    println!("VisionClip doctor");
    for check in &checks {
        println!(
            "[{}] {:<18} {}",
            check.status.label(),
            check.name,
            check.message
        );
    }

    Ok(!checks
        .iter()
        .any(|check| matches!(check.status, CheckStatus::Fail)))
}

fn check_config_path() -> Result<DoctorCheck> {
    let path = AppConfig::config_path()?;
    if path.exists() {
        Ok(DoctorCheck::ok(
            "config",
            format!("arquivo carregavel em {}", path.display()),
        ))
    } else {
        Ok(DoctorCheck::warn(
            "config",
            format!(
                "arquivo ainda nao existe em {}; defaults serao usados",
                path.display()
            ),
        ))
    }
}

async fn check_daemon_socket(config: &AppConfig) -> DoctorCheck {
    let socket_path = match config.socket_path() {
        Ok(path) => path,
        Err(error) => {
            return DoctorCheck::fail("daemon", format!("socket indisponivel: {error}"));
        }
    };

    let request_id = uuid::Uuid::new_v4();
    let mut stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(error) => {
            return DoctorCheck::fail(
                "daemon",
                format!("nao conectou em {}: {error}", socket_path.display()),
            );
        }
    };

    let request = VisionRequest::HealthCheck(HealthCheckJob { request_id });
    if let Err(error) = write_message(&mut stream, &request).await {
        return DoctorCheck::fail("daemon", format!("falha ao enviar healthcheck: {error}"));
    }

    match read_message::<_, JobResult>(&mut stream).await {
        Ok(JobResult::ActionStatus {
            request_id: response_id,
            ..
        }) if response_id == request_id => DoctorCheck::ok(
            "daemon",
            format!("healthcheck OK em {}", socket_path.display()),
        ),
        Ok(_) => DoctorCheck::fail("daemon", "resposta inesperada ao healthcheck"),
        Err(error) => DoctorCheck::fail("daemon", format!("falha ao ler healthcheck: {error}")),
    }
}

fn check_overlay_runtime() -> DoctorCheck {
    if !voice_overlay::is_overlay_available() {
        return DoctorCheck::fail(
            "overlay",
            "binario sem feature gtk-overlay; recompile com --features gtk-overlay",
        );
    }

    if env::var_os("WAYLAND_DISPLAY").is_none() && env::var_os("DISPLAY").is_none() {
        return DoctorCheck::warn(
            "overlay",
            "sem WAYLAND_DISPLAY/DISPLAY; overlay so abrira dentro da sessao grafica",
        );
    }

    DoctorCheck::ok("overlay", "runtime grafico disponivel")
}

fn check_voice_recorder(voice: &VoiceConfig, command_exists: impl Fn(&str) -> bool) -> DoctorCheck {
    if !voice.enabled {
        return DoctorCheck::warn("voice", "entrada por voz desabilitada na configuracao");
    }

    if let Some(command) = first_command_token(&voice.record_command) {
        return check_command("recorder", &command, command_exists);
    }

    let backend = voice.backend.trim().to_ascii_lowercase();
    if backend == "arecord" {
        return check_command("recorder", "arecord", command_exists);
    }
    if backend == "pw-record" || backend == "pw_record" {
        return check_command("recorder", "pw-record", command_exists);
    }

    if command_exists("pw-record") {
        DoctorCheck::ok("recorder", "usando pw-record")
    } else if command_exists("arecord") {
        DoctorCheck::ok("recorder", "usando fallback arecord")
    } else {
        DoctorCheck::fail(
            "recorder",
            "instale pw-record ou arecord, ou configure record_command",
        )
    }
}

fn check_voice_transcriber(
    voice: &VoiceConfig,
    command_exists: impl Fn(&str) -> bool,
) -> DoctorCheck {
    if !voice.enabled {
        return DoctorCheck::warn("stt", "entrada por voz desabilitada na configuracao");
    }

    let Some(command) = first_command_token(&voice.transcribe_command) else {
        return DoctorCheck::fail("stt", "transcribe_command nao configurado");
    };

    check_command("stt", &command, command_exists)
}

fn check_tts_player(audio: &AudioConfig, command_exists: impl Fn(&str) -> bool) -> DoctorCheck {
    if !audio.enabled {
        return DoctorCheck::warn("tts", "audio/TTS desabilitado na configuracao");
    }

    let Some(command) = first_command_token(&audio.player_command) else {
        return DoctorCheck::fail("tts", "player_command nao configurado");
    };

    check_command("tts", &command, command_exists)
}

fn check_voice_wrapper() -> DoctorCheck {
    let Some(home) = env::var_os("HOME") else {
        return DoctorCheck::warn("shortcut", "HOME nao definido; wrapper nao verificado");
    };
    let path = PathBuf::from(home)
        .join(".local")
        .join("bin")
        .join(VOICE_WRAPPER_NAME);

    if is_executable_path(&path) {
        DoctorCheck::ok(
            "shortcut",
            format!("wrapper executavel em {}", path.display()),
        )
    } else {
        DoctorCheck::fail(
            "shortcut",
            format!(
                "wrapper ausente ou nao executavel em {}; reinstale scripts/install_gnome_voice_shortcut.sh",
                path.display()
            ),
        )
    }
}

fn check_gnome_shortcuts(voice: &VoiceConfig) -> Vec<DoctorCheck> {
    if !command_available("gsettings") {
        return vec![DoctorCheck::warn(
            "gnome-key",
            "gsettings ausente; atalho GNOME nao verificado",
        )];
    }

    let expected_binding = voice.shortcut.trim();
    let mut checks = vec![
        match gsettings_get(GNOME_MEDIA_KEYS_SCHEMA, "binding") {
            Ok(value) if strip_gsettings_quotes(&value) == expected_binding => {
                DoctorCheck::ok("gnome-key", format!("binding ativo: {expected_binding}"))
            }
            Ok(value) => DoctorCheck::warn(
                "gnome-key",
                format!(
                    "binding atual {}, esperado {}",
                    value.trim(),
                    expected_binding
                ),
            ),
            Err(error) => DoctorCheck::warn("gnome-key", format!("binding nao lido: {error}")),
        },
        match gsettings_get(GNOME_MEDIA_KEYS_SCHEMA, "command") {
            Ok(value) if strip_gsettings_quotes(&value).ends_with(VOICE_WRAPPER_NAME) => {
                DoctorCheck::ok("gnome-cmd", format!("comando ativo: {}", value.trim()))
            }
            Ok(value) => DoctorCheck::warn(
                "gnome-cmd",
                format!(
                    "comando atual nao aponta para {VOICE_WRAPPER_NAME}: {}",
                    value.trim()
                ),
            ),
            Err(error) => DoctorCheck::warn("gnome-cmd", format!("comando nao lido: {error}")),
        },
    ];

    checks.push(
        match gsettings_get(GNOME_SECONDARY_MEDIA_KEYS_SCHEMA, "binding") {
            Ok(value) if strip_gsettings_quotes(&value) == SECONDARY_VOICE_SHORTCUT => {
                DoctorCheck::ok(
                    "gnome-key2",
                    format!("fallback ativo: {SECONDARY_VOICE_SHORTCUT}"),
                )
            }
            Ok(value) => DoctorCheck::warn(
                "gnome-key2",
                format!(
                    "fallback atual {}, esperado {}",
                    value.trim(),
                    SECONDARY_VOICE_SHORTCUT
                ),
            ),
            Err(error) => DoctorCheck::warn("gnome-key2", format!("fallback nao lido: {error}")),
        },
    );

    checks.push(
        match gsettings_get(GNOME_SECONDARY_MEDIA_KEYS_SCHEMA, "command") {
            Ok(value) if strip_gsettings_quotes(&value).ends_with(VOICE_WRAPPER_NAME) => {
                DoctorCheck::ok("gnome-cmd2", format!("fallback comando: {}", value.trim()))
            }
            Ok(value) => DoctorCheck::warn(
                "gnome-cmd2",
                format!(
                    "fallback nao aponta para {VOICE_WRAPPER_NAME}: {}",
                    value.trim()
                ),
            ),
            Err(error) => {
                DoctorCheck::warn("gnome-cmd2", format!("fallback comando nao lido: {error}"))
            }
        },
    );

    checks
}

fn check_media_keys_service() -> DoctorCheck {
    if !command_available("systemctl") {
        return DoctorCheck::warn(
            "media-keys",
            "systemctl ausente; servico GNOME de atalhos nao verificado",
        );
    }

    let status = Command::new("systemctl")
        .args(["--user", "is-active", GNOME_MEDIA_KEYS_SERVICE])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());

    media_keys_status_check(status.as_deref())
}

fn media_keys_status_check(status: Option<&str>) -> DoctorCheck {
    match status {
        Some("active") => DoctorCheck::ok("media-keys", "servico GNOME de atalhos ativo"),
        Some(value) if !value.is_empty() => DoctorCheck::fail(
            "media-keys",
            format!(
                "servico GNOME de atalhos esta {value}; rode: systemctl --user start org.gnome.SettingsDaemon.MediaKeys.target"
            ),
        ),
        _ => DoctorCheck::warn("media-keys", "nao foi possivel ler o estado do servico GNOME de atalhos"),
    }
}

fn check_command(
    name: &'static str,
    command: &str,
    command_exists: impl Fn(&str) -> bool,
) -> DoctorCheck {
    if command_exists(command) {
        DoctorCheck::ok(name, format!("comando disponivel: {command}"))
    } else {
        DoctorCheck::fail(name, format!("comando nao encontrado: {command}"))
    }
}

fn command_available(command: &str) -> bool {
    if command.contains('/') {
        return is_executable_path(Path::new(command));
    }
    which(command).is_ok()
}

fn is_executable_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn gsettings_get(schema: &str, key: &str) -> Result<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn strip_gsettings_quotes(input: &str) -> String {
    input
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .to_string()
}

fn first_command_token(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut chars = trimmed.chars().peekable();
    let quote = match chars.peek() {
        Some('"') | Some('\'') => chars.next(),
        _ => None,
    };

    let mut token = String::new();
    while let Some(ch) = chars.next() {
        if Some(ch) == quote {
            break;
        }
        if quote.is_none() && ch.is_whitespace() {
            break;
        }
        if ch == '\\' {
            if let Some(next) = chars.next() {
                token.push(next);
            }
        } else {
            token.push(ch);
        }
    }

    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice_config() -> VoiceConfig {
        VoiceConfig {
            enabled: true,
            backend: "auto".into(),
            target: String::new(),
            overlay_enabled: true,
            shortcut: "<Super><Alt>F12".into(),
            record_duration_ms: 4000,
            sample_rate_hz: 16000,
            channels: 1,
            record_command: String::new(),
            transcribe_command: "/opt/visionclip/venv/bin/python transcribe.py {wav_path}".into(),
            transcribe_timeout_ms: 120000,
        }
    }

    fn audio_config() -> AudioConfig {
        AudioConfig {
            enabled: true,
            backend: "piper_http".into(),
            base_url: "http://127.0.0.1:5000".into(),
            default_voice: String::new(),
            speak_actions: vec!["SearchWeb".into()],
            player_command: "pw-play".into(),
        }
    }

    #[test]
    fn first_command_token_handles_plain_and_quoted_commands() {
        assert_eq!(
            first_command_token("/home/me/venv/bin/python script.py"),
            Some("/home/me/venv/bin/python".into())
        );
        assert_eq!(
            first_command_token("\"/path with spaces/python\" script.py"),
            Some("/path with spaces/python".into())
        );
        assert_eq!(
            first_command_token("'/path with spaces/python' script.py"),
            Some("/path with spaces/python".into())
        );
        assert_eq!(first_command_token("   "), None);
    }

    #[test]
    fn recorder_auto_accepts_available_pipewire_recorder() {
        let check = check_voice_recorder(&voice_config(), |command| command == "pw-record");
        assert_eq!(check.status, CheckStatus::Ok);
        assert!(check.message.contains("pw-record"));
    }

    #[test]
    fn recorder_auto_fails_without_native_recorder() {
        let check = check_voice_recorder(&voice_config(), |_| false);
        assert_eq!(check.status, CheckStatus::Fail);
    }

    #[test]
    fn transcriber_requires_configured_command_when_voice_is_enabled() {
        let mut voice = voice_config();
        voice.transcribe_command.clear();
        let check = check_voice_transcriber(&voice, |_| true);
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("transcribe_command"));
    }

    #[test]
    fn transcriber_checks_first_command_token() {
        let check = check_voice_transcriber(&voice_config(), |command| {
            command == "/opt/visionclip/venv/bin/python"
        });
        assert_eq!(check.status, CheckStatus::Ok);
    }

    #[test]
    fn tts_player_checks_configured_player() {
        let check = check_tts_player(&audio_config(), |command| command == "pw-play");
        assert_eq!(check.status, CheckStatus::Ok);
    }

    #[test]
    fn gsettings_quote_stripping_handles_single_quotes() {
        assert_eq!(
            strip_gsettings_quotes("'<Super><Shift>F12'"),
            "<Super><Shift>F12"
        );
    }

    #[test]
    fn media_keys_status_reports_inactive_service_as_failure() {
        let check = media_keys_status_check(Some("inactive"));
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("MediaKeys.target"));
    }

    #[test]
    fn media_keys_status_accepts_active_service() {
        let check = media_keys_status_check(Some("active"));
        assert_eq!(check.status, CheckStatus::Ok);
    }
}
