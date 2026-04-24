use visionclip_common::ipc::Action;

pub fn sanitize_output(action: &Action, input: &str) -> String {
    let trimmed = input.trim();

    match action {
        Action::ExtractCode => strip_markdown_fences(trimmed),
        Action::CopyText => trimmed.to_string(),
        Action::TranslatePtBr | Action::Explain => {
            normalize_plain_text(&strip_model_meta_preamble(trimmed))
        }
        Action::SearchWeb => sanitize_search_query(&strip_model_meta_preamble(trimmed)),
    }
}

pub fn sanitize_for_speech(action: &Action, input: &str) -> String {
    match action {
        Action::ExtractCode => normalize_plain_text(&strip_markdown_fences(input)),
        Action::CopyText => normalize_plain_text(input),
        Action::TranslatePtBr | Action::Explain | Action::SearchWeb => {
            normalize_ptbr_speech(&normalize_plain_text(input))
        }
    }
}

fn strip_markdown_fences(input: &str) -> String {
    if !(input.starts_with("```") && input.ends_with("```")) {
        return input.to_string();
    }

    let mut lines = input.lines();
    let first = lines.next().unwrap_or_default();
    let last_removed = input.strip_suffix("```").unwrap_or(input);
    let body = last_removed.lines().skip(1).collect::<Vec<_>>().join("\n");

    if first.trim() == "```" {
        body.trim().to_string()
    } else {
        body.trim().to_string()
    }
}

fn sanitize_search_query(input: &str) -> String {
    let line = input
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    let line = strip_leading_label(line);
    let cleaned = normalize_inline_markup(&line);
    cleaned
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'))
        .to_string()
}

fn normalize_plain_text(input: &str) -> String {
    let normalized_lines = input
        .lines()
        .map(normalize_line_markup)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    join_preserving_paragraphs(&normalized_lines)
}

fn strip_model_meta_preamble(input: &str) -> String {
    let mut kept = Vec::new();
    let mut dropping_preamble = true;

    for line in input.lines() {
        let trimmed = line.trim();
        if dropping_preamble && is_model_meta_line(trimmed) {
            continue;
        }
        dropping_preamble = false;
        kept.push(line);
    }

    let joined = kept.join("\n");
    strip_leading_label(joined.trim())
}

fn normalize_line_markup(line: &str) -> String {
    let mut value = line.trim();

    while let Some(stripped) = strip_heading_marker(value) {
        value = stripped;
    }

    while let Some(stripped) = strip_list_marker(value) {
        value = stripped;
    }

    normalize_inline_markup(value)
}

fn strip_heading_marker(line: &str) -> Option<&str> {
    let marker_len = line.chars().take_while(|ch| *ch == '#').count();
    if marker_len == 0 {
        return None;
    }

    let stripped = &line[marker_len..];
    if stripped.starts_with(' ') {
        Some(stripped.trim_start())
    } else {
        None
    }
}

fn strip_list_marker(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ ", "> "] {
        if let Some(stripped) = line.strip_prefix(marker) {
            return Some(stripped.trim_start());
        }
    }

    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count > 0 {
        let suffix = &line[digit_count..];
        if let Some(stripped) = suffix.strip_prefix(". ") {
            return Some(stripped.trim_start());
        }
    }

    None
}

fn normalize_inline_markup(input: &str) -> String {
    collapse_spaces(
        input
            .replace("**", "")
            .replace("__", "")
            .replace('`', "")
            .replace('\t', " ")
            .trim(),
    )
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_model_meta_line(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }

    let normalized = ascii_fold(line);
    [
        "ocr:",
        "texto ocr:",
        "texto extraido por ocr:",
        "o texto foi extraido por ocr",
        "tarefa:",
        "regras:",
        "instrucao:",
        "instrucoes:",
        "resposta:",
        "saida:",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
}

fn strip_leading_label(input: &str) -> String {
    let normalized = ascii_fold(input);
    for prefix in [
        "resposta final:",
        "resposta:",
        "saida final:",
        "saida:",
        "traducao final:",
        "traducao:",
        "explicacao final:",
        "explicacao:",
        "consulta final:",
        "consulta:",
        "query final:",
        "query:",
    ] {
        if normalized.starts_with(prefix) {
            if let Some((_, value)) = input.split_once(':') {
                return value.trim().to_string();
            }
        }
    }

    input.to_string()
}

fn join_preserving_paragraphs(lines: &[String]) -> String {
    let mut joined = Vec::new();
    let mut previous_blank = false;

    for line in lines {
        if line.is_empty() {
            if !previous_blank {
                joined.push(String::new());
            }
            previous_blank = true;
        } else {
            joined.push(line.clone());
            previous_blank = false;
        }
    }

    joined.join("\n")
}

fn normalize_ptbr_speech(input: &str) -> String {
    input
        .split_whitespace()
        .map(accent_token_if_safe)
        .collect::<Vec<_>>()
        .join(" ")
}

fn accent_token_if_safe(token: &str) -> String {
    let (leading, core, trailing) = split_token_edges(token);
    if core.is_empty() {
        return token.to_string();
    }

    if !is_safe_natural_language_word(core) {
        return token.to_string();
    }

    let lower = core.to_ascii_lowercase();
    let Some(replacement) = ptbr_replacement(&lower) else {
        return token.to_string();
    };

    let replacement = apply_case_style(core, replacement);
    format!("{leading}{replacement}{trailing}")
}

fn split_token_edges(token: &str) -> (&str, &str, &str) {
    let start = token
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_alphanumeric())
        .map(|(index, _)| index)
        .unwrap_or(token.len());
    let end = token
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_ascii_alphanumeric())
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(start);

    (&token[..start], &token[start..end], &token[end..])
}

fn is_safe_natural_language_word(core: &str) -> bool {
    !core.is_empty()
        && core.chars().all(|ch| ch.is_ascii_alphabetic())
        && !core.contains("__")
        && !core.contains("::")
}

fn ptbr_replacement(word: &str) -> Option<&'static str> {
    match word {
        "nao" => Some("não"),
        "possivel" => Some("possível"),
        "traducao" => Some("tradução"),
        "traducoes" => Some("traduções"),
        "explicacao" => Some("explicação"),
        "explicacoes" => Some("explicações"),
        "codigo" => Some("código"),
        "codigos" => Some("códigos"),
        "configuracao" => Some("configuração"),
        "configuracoes" => Some("configurações"),
        "aplicacao" => Some("aplicação"),
        "aplicacoes" => Some("aplicações"),
        "operacao" => Some("operação"),
        "operacoes" => Some("operações"),
        "informacao" => Some("informação"),
        "informacoes" => Some("informações"),
        "servico" => Some("serviço"),
        "servicos" => Some("serviços"),
        "conexao" => Some("conexão"),
        "conexoes" => Some("conexões"),
        "execucao" => Some("execução"),
        "execucoes" => Some("execuções"),
        "sintese" => Some("síntese"),
        "tambem" => Some("também"),
        "area" => Some("área"),
        "areas" => Some("áreas"),
        "util" => Some("útil"),
        "uteis" => Some("úteis"),
        "voce" => Some("você"),
        "voces" => Some("vocês"),
        _ => None,
    }
}

fn apply_case_style(original: &str, replacement: &str) -> String {
    if original.chars().all(|ch| ch.is_ascii_uppercase()) {
        replacement.to_uppercase()
    } else if is_title_case_ascii(original) {
        let mut chars = replacement.chars();
        let Some(first) = chars.next() else {
            return replacement.to_string();
        };
        format!("{}{}", first.to_uppercase(), chars.as_str())
    } else {
        replacement.to_string()
    }
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

fn is_title_case_ascii(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    first.is_ascii_uppercase() && chars.all(|ch| ch.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use visionclip_common::ipc::Action;

    #[test]
    fn remove_fences_from_code() {
        let raw = "```rust\nfn main() {}\n```";
        let cleaned = sanitize_output(&Action::ExtractCode, raw);
        assert_eq!(cleaned, "fn main() {}");
    }

    #[test]
    fn translate_output_removes_markdown_markup() {
        let raw = "# **Titulo**\n- `comando` explicado";
        let cleaned = sanitize_output(&Action::TranslatePtBr, raw);
        assert_eq!(cleaned, "Titulo\ncomando explicado");
    }

    #[test]
    fn search_query_uses_single_clean_line() {
        let raw = "**erro rust unwrap**\ntexto extra";
        let cleaned = sanitize_output(&Action::SearchWeb, raw);
        assert_eq!(cleaned, "erro rust unwrap");
    }

    #[test]
    fn speech_sanitizer_removes_bullets_and_markers() {
        let raw = "> **Falha** ao abrir `daemon.sock`";
        let cleaned = sanitize_for_speech(&Action::Explain, raw);
        assert_eq!(cleaned, "Falha ao abrir daemon.sock");
    }

    #[test]
    fn speech_sanitizer_restores_common_ptbr_accents() {
        let raw = "Nao foi possivel gerar traducao e explicacao util para esta aplicacao.";
        let cleaned = sanitize_for_speech(&Action::Explain, raw);
        assert_eq!(
            cleaned,
            "Não foi possível gerar tradução e explicação útil para esta aplicação."
        );
    }

    #[test]
    fn speech_sanitizer_keeps_technical_identifiers_intact() {
        let raw = "Nao altere daemon_sock nem HTTPServer durante a explicacao.";
        let cleaned = sanitize_for_speech(&Action::Explain, raw);
        assert_eq!(
            cleaned,
            "Não altere daemon_sock nem HTTPServer durante a explicação."
        );
    }

    #[test]
    fn sanitize_output_removes_prompt_echo_prefixes() {
        let raw = "O texto foi extraido por OCR.\nResposta final: Traducao limpa";
        let cleaned = sanitize_output(&Action::TranslatePtBr, raw);
        assert_eq!(cleaned, "Traducao limpa");
    }

    #[test]
    fn sanitize_search_query_removes_query_label_and_quotes() {
        let raw = "Query: \"visionclip portal screenshot timeout\"";
        let cleaned = sanitize_output(&Action::SearchWeb, raw);
        assert_eq!(cleaned, "visionclip portal screenshot timeout");
    }
}
