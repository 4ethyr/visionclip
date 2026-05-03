use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssistantLanguage {
    PortugueseBrazil,
    English,
    Chinese,
    Spanish,
    Russian,
    Japanese,
    Korean,
    Hindi,
}

impl AssistantLanguage {
    pub fn prompt_label(self) -> &'static str {
        match self {
            Self::PortugueseBrazil => "Portuguese (Brazil)",
            Self::English => "English",
            Self::Chinese => "Chinese",
            Self::Spanish => "Spanish",
            Self::Russian => "Russian",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
            Self::Hindi => "Hindi",
        }
    }

    pub fn tts_language_code(self) -> &'static str {
        match self {
            Self::PortugueseBrazil => "pt-BR",
            Self::English => "en",
            Self::Chinese => "zh",
            Self::Spanish => "es",
            Self::Russian => "ru",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Hindi => "hi",
        }
    }

    pub fn is_portuguese(self) -> bool {
        matches!(self, Self::PortugueseBrazil)
    }

    pub fn detect(input: &str) -> Self {
        let mut has_han = false;
        let mut has_hiragana_or_katakana = false;
        let mut has_hangul = false;
        let mut has_devanagari = false;
        let mut has_cyrillic = false;

        for ch in input.chars() {
            let code = ch as u32;
            if (0x4E00..=0x9FFF).contains(&code) || (0x3400..=0x4DBF).contains(&code) {
                has_han = true;
            } else if (0x3040..=0x30FF).contains(&code) {
                has_hiragana_or_katakana = true;
            } else if (0xAC00..=0xD7AF).contains(&code) || (0x1100..=0x11FF).contains(&code) {
                has_hangul = true;
            } else if (0x0900..=0x097F).contains(&code) {
                has_devanagari = true;
            } else if (0x0400..=0x04FF).contains(&code) {
                has_cyrillic = true;
            }
        }

        if has_hiragana_or_katakana {
            Self::Japanese
        } else if has_hangul {
            Self::Korean
        } else if has_han {
            Self::Chinese
        } else if has_devanagari {
            Self::Hindi
        } else if has_cyrillic {
            Self::Russian
        } else {
            detect_latin_language(input)
        }
    }

    pub fn from_transcript(transcript: Option<&str>) -> Self {
        transcript
            .map(Self::detect)
            .unwrap_or(Self::PortugueseBrazil)
    }

    pub fn from_language_code(language_code: &str) -> Self {
        let normalized = language_code.trim().to_ascii_lowercase().replace('_', "-");

        if normalized == "en" || normalized.starts_with("en-") {
            Self::English
        } else if normalized == "es" || normalized.starts_with("es-") {
            Self::Spanish
        } else if normalized == "zh" || normalized.starts_with("zh-") {
            Self::Chinese
        } else if normalized == "ru" || normalized.starts_with("ru-") {
            Self::Russian
        } else if normalized == "ja" || normalized.starts_with("ja-") {
            Self::Japanese
        } else if normalized == "ko" || normalized.starts_with("ko-") {
            Self::Korean
        } else if normalized == "hi" || normalized.starts_with("hi-") {
            Self::Hindi
        } else {
            Self::PortugueseBrazil
        }
    }
}

fn detect_latin_language(input: &str) -> AssistantLanguage {
    let normalized = normalize_latin_for_language(input);
    let padded = format!(" {normalized} ");

    let portuguese_score = count_patterns(
        &padded,
        &[
            " abra ",
            " abre o ",
            " abre a ",
            " abrir o ",
            " abrir a ",
            " pesquise ",
            " pesquisar ",
            " busque ",
            " buscar ",
            " explique ",
            " traduza ",
            " traduzir ",
            " o que ",
            " quem ",
            " onde ",
            " quando ",
            " qual ",
            " quais ",
            " por que ",
            " voce ",
            " terminal ",
        ],
    );
    let english_score = count_patterns(
        &padded,
        &[
            " open ",
            " launch ",
            " start ",
            " search ",
            " translate ",
            " explain ",
            " what ",
            " who ",
            " where ",
            " when ",
            " why ",
            " how ",
            " terminal ",
            " browser ",
        ],
    );
    let mut spanish_score = count_patterns(
        &padded,
        &[
            " abre ",
            " abrir ",
            " busca ",
            " buscar ",
            " traduce ",
            " traducir ",
            " explica ",
            " que es ",
            " quien ",
            " donde ",
            " cuando ",
            " cual ",
            " cuales ",
            " por que ",
            " terminal ",
            " navegador ",
        ],
    );

    if input.contains('¿') || input.contains('¡') {
        spanish_score += 2;
    }

    if english_score > portuguese_score && english_score >= spanish_score {
        AssistantLanguage::English
    } else if spanish_score > portuguese_score && spanish_score > english_score {
        AssistantLanguage::Spanish
    } else {
        AssistantLanguage::PortugueseBrazil
    }
}

fn count_patterns(input: &str, patterns: &[&str]) -> usize {
    patterns
        .iter()
        .filter(|pattern| input.contains(**pattern))
        .count()
}

pub fn normalize_latin_for_language(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            'ñ' | 'Ñ' => 'n',
            other => other.to_ascii_lowercase(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_script_based_languages() {
        assert_eq!(
            AssistantLanguage::detect("打开终端"),
            AssistantLanguage::Chinese
        );
        assert_eq!(
            AssistantLanguage::detect("ターミナルを開いて"),
            AssistantLanguage::Japanese
        );
        assert_eq!(
            AssistantLanguage::detect("터미널 열어 줘"),
            AssistantLanguage::Korean
        );
        assert_eq!(
            AssistantLanguage::detect("открой терминал"),
            AssistantLanguage::Russian
        );
        assert_eq!(
            AssistantLanguage::detect("टर्मिनल खोलो"),
            AssistantLanguage::Hindi
        );
    }

    #[test]
    fn detects_latin_command_languages() {
        assert_eq!(
            AssistantLanguage::detect("open the terminal"),
            AssistantLanguage::English
        );
        assert_eq!(
            AssistantLanguage::detect("abra o terminal"),
            AssistantLanguage::PortugueseBrazil
        );
        assert_eq!(
            AssistantLanguage::detect("abre el terminal"),
            AssistantLanguage::Spanish
        );
        assert_eq!(
            AssistantLanguage::detect("¿que es Rust?"),
            AssistantLanguage::Spanish
        );
    }

    #[test]
    fn maps_language_codes_for_tts_and_documents() {
        assert_eq!(
            AssistantLanguage::from_language_code("pt-BR"),
            AssistantLanguage::PortugueseBrazil
        );
        assert_eq!(
            AssistantLanguage::from_language_code("en_US"),
            AssistantLanguage::English
        );
        assert_eq!(
            AssistantLanguage::from_language_code("zh-CN"),
            AssistantLanguage::Chinese
        );
        assert_eq!(
            AssistantLanguage::from_language_code("ru"),
            AssistantLanguage::Russian
        );
    }

    #[test]
    fn exposes_prompt_and_tts_metadata() {
        assert_eq!(AssistantLanguage::English.prompt_label(), "English");
        assert_eq!(AssistantLanguage::Chinese.tts_language_code(), "zh");
        assert!(AssistantLanguage::PortugueseBrazil.is_portuguese());
    }

    #[test]
    fn normalizes_latin_text_for_command_matching() {
        assert_eq!(
            normalize_latin_for_language("Abra o TERMINAL, por favor!"),
            "abra o terminal por favor"
        );
        assert_eq!(normalize_latin_for_language("¿Qué es Rust?"), "que es rust");
    }
}
