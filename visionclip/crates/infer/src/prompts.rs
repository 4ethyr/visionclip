use visionclip_common::ipc::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPolicy {
    StrictOcr,
    StrictCode,
    TechnicalTranslatePtBr,
    TechnicalExplainShort,
    SearchQueryBuilder,
}

pub fn policy_for_action(action: &Action) -> PromptPolicy {
    match action {
        Action::CopyText => PromptPolicy::StrictOcr,
        Action::ExtractCode => PromptPolicy::StrictCode,
        Action::TranslatePtBr => PromptPolicy::TechnicalTranslatePtBr,
        Action::Explain => PromptPolicy::TechnicalExplainShort,
        Action::SearchWeb => PromptPolicy::SearchQueryBuilder,
    }
}

pub fn system_prompt(policy: PromptPolicy) -> &'static str {
    match policy {
        PromptPolicy::StrictOcr => "Extraia apenas o texto visível da captura. Nao converse. Nao explique. Preserve ordem, quebras de linha, simbolos e numeros exatamente como aparecem.",
        PromptPolicy::StrictCode => "Retorne somente o codigo ou comando visivel. Preserve indentacao, nomes, simbolos, flags e pontuacao. Nao use markdown, nao use cercas ``` e nao adicione explicacoes.",
        PromptPolicy::TechnicalTranslatePtBr => "Voce traduz texto de captura para PT-BR claro. Responda somente com a traducao final. Nao mencione OCR, tarefa, captura ou instrucoes. Preserve comandos, codigo, flags, caminhos, URLs, nomes de arquivo, APIs e identificadores literais. Ignore marcadores decorativos de markdown como #, **, >, -, _ e crases quando forem apenas formatacao.",
        PromptPolicy::TechnicalExplainShort => "Voce explica conteudo tecnico em PT-BR curto e util. Responda somente com a explicacao final, em ate 4 frases curtas. Se for codigo, explique objetivo e comportamento. Se for terminal ou log, destaque erro principal, causa provavel e proxima verificacao. Se for texto natural, documento ou UI, resuma o significado. Nao mencione OCR, tarefa, captura ou instrucoes. Nao recite simbolos decorativos.",
        PromptPolicy::SearchQueryBuilder => "Voce gera uma unica consulta curta para pesquisa web. Responda somente com a query final, em uma unica linha. Priorize produto, biblioteca, comando, arquivo, erro, mensagens de erro, sintoma e pistas de terminal ou log. Se o conteudo for texto natural, use o topico central e nomes proprios. Ignore marcadores decorativos de markdown. Nao use aspas, markdown ou comentarios.",
    }
}

pub fn user_prompt(action: &Action, source_app: Option<&str>) -> String {
    let context = source_app_context(source_app);

    match action {
        Action::CopyText => format!("{context}Extraia todo o texto desta captura."),
        Action::ExtractCode => format!(
            "{context}Transcreva o codigo, os comandos ou a configuracao desta captura e devolva somente o conteudo literal."
        ),
        Action::TranslatePtBr => format!(
            "{context}Traduza para PT-BR apenas o texto humano relevante desta captura. Preserve literais tecnicos e devolva texto simples, legivel e sem marcadores decorativos."
        ),
        Action::Explain => format!(
            "{context}Explique tecnicamente o que aparece nesta captura, destacando contexto, ponto principal e proxima acao util."
        ),
        Action::SearchWeb => format!(
            "{context}Gere a melhor consulta unica para pesquisar o problema, topico ou tarefa mostrada nesta captura."
        ),
    }
}

pub fn user_prompt_from_text(action: &Action, source_app: Option<&str>, ocr_text: &str) -> String {
    let context = source_app_context(source_app);
    let profile_hint = ocr_profile_hint(ocr_text);
    let text_block = ocr_text.trim();

    match action {
        Action::CopyText => format!(
            "{context}Tarefa: revisar o texto extraido por OCR.\nRegras:\n- responda somente com o texto final\n- nao explique\n- nao use markdown\n- corrija apenas ruido evidente de OCR sem mudar o sentido\n\nTexto OCR:\n<<<OCR\n{text_block}\nOCR>>>"
        ),
        Action::ExtractCode => format!(
            "{context}Tarefa: reconstruir codigo, comando ou configuracao a partir do OCR.\nRegras:\n- responda somente com o conteudo literal final\n- nao explique\n- nao use markdown\n- preserve simbolos, indentacao, flags e caminhos quando existirem\n\nTexto OCR:\n<<<OCR\n{text_block}\nOCR>>>"
        ),
        Action::TranslatePtBr => format!(
            "{context}{profile_hint}Traduza o texto OCR para portugues do Brasil.\nResponda somente com a traducao final.\nNao mencione OCR, captura, tarefa ou instrucao.\nPreserve siglas, nomes proprios, comandos, codigo, caminhos, URLs, APIs, flags e identificadores.\nIgnore #, **, >, -, _, crases e outros marcadores quando forem apenas formatacao.\nSe houver ruido leve de OCR, escolha a leitura mais provavel sem inventar conteudo novo.\n\nOCR:\n{text_block}"
        ),
        Action::Explain => format!(
            "{context}{profile_hint}Explique em PT-BR o conteudo do OCR.\nResponda somente com a explicacao final.\nUse no maximo 4 frases curtas.\nSe for texto natural, documento ou UI, resuma o significado.\nSe for terminal ou log, destaque erro, causa provavel e proxima verificacao.\nSe for codigo, diga objetivo, tecnologia aparente e comportamento principal.\nNao mencione OCR, captura, tarefa ou instrucao.\nNao recite simbolos decorativos como #, **, >, -, _, crases ou cercas de markdown.\n\nOCR:\n{text_block}"
        ),
        Action::SearchWeb => format!(
            "{context}{profile_hint}Gere uma unica consulta curta para pesquisa web.\nResponda somente com a consulta final, em uma unica linha.\nSe for erro tecnico, priorize produto, biblioteca, comando, arquivo e sintoma.\nSe for texto natural, documento ou pagina informativa, priorize o topico central e nomes proprios.\nRemova marcacao decorativa e mantenha apenas sintaxe que muda o significado tecnico.\nNao mencione OCR, captura, tarefa ou instrucao.\nNao use aspas ou markdown.\n\nOCR:\n{text_block}"
        ),
    }
}

fn ocr_profile_hint(ocr_text: &str) -> &'static str {
    if looks_like_terminal_or_log(ocr_text) {
        "Dica local: o texto parece terminal ou log. "
    } else if looks_like_code(ocr_text) {
        "Dica local: o texto parece codigo ou configuracao. "
    } else if looks_like_markdown_or_documentation(ocr_text) {
        "Dica local: o texto parece markdown ou documentacao. "
    } else {
        "Dica local: o texto parece texto natural, documento ou interface. "
    }
}

fn looks_like_terminal_or_log(input: &str) -> bool {
    let normalized = input.to_ascii_lowercase();
    let line_starts = [
        "$ ",
        "# ",
        "> ",
        "error:",
        "warning:",
        "traceback",
        "panic:",
        "failed",
    ];
    let contains_any = [
        "command not found",
        "no such file",
        "permission denied",
        "segmentation fault",
        "cargo ",
        "sudo ",
        "apt ",
        "dnf ",
        "systemctl ",
        "journalctl ",
        "/usr/",
        "/home/",
        "~/",
        "--help",
        "--version",
    ];

    input.lines().any(|line| {
        let trimmed = line.trim_start();
        line_starts
            .iter()
            .any(|marker| trimmed.to_ascii_lowercase().starts_with(marker))
    }) || contains_any
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn looks_like_code(input: &str) -> bool {
    let normalized = input.to_ascii_lowercase();
    let strong_markers = [
        "fn ", "let ", "const ", "class ", "struct ", "enum ", "impl ", "import ", "from ",
        "return ", "public ", "private ", "#include", "->", "=>", "</", "/>", "::",
    ];
    let symbol_score = ['{', '}', '(', ')', ';']
        .iter()
        .filter(|marker| input.contains(**marker))
        .count();

    symbol_score >= 3
        || strong_markers
            .iter()
            .any(|needle| normalized.contains(needle))
}

fn looks_like_markdown_or_documentation(input: &str) -> bool {
    let markdown_lines = input
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with('#')
                || trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("> ")
        })
        .count();

    markdown_lines >= 2 || input.contains("```") || input.contains("**") || input.contains("__")
}

fn source_app_context(source_app: Option<&str>) -> String {
    let Some(value) = source_app.filter(|value| !value.trim().is_empty()) else {
        return String::new();
    };
    let value = value.trim();
    let normalized = value.to_ascii_lowercase();

    let hint = if normalized.contains("terminal")
        || normalized.contains("ptyxis")
        || normalized.contains("konsole")
        || normalized.contains("xterm")
    {
        "A origem sugere um terminal; priorize comandos, saida, erros e causa operacional."
    } else if normalized.contains("code")
        || normalized.contains("zed")
        || normalized.contains("nvim")
        || normalized.contains("vim")
        || normalized.contains("emacs")
    {
        "A origem sugere um editor; priorize codigo, comentarios, arquivos e diagnosticos tecnicos."
    } else if normalized.contains("firefox")
        || normalized.contains("chrome")
        || normalized.contains("browser")
        || normalized.contains("web")
    {
        "A origem sugere conteudo web; priorize o topico central e ignore chrome visual da pagina."
    } else if normalized.contains("libreoffice")
        || normalized.contains("writer")
        || normalized.contains("evince")
        || normalized.contains("okular")
    {
        "A origem sugere um documento; priorize o conteudo textual e a estrutura do material."
    } else {
        ""
    };

    if hint.is_empty() {
        format!("Contexto adicional da origem da captura: {value}. ")
    } else {
        format!("Contexto adicional da origem da captura: {value}. {hint} ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use visionclip_common::ipc::Action;

    #[test]
    fn map_action_to_policy() {
        assert_eq!(
            policy_for_action(&Action::CopyText),
            PromptPolicy::StrictOcr
        );
        assert_eq!(
            policy_for_action(&Action::Explain),
            PromptPolicy::TechnicalExplainShort
        );
    }

    #[test]
    fn search_prompt_mentions_terminal_and_error_context() {
        let prompt = system_prompt(PromptPolicy::SearchQueryBuilder);
        assert!(prompt.contains("terminal"));
        assert!(prompt.contains("mensagens de erro"));
    }

    #[test]
    fn translate_prompt_mentions_markdown_cleanup() {
        let prompt = system_prompt(PromptPolicy::TechnicalTranslatePtBr);
        assert!(prompt.contains("markdown"));
        assert!(prompt.contains("marcadores"));
    }

    #[test]
    fn user_prompt_includes_source_app_context_when_available() {
        let prompt = user_prompt(&Action::Explain, Some("org.gnome.Terminal"));
        assert!(prompt.contains("org.gnome.Terminal"));
    }

    #[test]
    fn text_prompt_mentions_ocr_and_terminal_context() {
        let prompt = user_prompt_from_text(
            &Action::Explain,
            Some("org.gnome.Terminal"),
            "error: daemon.sock not found",
        );
        assert!(prompt.contains("OCR"));
        assert!(prompt.contains("terminal"));
        assert!(prompt.contains("daemon.sock"));
    }

    #[test]
    fn natural_text_prompt_adds_natural_language_hint() {
        let prompt = user_prompt_from_text(
            &Action::Explain,
            None,
            "17の外国語で放送しています。短波、FM、中波や衛星ラジオによる送信。",
        );
        assert!(prompt.contains("texto natural"));
    }

    #[test]
    fn code_prompt_adds_code_hint() {
        let prompt = user_prompt_from_text(
            &Action::Explain,
            None,
            "fn main() {\n    println!(\"hello\");\n}",
        );
        assert!(prompt.contains("codigo ou configuracao"));
    }
}
