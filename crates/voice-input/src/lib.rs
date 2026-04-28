use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    process::Command,
    time::{timeout, Duration},
};
use tracing::{info, warn};
use uuid::Uuid;
use which::which;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceInputConfig {
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

impl Default for VoiceInputConfig {
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

pub async fn capture_and_transcribe(config: &VoiceInputConfig) -> Result<String> {
    if !config.enabled {
        anyhow::bail!(
            "voice input is disabled in config; enable [voice].enabled or pass a transcript for testing"
        );
    }

    let wav_path = runtime_voice_path("wav")?;
    let transcript_path = runtime_voice_path("txt")?;

    let result = async {
        record_voice_sample(config, &wav_path).await?;
        transcribe_voice_sample(config, &wav_path, &transcript_path).await
    }
    .await;

    let _ = fs::remove_file(&wav_path);
    let _ = fs::remove_file(&transcript_path);

    result
}

pub fn runtime_voice_path(extension: &str) -> Result<PathBuf> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .context("XDG_RUNTIME_DIR is required for transient voice capture files")?;
    let voice_dir = runtime_dir.join("visionclip").join("voice");
    fs::create_dir_all(&voice_dir).with_context(|| {
        format!(
            "failed to create transient voice directory {}",
            voice_dir.display()
        )
    })?;

    Ok(voice_dir.join(format!("voice-{}.{}", Uuid::new_v4(), extension)))
}

pub async fn record_voice_sample(config: &VoiceInputConfig, wav_path: &Path) -> Result<()> {
    let duration_ms = config.record_duration_ms.max(1_000);

    if !config.record_command.trim().is_empty() {
        let rendered = render_template(&config.record_command, wav_path, None, config);
        run_shell_command(
            &rendered,
            duration_ms.saturating_add(5_000),
            "voice record command",
        )
        .await?;
    } else if config.backend.eq_ignore_ascii_case("auto")
        || config.backend.eq_ignore_ascii_case("pw-record")
        || config.backend.eq_ignore_ascii_case("pw_record")
    {
        if command_exists("pw-record") {
            let args = vec![
                "--media-type".to_string(),
                "Audio".to_string(),
                "--media-category".to_string(),
                "Capture".to_string(),
                "--media-role".to_string(),
                "Communication".to_string(),
                "--rate".to_string(),
                config.sample_rate_hz.to_string(),
                "--channels".to_string(),
                config.channels.to_string(),
                "--format".to_string(),
                "s16".to_string(),
                wav_path.display().to_string(),
            ];
            let args = with_optional_target(args, &config.target);
            record_with_window("pw-record", &args, duration_ms).await?;
        } else if config.backend.eq_ignore_ascii_case("pw-record")
            || config.backend.eq_ignore_ascii_case("pw_record")
        {
            anyhow::bail!("voice backend requires `pw-record` but it is not installed");
        } else {
            record_with_arecord_if_available(config, wav_path, duration_ms).await?;
        }
    } else if config.backend.eq_ignore_ascii_case("arecord") {
        record_with_arecord_if_available(config, wav_path, duration_ms).await?;
    } else {
        anyhow::bail!("unsupported voice backend `{}`", config.backend);
    }

    let metadata = fs::metadata(wav_path)
        .with_context(|| format!("voice recorder did not produce {}", wav_path.display()))?;
    if metadata.len() == 0 {
        anyhow::bail!("voice recorder produced an empty audio file");
    }

    info!(
        path = %wav_path.display(),
        bytes = metadata.len(),
        duration_ms = config.record_duration_ms,
        "voice sample captured"
    );

    Ok(())
}

pub async fn transcribe_voice_sample(
    config: &VoiceInputConfig,
    wav_path: &Path,
    transcript_path: &Path,
) -> Result<String> {
    if config.transcribe_command.trim().is_empty() {
        anyhow::bail!(
            "voice transcription is not configured; set [voice].transcribe_command to a command that writes the transcript to stdout or {}",
            transcript_path.display()
        );
    }

    let rendered = render_template(
        &config.transcribe_command,
        wav_path,
        Some(transcript_path),
        config,
    );
    let output = run_shell_command(
        &rendered,
        config.transcribe_timeout_ms,
        "voice transcription command",
    )
    .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        info!(
            chars = stdout.chars().count(),
            "voice transcript received from stdout"
        );
        return Ok(stdout);
    }

    if transcript_path.exists() {
        let transcript = fs::read_to_string(transcript_path).with_context(|| {
            format!("failed to read transcript at {}", transcript_path.display())
        })?;
        let transcript = transcript.trim().to_string();
        if !transcript.is_empty() {
            info!(
                path = %transcript_path.display(),
                chars = transcript.chars().count(),
                "voice transcript received from file"
            );
            return Ok(transcript);
        }
    }

    anyhow::bail!(
        "voice transcription command produced no transcript on stdout and no usable transcript file"
    );
}

async fn record_with_arecord_if_available(
    config: &VoiceInputConfig,
    wav_path: &Path,
    duration_ms: u64,
) -> Result<()> {
    if !command_exists("arecord") {
        anyhow::bail!(
            "no supported native microphone recorder found; install `pw-record` or `arecord`, or configure [voice].record_command"
        );
    }

    let args = vec![
        "-q".to_string(),
        "-f".to_string(),
        "S16_LE".to_string(),
        "-r".to_string(),
        config.sample_rate_hz.to_string(),
        "-c".to_string(),
        config.channels.to_string(),
        wav_path.display().to_string(),
    ];
    record_with_window("arecord", &args, duration_ms).await
}

fn with_optional_target(mut args: Vec<String>, target: &str) -> Vec<String> {
    let target = target.trim();
    if !target.is_empty() {
        let insert_at = args.len().saturating_sub(1);
        args.insert(insert_at, "--target".to_string());
        args.insert(insert_at + 1, target.to_string());
    }
    args
}

async fn record_with_window(program: &str, args: &[String], duration_ms: u64) -> Result<()> {
    let rendered = render_command(program, args);
    let mut child = Command::new(program);
    child
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = child
        .spawn()
        .with_context(|| format!("failed to execute voice recorder `{rendered}`"))?;

    match timeout(Duration::from_millis(duration_ms), child.wait()).await {
        Ok(wait_result) => {
            let status = wait_result.with_context(|| format!("failed to wait for `{rendered}`"))?;
            if !status.success() {
                warn!(program, ?status, "voice recorder exited before timeout");
            }
        }
        Err(_) => {
            if let Some(pid) = child.id() {
                let _ = Command::new("kill")
                    .arg("-INT")
                    .arg(pid.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;
            } else {
                let _ = child.start_kill();
            }
            let _ = child.wait().await;
        }
    }

    Ok(())
}

async fn run_shell_command(
    command: &str,
    timeout_ms: u64,
    label: &str,
) -> Result<std::process::Output> {
    let mut child = Command::new("sh");
    child
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::null())
        .kill_on_drop(true);

    let output = timeout(Duration::from_millis(timeout_ms), child.output())
        .await
        .with_context(|| format!("{label} timed out after {timeout_ms} ms: `{command}`"))?
        .with_context(|| format!("failed to execute {label} `{command}`"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            anyhow::bail!("{label} failed with status {}: `{command}`", output.status);
        } else {
            anyhow::bail!("{label} failed with status {}: {}", output.status, stderr);
        }
    }

    Ok(output)
}

fn render_template(
    template: &str,
    wav_path: &Path,
    transcript_path: Option<&Path>,
    config: &VoiceInputConfig,
) -> String {
    let duration_s = config.record_duration_ms.div_ceil(1_000).to_string();

    template
        .replace("{wav_path}", &wav_path.display().to_string())
        .replace(
            "{transcript_path}",
            &transcript_path
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        )
        .replace("{duration_ms}", &config.record_duration_ms.to_string())
        .replace("{duration_s}", &duration_s)
        .replace("{sample_rate_hz}", &config.sample_rate_hz.to_string())
        .replace("{channels}", &config.channels.to_string())
}

fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        Path::new(command).exists()
    } else {
        which(command).is_ok()
    }
}

fn render_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

fn default_true() -> bool {
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_template_with_audio_placeholders() {
        let config = test_voice_config();
        let rendered = render_template(
            "tool --input {wav_path} --out {transcript_path} --seconds {duration_s} --rate {sample_rate_hz} --channels {channels}",
            Path::new("/run/user/1000/visionclip/voice/in.wav"),
            Some(Path::new("/run/user/1000/visionclip/voice/out.txt")),
            &config,
        );

        assert!(rendered.contains("/run/user/1000/visionclip/voice/in.wav"));
        assert!(rendered.contains("/run/user/1000/visionclip/voice/out.txt"));
        assert!(rendered.contains("--seconds 4"));
        assert!(rendered.contains("--rate 16000"));
        assert!(rendered.contains("--channels 1"));
    }

    #[test]
    fn optional_pipewire_target_is_inserted_before_output_path() {
        let args = with_optional_target(
            vec![
                "--rate".to_string(),
                "16000".to_string(),
                "/run/user/1000/voice.wav".to_string(),
            ],
            "alsa_input.pci-test",
        );

        assert_eq!(
            args,
            vec![
                "--rate",
                "16000",
                "--target",
                "alsa_input.pci-test",
                "/run/user/1000/voice.wav"
            ]
        );
    }

    #[test]
    fn render_command_keeps_simple_process_shape() {
        assert_eq!(render_command("pw-record", &[]), "pw-record");
        assert_eq!(
            render_command("pw-record", &["--rate".into(), "16000".into()]),
            "pw-record --rate 16000"
        );
    }

    fn test_voice_config() -> VoiceInputConfig {
        VoiceInputConfig {
            enabled: true,
            backend: "auto".into(),
            target: String::new(),
            overlay_enabled: true,
            shortcut: "Shift+CapsLk".into(),
            record_duration_ms: 4_000,
            sample_rate_hz: 16_000,
            channels: 1,
            record_command: String::new(),
            transcribe_command: String::new(),
            transcribe_timeout_ms: 60_000,
        }
    }
}
