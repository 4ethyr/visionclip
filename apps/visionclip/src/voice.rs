use crate::voice_overlay;
use anyhow::{Context, Result};
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    process::Command,
    time::{timeout, Duration},
};
use tracing::{info, warn};
use uuid::Uuid;
use visionclip_common::{config::VoiceConfig, Action};
use which::which;

#[derive(Debug, Clone)]
pub struct VoiceRequest {
    pub transcript: String,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub struct VoiceSearch {
    pub transcript: String,
    pub query: String,
}

#[derive(Debug, Clone)]
pub enum VoiceAgentCommand {
    OpenApplication {
        transcript: String,
        app_name: String,
    },
    OpenUrl {
        transcript: String,
        label: String,
        url: String,
    },
    SearchWeb {
        transcript: String,
        query: String,
    },
}

pub async fn resolve_voice_request(
    config: &VoiceConfig,
    transcript_override: Option<&str>,
) -> Result<VoiceRequest> {
    let transcript =
        if let Some(transcript) = transcript_override.filter(|value| !value.trim().is_empty()) {
            transcript.trim().to_string()
        } else {
            capture_and_transcribe(config).await?
        };

    let action = resolve_action_from_transcript(&transcript)?;
    Ok(VoiceRequest { transcript, action })
}

pub async fn resolve_voice_search(
    config: &VoiceConfig,
    transcript_override: Option<&str>,
) -> Result<VoiceSearch> {
    let transcript =
        if let Some(transcript) = transcript_override.filter(|value| !value.trim().is_empty()) {
            transcript.trim().to_string()
        } else {
            capture_and_transcribe(config).await?
        };
    let query = resolve_search_query_from_transcript(&transcript)?;
    Ok(VoiceSearch { transcript, query })
}

pub async fn resolve_voice_agent_command(
    config: &VoiceConfig,
    transcript_override: Option<&str>,
) -> Result<VoiceAgentCommand> {
    let transcript =
        if let Some(transcript) = transcript_override.filter(|value| !value.trim().is_empty()) {
            transcript.trim().to_string()
        } else {
            capture_and_transcribe(config).await?
        };

    if let Some(target) = resolve_open_target_from_transcript(&transcript) {
        return match target {
            VoiceOpenTarget::Application(app_name) => Ok(VoiceAgentCommand::OpenApplication {
                transcript,
                app_name,
            }),
            VoiceOpenTarget::Url { label, url } => Ok(VoiceAgentCommand::OpenUrl {
                transcript,
                label,
                url,
            }),
        };
    }

    let query = resolve_search_query_from_transcript(&transcript)?;
    Ok(VoiceAgentCommand::SearchWeb { transcript, query })
}

async fn capture_and_transcribe(config: &VoiceConfig) -> Result<String> {
    if !config.enabled {
        anyhow::bail!(
            "voice input is disabled in config; enable [voice].enabled or pass --voice-transcript for testing"
        );
    }

    let wav_path = temp_voice_path("wav");
    let transcript_path = temp_voice_path("txt");

    let _overlay = start_listening_overlay(config);
    record_voice_sample(config, &wav_path).await?;
    let transcript = transcribe_voice_sample(config, &wav_path, &transcript_path).await?;

    let _ = fs::remove_file(&wav_path);
    let _ = fs::remove_file(&transcript_path);

    Ok(transcript)
}

fn start_listening_overlay(config: &VoiceConfig) -> Option<OverlayGuard> {
    if !config.overlay_enabled {
        return None;
    }

    if !voice_overlay::is_overlay_available() {
        warn!("voice overlay is enabled in config, but this build does not include the `gtk-overlay` feature");
        return None;
    }

    if env::var_os("WAYLAND_DISPLAY").is_none() && env::var_os("DISPLAY").is_none() {
        return None;
    }

    let current_exe = env::current_exe().ok()?;
    let mut child = Command::new(current_exe);
    child
        .args(overlay_cli_args(config.record_duration_ms))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    match child.spawn() {
        Ok(child) => Some(OverlayGuard { child: Some(child) }),
        Err(error) => {
            warn!(?error, "failed to spawn listening overlay");
            None
        }
    }
}

async fn record_voice_sample(config: &VoiceConfig, wav_path: &Path) -> Result<()> {
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

async fn record_with_arecord_if_available(
    config: &VoiceConfig,
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

async fn transcribe_voice_sample(
    config: &VoiceConfig,
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

fn resolve_action_from_transcript(transcript: &str) -> Result<Action> {
    let normalized = normalize_transcript(transcript);
    if normalized.is_empty() {
        anyhow::bail!("voice transcript is empty");
    }

    let patterns = [
        (Action::ExtractCode, " extraia o codigo "),
        (Action::ExtractCode, " extrair codigo "),
        (Action::ExtractCode, " copie o codigo "),
        (Action::ExtractCode, " copy code "),
        (Action::ExtractCode, " extract code "),
        (Action::ExtractCode, " extraia o comando "),
        (Action::ExtractCode, "提取代码"),
        (Action::ExtractCode, "复制代码"),
        (Action::ExtractCode, "抽出コード"),
        (Action::ExtractCode, "コードを抽出"),
        (Action::ExtractCode, "코드 추출"),
        (Action::ExtractCode, "извлеки код"),
        (Action::CopyText, " copie o texto "),
        (Action::CopyText, " copiar texto "),
        (Action::CopyText, " extraia o texto "),
        (Action::CopyText, " extract text "),
        (Action::CopyText, " read the text "),
        (Action::CopyText, "复制文本"),
        (Action::CopyText, "提取文本"),
        (Action::CopyText, "识别文字"),
        (Action::CopyText, "テキストをコピー"),
        (Action::CopyText, "텍스트 복사"),
        (Action::CopyText, "скопируй текст"),
        (Action::TranslatePtBr, " traduza "),
        (Action::TranslatePtBr, " traduzir "),
        (Action::TranslatePtBr, " traducao "),
        (Action::TranslatePtBr, " para portugues "),
        (Action::TranslatePtBr, " para portugues do brasil "),
        (Action::TranslatePtBr, " translate "),
        (Action::TranslatePtBr, " translation "),
        (Action::TranslatePtBr, " traduce "),
        (Action::TranslatePtBr, " traducir "),
        (Action::TranslatePtBr, "翻译"),
        (Action::TranslatePtBr, "翻譯"),
        (Action::TranslatePtBr, "翻訳"),
        (Action::TranslatePtBr, "번역"),
        (Action::TranslatePtBr, "переведи"),
        (Action::SearchWeb, " pesquise "),
        (Action::SearchWeb, " pesquisar "),
        (Action::SearchWeb, " busque "),
        (Action::SearchWeb, " buscar "),
        (Action::SearchWeb, " procure "),
        (Action::SearchWeb, " procurar "),
        (Action::SearchWeb, " search "),
        (Action::SearchWeb, " look up "),
        (Action::SearchWeb, " google "),
        (Action::SearchWeb, " web search "),
        (Action::SearchWeb, "搜索"),
        (Action::SearchWeb, "搜一下"),
        (Action::SearchWeb, "查询"),
        (Action::SearchWeb, "查找"),
        (Action::SearchWeb, "検索"),
        (Action::SearchWeb, "검색"),
        (Action::SearchWeb, "найди"),
        (Action::SearchWeb, "поиск"),
        (Action::Explain, " explique "),
        (Action::Explain, " explicar "),
        (Action::Explain, " explicacao "),
        (Action::Explain, " explain "),
        (Action::Explain, " explanation "),
        (Action::Explain, " summarize "),
        (Action::Explain, " summary "),
        (Action::Explain, " resuma "),
        (Action::Explain, " resumir "),
        (Action::Explain, " o que significa "),
        (Action::Explain, " what does this mean "),
        (Action::Explain, "解释"),
        (Action::Explain, "說明"),
        (Action::Explain, "说明"),
        (Action::Explain, "説明"),
        (Action::Explain, "설명"),
        (Action::Explain, "объясни"),
    ];

    let mut best: Option<(usize, Action)> = None;
    let mut matched_actions = Vec::new();

    for (action, pattern) in patterns {
        let normalized_pattern = normalize_transcript(pattern);
        if !voice_pattern_matches(&normalized, &normalized_pattern) {
            continue;
        }

        if matched_actions.iter().all(|candidate| candidate != &action) {
            matched_actions.push(action.clone());
        }

        let score = normalized_pattern.chars().count();
        match &best {
            None => best = Some((score, action.clone())),
            Some((best_score, _)) if score > *best_score => {
                best = Some((score, action.clone()));
            }
            _ => {}
        }
    }

    match best {
        Some((_, action)) if matched_actions.len() == 1 => Ok(action),
        Some(_) => anyhow::bail!(
            "voice request is ambiguous; say a clearer command such as `traduza`, `explique`, `pesquise`, `copie o texto` or `extraia o codigo`"
        ),
        None => anyhow::bail!(
            "could not map the voice request to an action; say a clearer command such as `traduza`, `explique`, `pesquise`, `copie o texto` or `extraia o codigo`"
        ),
    }
}

fn resolve_search_query_from_transcript(transcript: &str) -> Result<String> {
    let raw = transcript.trim();
    if raw.is_empty() {
        anyhow::bail!("voice transcript is empty");
    }

    let normalized = normalize_transcript(raw);
    if normalized.is_empty() {
        anyhow::bail!("voice transcript is empty");
    }

    let stripped = strip_search_prefix(raw);
    let query = if stripped.trim().is_empty() {
        if normalized_is_search_command_only(&normalized) {
            anyhow::bail!("voice search query is empty");
        }
        raw
    } else {
        stripped.as_str()
    };

    let query = query
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '.' | ',' | ';' | ':'))
        .to_string();

    if query.is_empty() {
        anyhow::bail!("voice search query is empty");
    }

    Ok(query)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VoiceOpenTarget {
    Application(String),
    Url { label: String, url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenSubjectMode {
    Explicit,
    Standalone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KnownWebsite {
    label: &'static str,
    url: &'static str,
}

fn resolve_open_target_from_transcript(transcript: &str) -> Option<VoiceOpenTarget> {
    let raw = transcript.trim();
    if raw.is_empty() {
        return None;
    }

    let normalized = normalize_transcript(raw);
    if let Some(subject) = extract_open_subject(raw, &normalized) {
        return resolve_open_subject(&subject, OpenSubjectMode::Explicit);
    }

    if is_standalone_open_candidate(&normalized) {
        return resolve_open_subject(raw, OpenSubjectMode::Standalone);
    }

    None
}

#[cfg(test)]
fn resolve_open_application_from_transcript(transcript: &str) -> Option<String> {
    match resolve_open_target_from_transcript(transcript) {
        Some(VoiceOpenTarget::Application(app_name)) => Some(app_name),
        _ => None,
    }
}

fn extract_open_subject(raw: &str, normalized: &str) -> Option<String> {
    let prefixes = [
        "请打开",
        "請打開",
        "打开",
        "打開",
        "开启",
        "開啟",
        "启动",
        "啟動",
        "por favor abra o aplicativo",
        "por favor abra a aplicacao",
        "por favor abra o programa",
        "por favor abra o site do",
        "por favor abra o site da",
        "por favor abra o site de",
        "por favor abra o site",
        "por favor abra o",
        "por favor abra a",
        "por favor abra",
        "abra o aplicativo",
        "abra a aplicacao",
        "abra o programa",
        "abra o software",
        "abra o site do",
        "abra o site da",
        "abra o site de",
        "abra o site",
        "abra a pagina",
        "abra o",
        "abra a",
        "abra",
        "abre o aplicativo",
        "abre a aplicacao",
        "abre o programa",
        "abre o site do",
        "abre o site da",
        "abre o site de",
        "abre o site",
        "abre o",
        "abre a",
        "abre",
        "abrir o",
        "abrir a",
        "abrir",
        "acesse o",
        "acesse a",
        "acesse",
        "acessa o",
        "acessa a",
        "acessa",
        "acessar o",
        "acessar a",
        "acessar",
        "abre el",
        "abre la",
        "abrir el",
        "abrir la",
        "abrir",
        "inicia el",
        "inicia la",
        "ejecuta el",
        "ejecuta la",
        "открой",
        "запусти",
        "entre no",
        "entre na",
        "entre em",
        "ir para",
        "inicie o",
        "inicie a",
        "inicie",
        "execute o",
        "execute a",
        "execute",
        "abrir aplicativo",
        "open",
        "open the",
        "launch",
        "start",
    ];

    for prefix in prefixes {
        if normalized == prefix {
            return Some(String::new());
        }
        if normalized_prefix_match(normalized, prefix) {
            let prefix_len = prefix.chars().count();
            let start = raw
                .char_indices()
                .nth(prefix_len)
                .map(|(index, _)| index)
                .unwrap_or(raw.len());
            let app_name = raw[start..]
                .trim()
                .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '.' | ',' | ';' | ':'))
                .to_string();
            return Some(app_name);
        }
    }

    None
}

fn resolve_open_subject(subject: &str, mode: OpenSubjectMode) -> Option<VoiceOpenTarget> {
    let cleaned = clean_open_subject(subject)?;
    let normalized = normalize_transcript(&cleaned);

    if let Some(website) = known_website(&normalized) {
        return Some(VoiceOpenTarget::Url {
            label: website.label.to_string(),
            url: website.url.to_string(),
        });
    }

    if let Some(application) = known_application_name(&normalized) {
        return Some(VoiceOpenTarget::Application(application.to_string()));
    }

    match mode {
        OpenSubjectMode::Explicit => Some(VoiceOpenTarget::Application(cleaned)),
        OpenSubjectMode::Standalone if is_known_standalone_application(&normalized) => {
            Some(VoiceOpenTarget::Application(cleaned))
        }
        OpenSubjectMode::Standalone => None,
    }
}

fn clean_open_subject(subject: &str) -> Option<String> {
    let mut value = subject
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '.' | ',' | ';' | ':'))
        .to_string();
    if value.is_empty() {
        return None;
    }

    for qualifier in [
        "o aplicativo",
        "a aplicacao",
        "o programa",
        "o software",
        "o site do",
        "o site da",
        "o site de",
        "site do",
        "site da",
        "site de",
        "a pagina do",
        "a pagina da",
        "a pagina de",
        "pagina do",
        "pagina da",
        "pagina de",
        "aplicativo",
        "aplicacao",
        "programa",
        "software",
        "site",
        "pagina",
        "the",
        "el",
        "la",
        "los",
        "las",
        "del",
        "de la",
        "приложение",
        "программу",
        "应用",
        "應用",
        "程序",
        "软件",
        "軟件",
        "网站",
        "網站",
        "网页",
        "網頁",
        "do",
        "da",
        "de",
        "o",
        "a",
        "os",
        "as",
    ] {
        let normalized = normalize_transcript(&value);
        if normalized == qualifier {
            return None;
        }
        if normalized
            .strip_prefix(qualifier)
            .is_some_and(|rest| rest.starts_with(' '))
        {
            let qualifier_len = qualifier.chars().count();
            let start = value
                .char_indices()
                .nth(qualifier_len)
                .map(|(index, _)| index)
                .unwrap_or(value.len());
            value = value[start..].trim_start().to_string();
            break;
        }
    }

    (!value.is_empty()).then_some(value)
}

fn is_standalone_open_candidate(normalized: &str) -> bool {
    known_website(normalized).is_some() || is_known_standalone_application(normalized)
}

fn is_known_standalone_application(normalized: &str) -> bool {
    if known_application_name(normalized).is_some() {
        return true;
    }

    let compact = compact_normalized(normalized);
    matches!(
        compact.as_str(),
        "terminal"
            | "terminalemulator"
            | "console"
            | "shell"
            | "navegador"
            | "browser"
            | "webbrowser"
            | "firefox"
            | "chrome"
            | "chromium"
            | "brave"
            | "vscode"
            | "code"
            | "visualstudiocode"
            | "burp"
            | "burpsuite"
            | "burpsuitecommunity"
            | "wireshark"
            | "antigravity"
            | "steam"
            | "configuracoes"
            | "settings"
            | "gnomesettings"
            | "ajustes"
    )
}

fn known_application_name(normalized: &str) -> Option<&'static str> {
    let compact = compact_normalized(normalized);
    let exact = match compact.as_str() {
        "terminal" | "terminalemulator" | "console" | "shell" | "终端" | "終端" | "终端机"
        | "終端機" | "命令行" | "控制台" | "терминал" => Some("terminal"),
        "vscode" | "code" | "visualstudiocode" => Some("VS Code"),
        "configuracoes" | "settings" | "gnomesettings" | "ajustes" | "设置" | "設定" => {
            Some("configurações")
        }
        _ => None,
    };
    if exact.is_some() {
        return exact;
    }

    if likely_terminal_misrecognition(&compact) {
        return Some("terminal");
    }

    None
}

fn likely_terminal_misrecognition(compact: &str) -> bool {
    if !compact.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }
    if !(6..=9).contains(&compact.len()) {
        return false;
    }
    levenshtein_distance(compact, "terminal") <= 2
}

fn known_website(normalized: &str) -> Option<KnownWebsite> {
    let compact = compact_normalized(normalized);
    let target = match compact.as_str() {
        "youtube" | "youtubecom" => KnownWebsite {
            label: "YouTube",
            url: "https://www.youtube.com/",
        },
        "油管" | "youtube中国" | "youtube中文" => KnownWebsite {
            label: "YouTube",
            url: "https://www.youtube.com/",
        },
        "youtubemusic" | "musicayoutube" => KnownWebsite {
            label: "YouTube Music",
            url: "https://music.youtube.com/",
        },
        "facebook" | "facebookcom" => KnownWebsite {
            label: "Facebook",
            url: "https://www.facebook.com/",
        },
        "linkedin" | "linkedincom" => KnownWebsite {
            label: "LinkedIn",
            url: "https://www.linkedin.com/",
        },
        "github" | "githubcom" => KnownWebsite {
            label: "GitHub",
            url: "https://github.com/",
        },
        "gitlab" | "gitlabcom" => KnownWebsite {
            label: "GitLab",
            url: "https://gitlab.com/",
        },
        "instagram" | "instagramcom" => KnownWebsite {
            label: "Instagram",
            url: "https://www.instagram.com/",
        },
        "reddit" | "redditcom" => KnownWebsite {
            label: "Reddit",
            url: "https://www.reddit.com/",
        },
        "stackoverflow" | "stackoverflowcom" => KnownWebsite {
            label: "Stack Overflow",
            url: "https://stackoverflow.com/",
        },
        "gmail" | "mailgoogle" | "googlemail" => KnownWebsite {
            label: "Gmail",
            url: "https://mail.google.com/",
        },
        "whatsapp" | "whatsappweb" => KnownWebsite {
            label: "WhatsApp Web",
            url: "https://web.whatsapp.com/",
        },
        "telegram" | "telegramweb" => KnownWebsite {
            label: "Telegram Web",
            url: "https://web.telegram.org/",
        },
        "google" | "googlecom" => KnownWebsite {
            label: "Google",
            url: "https://www.google.com/",
        },
        "谷歌" => KnownWebsite {
            label: "Google",
            url: "https://www.google.com/",
        },
        _ => return None,
    };

    Some(target)
}

fn compact_normalized(normalized: &str) -> String {
    normalized
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
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
    config: &VoiceConfig,
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

fn normalize_transcript(input: &str) -> String {
    ascii_fold(input)
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii_fold(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => output.push('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => output.push('e'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => output.push('i'),
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => output.push('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => output.push('u'),
            'ç' | 'Ç' => output.push('c'),
            other => output.extend(other.to_lowercase()),
        }
    }
    output
}

fn voice_pattern_matches(normalized: &str, normalized_pattern: &str) -> bool {
    if normalized_pattern.is_empty() {
        return false;
    }
    if contains_non_ascii(normalized_pattern) {
        return normalized.contains(normalized_pattern);
    }

    let padded = format!(" {normalized} ");
    let padded_pattern = format!(" {normalized_pattern} ");
    padded.contains(&padded_pattern)
}

fn normalized_prefix_match(normalized: &str, prefix: &str) -> bool {
    let Some(rest) = normalized.strip_prefix(prefix) else {
        return false;
    };
    rest.starts_with(' ')
        || rest.chars().next().is_some_and(|ch| !ch.is_ascii())
        || contains_non_ascii(prefix)
}

fn contains_non_ascii(value: &str) -> bool {
    !value.is_ascii()
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    let mut current = vec![0; previous.len()];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.chars().enumerate() {
            let substitution_cost = usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.chars().count()]
}

fn temp_voice_path(extension: &str) -> PathBuf {
    let base = env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir());
    base.join(format!("visionclip-voice-{}.{}", Uuid::new_v4(), extension))
}

fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        Path::new(command).exists()
    } else {
        which(command).is_ok()
    }
}

fn strip_search_prefix(input: &str) -> String {
    let trimmed = input.trim();
    let normalized = normalize_transcript(trimmed);
    let prefixes = [
        "搜索",
        "搜一下",
        "查询",
        "查找",
        "查一下",
        "pesquise por",
        "pesquise sobre",
        "pesquisar por",
        "pesquisar sobre",
        "procure por",
        "procure sobre",
        "buscar por",
        "busque por",
        "busque sobre",
        "google",
        "search for",
        "search about",
        "look up",
        "find information about",
    ];

    for prefix in prefixes {
        let prefix_len = prefix.chars().count();
        if normalized == prefix {
            return String::new();
        }
        if normalized_prefix_match(&normalized, prefix) {
            let start = trimmed
                .char_indices()
                .nth(prefix_len)
                .map(|(index, _)| index)
                .unwrap_or(trimmed.len());
            return trimmed[start..].trim_start().to_string();
        }
    }

    trimmed.to_string()
}

fn normalized_is_search_command_only(normalized: &str) -> bool {
    [
        "搜索",
        "搜一下",
        "查询",
        "查找",
        "查一下",
        "pesquise por",
        "pesquise sobre",
        "pesquisar por",
        "pesquisar sobre",
        "procure por",
        "procure sobre",
        "buscar por",
        "busque por",
        "busque sobre",
        "google",
        "search for",
        "search about",
        "look up",
        "find information about",
    ]
    .contains(&normalized)
}

pub fn overlay_cli_args(duration_ms: u64) -> Vec<OsString> {
    vec![
        OsString::from("--voice-overlay-listening"),
        OsString::from("--voice-overlay-duration-ms"),
        OsString::from(duration_ms.to_string()),
    ]
}

struct OverlayGuard {
    child: Option<tokio::process::Child>,
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
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
    fn resolves_translate_from_ptbr_voice_request() {
        let action = resolve_action_from_transcript("Traduza isso para português").unwrap();
        assert_eq!(action, Action::TranslatePtBr);
    }

    #[test]
    fn resolves_search_from_english_voice_request() {
        let action = resolve_action_from_transcript("search this error on google").unwrap();
        assert_eq!(action, Action::SearchWeb);
    }

    #[test]
    fn resolves_extract_code_from_specific_phrase() {
        let action = resolve_action_from_transcript("extraia o codigo dessa tela").unwrap();
        assert_eq!(action, Action::ExtractCode);
    }

    #[test]
    fn resolves_chinese_screen_actions() {
        assert_eq!(
            resolve_action_from_transcript("翻译这个屏幕").unwrap(),
            Action::TranslatePtBr
        );
        assert_eq!(
            resolve_action_from_transcript("解释这个错误").unwrap(),
            Action::Explain
        );
        assert_eq!(
            resolve_action_from_transcript("搜索 Rust async").unwrap(),
            Action::SearchWeb
        );
    }

    #[test]
    fn reports_ambiguous_voice_request() {
        let error = resolve_action_from_transcript("traduza e explique").unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn renders_template_with_known_placeholders() {
        let config = VoiceConfig {
            enabled: true,
            backend: "auto".into(),
            target: String::new(),
            overlay_enabled: true,
            shortcut: "<Super>F12".into(),
            record_duration_ms: 4_000,
            sample_rate_hz: 16_000,
            channels: 1,
            record_command: String::new(),
            transcribe_command: String::new(),
            transcribe_timeout_ms: 60_000,
        };
        let rendered = render_template(
            "tool --input {wav_path} --out {transcript_path} --rate {sample_rate_hz}",
            Path::new("/tmp/in.wav"),
            Some(Path::new("/tmp/out.txt")),
            &config,
        );

        assert!(rendered.contains("/tmp/in.wav"));
        assert!(rendered.contains("/tmp/out.txt"));
        assert!(rendered.contains("16000"));
    }

    #[test]
    fn strips_search_prefix_from_voice_transcript() {
        let query =
            resolve_search_query_from_transcript("Pesquise por clima em Sao Paulo hoje").unwrap();
        assert_eq!(query, "clima em Sao Paulo hoje");
    }

    #[test]
    fn strips_chinese_search_prefix_from_voice_transcript() {
        let query = resolve_search_query_from_transcript("搜索Rust async 教程").unwrap();
        assert_eq!(query, "Rust async 教程");
    }

    #[test]
    fn keeps_plain_voice_search_text_when_no_prefix_is_present() {
        let query = resolve_search_query_from_transcript("melhores cafeterias em Recife").unwrap();
        assert_eq!(query, "melhores cafeterias em Recife");
    }

    #[test]
    fn rejects_empty_search_query_after_prefix_only() {
        let error = resolve_search_query_from_transcript("pesquise por").unwrap_err();
        assert!(error.to_string().contains("empty"));
    }

    #[test]
    fn resolves_open_application_from_voice_transcript() {
        let app_name = resolve_open_application_from_transcript("Abra o VS Code").unwrap();
        assert_eq!(app_name, "VS Code");
    }

    #[test]
    fn resolves_standalone_known_app_from_voice_transcript() {
        let cases = [
            ("terminal", "terminal"),
            ("vscode", "VS Code"),
            ("configurações", "configurações"),
            ("BurpSuite", "BurpSuite"),
            ("wireshark", "wireshark"),
            ("antigravity", "antigravity"),
            ("steam", "steam"),
        ];

        for (transcript, expected_app) in cases {
            let app_name = resolve_open_application_from_transcript(transcript).unwrap();
            assert_eq!(app_name, expected_app);
        }
    }

    #[test]
    fn resolves_common_open_application_phrases() {
        let cases = [
            ("abra o navegador", "navegador"),
            ("abre o terminal", "terminal"),
            ("abra o terminau", "terminal"),
            ("abra o termnal", "terminal"),
            ("open the terminal", "terminal"),
            ("abra o vscode", "VS Code"),
            ("abra o BurpSuite", "BurpSuite"),
            ("abra o wireshark", "wireshark"),
            ("abra antigravity", "antigravity"),
            ("abra as configurações", "configurações"),
            ("abra a steam", "steam"),
        ];

        for (transcript, expected_app) in cases {
            let app_name = resolve_open_application_from_transcript(transcript).unwrap();
            assert_eq!(app_name, expected_app);
        }
    }

    #[test]
    fn resolves_chinese_open_application_phrases() {
        let cases = [
            ("打开终端", "terminal"),
            ("请打开终端", "terminal"),
            ("启动命令行", "terminal"),
            ("打开 VS Code", "VS Code"),
        ];

        for (transcript, expected_app) in cases {
            let app_name = resolve_open_application_from_transcript(transcript).unwrap();
            assert_eq!(app_name, expected_app);
        }
    }

    #[test]
    fn preserves_non_latin_scripts_during_normalization() {
        assert_eq!(normalize_transcript("打开终端!"), "打开终端");
        assert_eq!(normalize_transcript("ОТКРОЙ терминал"), "открой терминал");
    }

    #[test]
    fn does_not_treat_general_questions_as_open_application() {
        assert!(resolve_open_application_from_transcript("Quem foi Rousseau?").is_none());
        assert!(resolve_open_application_from_transcript("O que é JavaScript?").is_none());
        assert!(resolve_open_application_from_transcript("pesquise youtube").is_none());
    }

    #[test]
    fn resolves_known_website_from_voice_transcript() {
        let cases = [
            ("youtube", "YouTube", "https://www.youtube.com/"),
            (
                "abra o site do LinkedIn",
                "LinkedIn",
                "https://www.linkedin.com/",
            ),
            ("facebook.com", "Facebook", "https://www.facebook.com/"),
            ("打开油管", "YouTube", "https://www.youtube.com/"),
        ];

        for (transcript, expected_label, expected_url) in cases {
            let target = resolve_open_target_from_transcript(transcript).unwrap();
            assert_eq!(
                target,
                VoiceOpenTarget::Url {
                    label: expected_label.to_string(),
                    url: expected_url.to_string()
                }
            );
        }
    }

    #[tokio::test]
    async fn voice_agent_prefers_open_application_intent() {
        let config = VoiceConfig {
            enabled: false,
            backend: "auto".into(),
            target: String::new(),
            overlay_enabled: true,
            shortcut: "<Super>F12".into(),
            record_duration_ms: 4_000,
            sample_rate_hz: 16_000,
            channels: 1,
            record_command: String::new(),
            transcribe_command: String::new(),
            transcribe_timeout_ms: 60_000,
        };

        let command = resolve_voice_agent_command(&config, Some("Abra o terminal"))
            .await
            .unwrap();
        match command {
            VoiceAgentCommand::OpenApplication { app_name, .. } => {
                assert_eq!(app_name, "terminal");
            }
            other => panic!("unexpected voice agent command: {other:?}"),
        }
    }

    #[tokio::test]
    async fn voice_agent_prefers_open_url_intent_for_known_sites() {
        let config = VoiceConfig {
            enabled: false,
            backend: "auto".into(),
            target: String::new(),
            overlay_enabled: true,
            shortcut: "<Super>F12".into(),
            record_duration_ms: 4_000,
            sample_rate_hz: 16_000,
            channels: 1,
            record_command: String::new(),
            transcribe_command: String::new(),
            transcribe_timeout_ms: 60_000,
        };

        let command = resolve_voice_agent_command(&config, Some("youtube"))
            .await
            .unwrap();
        match command {
            VoiceAgentCommand::OpenUrl { label, url, .. } => {
                assert_eq!(label, "YouTube");
                assert_eq!(url, "https://www.youtube.com/");
            }
            other => panic!("unexpected voice agent command: {other:?}"),
        }
    }
}
