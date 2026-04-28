use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VoiceTurnIntent {
    OpenApplication {
        transcript: String,
        app_name: String,
    },
    OpenWebsite {
        transcript: String,
        label: String,
        url: String,
    },
    SearchWeb {
        transcript: String,
        query: String,
    },
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenTarget {
    Application(String),
    Website { label: String, url: String },
}

pub fn resolve_voice_turn_intent(transcript: &str) -> Option<VoiceTurnIntent> {
    let transcript = transcript.trim();
    if transcript.is_empty() {
        return None;
    }

    if let Some(target) = resolve_open_target(transcript) {
        return match target {
            OpenTarget::Application(app_name) => Some(VoiceTurnIntent::OpenApplication {
                transcript: transcript.to_string(),
                app_name,
            }),
            OpenTarget::Website { label, url } => Some(VoiceTurnIntent::OpenWebsite {
                transcript: transcript.to_string(),
                label,
                url,
            }),
        };
    }

    if is_open_command_only(&normalize_transcript(transcript)) {
        return None;
    }

    let query = resolve_search_query(transcript)?;
    Some(VoiceTurnIntent::SearchWeb {
        transcript: transcript.to_string(),
        query,
    })
}

fn resolve_open_target(transcript: &str) -> Option<OpenTarget> {
    let normalized = normalize_transcript(transcript);
    if let Some(subject) = extract_open_subject(transcript, &normalized) {
        return resolve_open_subject(&subject, OpenSubjectMode::Explicit);
    }

    if is_standalone_open_candidate(&normalized) {
        return resolve_open_subject(transcript, OpenSubjectMode::Standalone);
    }

    None
}

fn extract_open_subject(raw: &str, normalized: &str) -> Option<String> {
    let prefixes = [
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
        if normalized
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(' '))
        {
            let prefix_len = prefix.chars().count();
            let start = raw
                .char_indices()
                .nth(prefix_len)
                .map(|(index, _)| index)
                .unwrap_or(raw.len());
            return Some(clean_command_subject(&raw[start..]));
        }
    }

    None
}

fn resolve_open_subject(subject: &str, mode: OpenSubjectMode) -> Option<OpenTarget> {
    let cleaned = clean_open_subject(subject)?;
    let normalized = normalize_transcript(&cleaned);

    if let Some(website) = known_website(&normalized) {
        return Some(OpenTarget::Website {
            label: website.label.to_string(),
            url: website.url.to_string(),
        });
    }

    match mode {
        OpenSubjectMode::Explicit => Some(OpenTarget::Application(cleaned)),
        OpenSubjectMode::Standalone if is_known_standalone_application(&normalized) => {
            Some(OpenTarget::Application(cleaned))
        }
        OpenSubjectMode::Standalone => None,
    }
}

fn clean_open_subject(subject: &str) -> Option<String> {
    let mut value = clean_command_subject(subject);
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

fn clean_command_subject(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '.' | ',' | ';' | ':'))
        .to_string()
}

fn is_standalone_open_candidate(normalized: &str) -> bool {
    known_website(normalized).is_some() || is_known_standalone_application(normalized)
}

fn is_open_command_only(normalized: &str) -> bool {
    matches!(
        normalized,
        "abra"
            | "abra o"
            | "abra a"
            | "abre"
            | "abre o"
            | "abre a"
            | "abrir"
            | "acesse"
            | "acessa"
            | "acessar"
            | "entre em"
            | "ir para"
            | "inicie"
            | "execute"
            | "open"
            | "open the"
            | "launch"
            | "start"
    )
}

fn is_known_standalone_application(normalized: &str) -> bool {
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

fn known_website(normalized: &str) -> Option<KnownWebsite> {
    let compact = compact_normalized(normalized);
    let target = match compact.as_str() {
        "youtube" | "youtubecom" => KnownWebsite {
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
        _ => return None,
    };

    Some(target)
}

fn resolve_search_query(transcript: &str) -> Option<String> {
    let stripped = strip_search_prefix(transcript);
    let query = if stripped.trim().is_empty() {
        let normalized = normalize_transcript(transcript);
        if normalized_is_search_command_only(&normalized) {
            return None;
        }
        transcript.trim()
    } else {
        stripped.as_str()
    };

    let query = clean_command_subject(query);
    (!query.is_empty()).then_some(query)
}

fn strip_search_prefix(input: &str) -> String {
    let trimmed = input.trim();
    let normalized = normalize_transcript(trimmed);
    let prefixes = [
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
        if normalized.starts_with(prefix) {
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

fn compact_normalized(normalized: &str) -> String {
    normalized
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn normalize_transcript(input: &str) -> String {
    ascii_fold(input)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
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
    input
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_open_application_commands() {
        let cases = [
            ("abra o navegador", "navegador"),
            ("abre o terminal", "terminal"),
            ("abra o vscode", "vscode"),
            ("abra o BurpSuite", "BurpSuite"),
            ("abra o wireshark", "wireshark"),
            ("abra antigravity", "antigravity"),
            ("abra as configurações", "configurações"),
            ("steam", "steam"),
        ];

        for (transcript, expected_app) in cases {
            let intent = resolve_voice_turn_intent(transcript).unwrap();
            assert_eq!(
                intent,
                VoiceTurnIntent::OpenApplication {
                    transcript: transcript.to_string(),
                    app_name: expected_app.to_string()
                }
            );
        }
    }

    #[test]
    fn resolves_known_websites_without_searching() {
        let intent = resolve_voice_turn_intent("linkedin").unwrap();

        assert_eq!(
            intent,
            VoiceTurnIntent::OpenWebsite {
                transcript: "linkedin".to_string(),
                label: "LinkedIn".to_string(),
                url: "https://www.linkedin.com/".to_string()
            }
        );
    }

    #[test]
    fn resolves_general_questions_as_search() {
        let intent = resolve_voice_turn_intent("Quem foi Rousseau?").unwrap();

        assert_eq!(
            intent,
            VoiceTurnIntent::SearchWeb {
                transcript: "Quem foi Rousseau?".to_string(),
                query: "Quem foi Rousseau?".to_string()
            }
        );
    }

    #[test]
    fn strips_search_prefixes() {
        let intent = resolve_voice_turn_intent("Pesquise sobre NASA").unwrap();

        assert_eq!(
            intent,
            VoiceTurnIntent::SearchWeb {
                transcript: "Pesquise sobre NASA".to_string(),
                query: "NASA".to_string()
            }
        );
    }

    #[test]
    fn rejects_empty_or_prefix_only_turns() {
        assert!(resolve_voice_turn_intent("").is_none());
        assert!(resolve_voice_turn_intent("pesquise sobre").is_none());
        assert!(resolve_voice_turn_intent("abra").is_none());
    }
}
