use visionclip_common::ipc::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPolicy {
    StrictOcr,
    StrictCode,
    TechnicalTranslatePtBr,
    TechnicalExplainShort,
    SearchQueryBuilder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcrContentProfile {
    Terminal,
    Code,
    Markdown,
    Dashboard,
    Natural,
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
        PromptPolicy::TechnicalExplainShort => "Voce explica conteudo tecnico no idioma solicitado, de forma curta e util. Se nenhum idioma for solicitado, use PT-BR. Responda somente com a explicacao final, em ate 4 frases curtas. Se for codigo, explique objetivo e comportamento. Se for terminal ou log, destaque erro principal, causa provavel e proxima verificacao. Se for texto natural, documento, dashboard ou UI, resuma o significado. Nao mencione OCR, tarefa, captura ou instrucoes. Nao recite simbolos decorativos.",
        PromptPolicy::SearchQueryBuilder => "Voce gera uma unica consulta curta para pesquisa web. Responda somente com a query final, em uma unica linha. Se a captura for tecnica, priorize produto, biblioteca, comando, arquivo, erro, sintoma e pistas de terminal ou log. Se a captura for geral, priorize assunto principal, pergunta do usuario, nomes proprios, pessoa, lugar, servico, organizacao, evento, data e termos centrais do tema. Preserve siglas e entidades uteis. Ignore marcadores decorativos de markdown. Nao use aspas, markdown ou comentarios.",
    }
}

pub fn user_prompt(
    action: &Action,
    source_app: Option<&str>,
    response_language: Option<&str>,
) -> String {
    let context = source_app_context(source_app);
    let response_language = response_language_instruction(action, response_language);

    match action {
        Action::CopyText => format!("{context}Extraia todo o texto desta captura."),
        Action::ExtractCode => format!(
            "{context}Transcreva o codigo, os comandos ou a configuracao desta captura e devolva somente o conteudo literal."
        ),
        Action::TranslatePtBr => format!(
            "{context}Traduza para PT-BR o texto humano relevante desta captura. Se for documento ou interface, preserve o sentido completo. Se houver codigo, log ou terminal, preserve literais tecnicos. Ignore marcadores decorativos de markdown e devolva texto simples e legivel."
        ),
        Action::Explain => format!(
            "{context}{response_language}Explique tecnicamente o que aparece nesta captura. Se for codigo, explique objetivo e comportamento. Se for terminal ou log, destaque erro e proxima acao util. Se for documento, painel ou interface, resuma significado e pontos relevantes."
        ),
        Action::SearchWeb => format!(
            "{context}Gere a melhor consulta unica para pesquisar o assunto principal desta captura. Se for conteudo tecnico, priorize erro, produto, biblioteca, nome de arquivo, comando e sintoma. Se for conteudo geral, preserve pergunta principal, topico central, nomes proprios, pessoa, lugar, servico, organizacao, evento, data ou idioma relevante. Ignore chrome visual da pagina e ruído decorativo."
        ),
    }
}

pub fn user_prompt_from_text(
    action: &Action,
    source_app: Option<&str>,
    response_language: Option<&str>,
    ocr_text: &str,
) -> String {
    let context = source_app_context(source_app);
    let response_language = response_language_instruction(action, response_language);
    let language_hint = ocr_language_hint(ocr_text);
    let profile_hint = ocr_profile_hint(ocr_text);
    let profile_guidance = ocr_profile_guidance(action, ocr_text);
    let text_block = ocr_text.trim();

    match action {
        Action::CopyText => format!(
            "{context}Tarefa: revisar o texto extraido por OCR.\nRegras:\n- responda somente com o texto final\n- nao explique\n- nao use markdown\n- corrija apenas ruido evidente de OCR sem mudar o sentido\n\nTexto OCR:\n<<<OCR\n{text_block}\nOCR>>>"
        ),
        Action::ExtractCode => format!(
            "{context}Tarefa: reconstruir codigo, comando ou configuracao a partir do OCR.\nRegras:\n- responda somente com o conteudo literal final\n- nao explique\n- nao use markdown\n- preserve simbolos, indentacao, flags e caminhos quando existirem\n\nTexto OCR:\n<<<OCR\n{text_block}\nOCR>>>"
        ),
        Action::TranslatePtBr => format!(
            "{context}{language_hint}{profile_hint}{profile_guidance}Traduza o texto OCR para portugues do Brasil.\nResponda somente com a traducao final.\nNao mencione OCR, captura, tarefa ou instrucao.\nPreserve siglas, nomes proprios, comandos, codigo, caminhos, URLs, APIs, flags e identificadores.\nIgnore #, **, >, -, _, crases e outros marcadores quando forem apenas formatacao.\nSe o texto estiver em outro idioma, entregue uma traducao natural e completa, nao apenas palavras-chave soltas.\nSe houver ruido leve de OCR, escolha a leitura mais provavel sem inventar conteudo novo.\n\nOCR:\n{text_block}"
        ),
        Action::Explain => format!(
            "{context}{language_hint}{profile_hint}{profile_guidance}{response_language}Explique o conteudo do OCR.\nResponda somente com a explicacao final.\nUse no maximo 4 frases curtas.\nSe for texto natural, documento ou UI, resuma o significado.\nSe for terminal ou log, destaque erro, causa provavel e proxima verificacao.\nSe for codigo, diga objetivo, tecnologia aparente e comportamento principal.\nNao mencione OCR, captura, tarefa ou instrucao.\nNao recite simbolos decorativos como #, **, >, -, _, crases ou cercas de markdown.\nNao devolva apenas palavras-chave desconexas; explique o sentido central do conteudo.\n\nOCR:\n{text_block}"
        ),
        Action::SearchWeb => format!(
            "{context}{language_hint}{profile_hint}{profile_guidance}Gere uma unica consulta curta para pesquisa web.\nResponda somente com a consulta final, em uma unica linha.\nSe for erro tecnico, priorize produto, biblioteca, comando, arquivo e sintoma.\nSe for texto natural, documento, pagina informativa ou pergunta geral, priorize o assunto central, pergunta principal, nomes proprios, pessoa, lugar, servico, organizacao, evento, data e termos essenciais.\nSe o texto estiver em outro idioma, preserve palavras-chave fortes e nomes proprios uteis para a busca.\nRemova marcacao decorativa e mantenha apenas sintaxe que muda o significado tecnico.\nNao mencione OCR, captura, tarefa ou instrucao.\nNao use aspas ou markdown.\n\nOCR:\n{text_block}"
        ),
    }
}

fn response_language_instruction(action: &Action, response_language: Option<&str>) -> String {
    if !matches!(action, Action::Explain) {
        return String::new();
    }

    let language = response_language
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Portuguese (Brazil)");

    format!("Idioma da resposta: {language}. ")
}

pub fn search_answer_system_prompt() -> &'static str {
    "Voce responde perguntas usando somente o contexto de busca fornecido. O contexto pode conter uma Visao Geral criada por IA do Google e fontes organicas. Responda de forma natural, amigavel e precisa, em 2 a 4 frases curtas, no idioma solicitado. Nao invente dados, datas, fontes ou detalhes ausentes. Nao mencione OCR, scraping, prompt ou instrucoes internas. Nao leia simbolos decorativos. Se o contexto for insuficiente, diga isso objetivamente no idioma solicitado."
}

pub fn repl_agent_system_prompt() -> &'static str {
    "Voce e o Coddy, um agente CLI para desenvolvimento de software dentro de um REPL local. Responda como ferramentas como Codex CLI ou Claude Code: direto, tecnico e acionavel. Nao trate toda mensagem como pesquisa web. Use web somente quando o usuario pedir explicitamente pesquisar, buscar, googlear ou consultar fontes. Para cumprimentos ou comandos simples, responda curto. Para perguntas tecnicas, explique com passos objetivos. Para codigo, preserve blocos de codigo quando util. Nao invente execucao de ferramentas, arquivos ou resultados que nao foram fornecidos."
}

pub fn repl_agent_user_prompt(user_message: &str) -> String {
    format!(
        "Mensagem do usuario no REPL:\n<<<USER\n{}\nUSER>>>\n\nResponda como um agente CLI. Se precisar de dados externos e o usuario nao pediu pesquisa web explicitamente, diga qual informacao precisa ser verificada em vez de abrir pesquisa.",
        user_message.trim()
    )
}

pub fn search_answer_user_prompt(
    query: &str,
    response_language: &str,
    source_label: &str,
    ai_overview_text: &str,
    supporting_sources: &str,
) -> String {
    let sources = if supporting_sources.trim().is_empty() {
        "Nenhuma fonte organica complementar foi extraida.".to_string()
    } else {
        supporting_sources.trim().to_string()
    };

    format!(
        "Pergunta do usuario:\n{query}\n\nIdioma da resposta:\n{response_language}\n\nFonte principal extraida:\n{source_label}\n\nTexto extraido da Visao Geral por IA do Google:\n<<<GOOGLE_AI_OVERVIEW\n{}\nGOOGLE_AI_OVERVIEW>>>\n\nFontes organicas complementares para validacao:\n<<<FONTES\n{sources}\nFONTES>>>\n\nTarefa:\nResponda ao usuario com base no texto extraido da Visao Geral por IA do Google. Use as fontes organicas apenas como apoio quando forem coerentes com a visao geral. Entregue somente a resposta final no idioma indicado.",
        ai_overview_text.trim()
    )
}

fn ocr_language_hint(ocr_text: &str) -> &'static str {
    if looks_like_hangul_text(ocr_text) {
        "Dica local: o texto parece estar em coreano. Leia o conteudo como texto corrido do idioma original antes de resumir ou traduzir. "
    } else if looks_like_cjk_text(ocr_text) {
        "Dica local: o texto parece estar em japones ou outro idioma CJK. Leia o conteudo como frases do idioma original antes de traduzir ou resumir. "
    } else if looks_like_cyrillic_text(ocr_text) {
        "Dica local: o texto parece usar alfabeto cirilico. Leia o conteudo como frase natural do idioma original, nao como tokens isolados. "
    } else if looks_like_arabic_text(ocr_text) {
        "Dica local: o texto parece estar em arabe. Leia o conteudo como frase natural do idioma original, nao como palavras soltas. "
    } else if looks_like_greek_text(ocr_text) {
        "Dica local: o texto parece estar em grego. Leia o conteudo como frase natural do idioma original. "
    } else if looks_like_devanagari_text(ocr_text) {
        "Dica local: o texto parece usar devanagari. Leia o conteudo como frase natural do idioma original. "
    } else {
        ""
    }
}

fn ocr_profile_hint(ocr_text: &str) -> &'static str {
    match ocr_content_profile(ocr_text) {
        OcrContentProfile::Terminal => "Dica local: o texto parece terminal ou log. ",
        OcrContentProfile::Code => "Dica local: o texto parece codigo ou configuracao. ",
        OcrContentProfile::Markdown => "Dica local: o texto parece markdown ou documentacao. ",
        OcrContentProfile::Dashboard => {
            "Dica local: o texto parece painel, dashboard ou tabela curta. "
        }
        OcrContentProfile::Natural => {
            "Dica local: o texto parece texto natural, documento ou interface. "
        }
    }
}

fn ocr_profile_guidance(action: &Action, ocr_text: &str) -> &'static str {
    match (action, ocr_content_profile(ocr_text)) {
        (Action::TranslatePtBr, OcrContentProfile::Terminal) => {
            "Guia local: traduza apenas mensagens humanas e textos explicativos; preserve comandos, caminhos, flags e trechos de log literalmente. "
        }
        (Action::TranslatePtBr, OcrContentProfile::Code) => {
            "Guia local: traduza comentarios, mensagens e texto humano, mas preserve codigo, APIs e identificadores literalmente. "
        }
        (Action::TranslatePtBr, OcrContentProfile::Markdown) => {
            "Guia local: ignore a sintaxe de markdown e traduza o conteudo pelo sentido, preservando estrutura textual de titulos e listas. "
        }
        (Action::TranslatePtBr, OcrContentProfile::Dashboard) => {
            "Guia local: traduza rotulos e descricoes, preservando numeros, percentuais, colunas e nomes de metricas. "
        }
        (Action::Explain, OcrContentProfile::Terminal) => {
            "Guia local: destaque erro ou estado principal, causa provavel e a proxima verificacao objetiva. "
        }
        (Action::Explain, OcrContentProfile::Code) => {
            "Guia local: explique objetivo, componente aparente, fluxo principal e o ponto de atencao mais relevante. "
        }
        (Action::Explain, OcrContentProfile::Markdown) => {
            "Guia local: explique o assunto do documento, a estrutura e a intencao do conteudo. "
        }
        (Action::Explain, OcrContentProfile::Dashboard) => {
            "Guia local: explique o que o painel mede, o contexto geral e os indicadores que mais chamam atencao. "
        }
        (Action::SearchWeb, OcrContentProfile::Terminal) => {
            "Guia local: monte a query com produto, comando, arquivo, erro exato e sintoma observado. "
        }
        (Action::SearchWeb, OcrContentProfile::Code) => {
            "Guia local: monte a query com linguagem, biblioteca ou API, funcao, arquivo e sintoma tecnico. "
        }
        (Action::SearchWeb, OcrContentProfile::Markdown) => {
            "Guia local: monte a query com o topico central, produto e nomes proprios mais relevantes. "
        }
        (Action::SearchWeb, OcrContentProfile::Dashboard) => {
            "Guia local: monte a query com nome do produto ou painel, metricas principais e contexto do problema. "
        }
        (Action::SearchWeb, OcrContentProfile::Natural) => {
            "Guia local: monte a query com o assunto central, pergunta principal, nomes proprios, pessoa, lugar, servico, organizacao, evento, data e termos do idioma original que ajudam a encontrar fontes melhores. "
        }
        _ => "",
    }
}

fn ocr_content_profile(ocr_text: &str) -> OcrContentProfile {
    if looks_like_terminal_or_log(ocr_text) {
        OcrContentProfile::Terminal
    } else if looks_like_code(ocr_text) {
        OcrContentProfile::Code
    } else if looks_like_markdown_or_documentation(ocr_text) {
        OcrContentProfile::Markdown
    } else if looks_like_dashboard_or_table(ocr_text) {
        OcrContentProfile::Dashboard
    } else {
        OcrContentProfile::Natural
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

fn looks_like_dashboard_or_table(input: &str) -> bool {
    let lines = input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let short_lines = lines
        .iter()
        .filter(|line| line.chars().count() <= 32)
        .count();
    let number_lines = lines
        .iter()
        .filter(|line| line.chars().any(|ch| ch.is_ascii_digit()))
        .count();
    let title_like_lines = lines
        .iter()
        .filter(|line| {
            !line.contains('.')
                && !line.contains('!')
                && !line.contains('?')
                && line.split_whitespace().count() <= 4
        })
        .count();

    lines.len() >= 4 && short_lines >= 3 && (number_lines >= 2 || title_like_lines >= 3)
}

fn looks_like_hangul_text(input: &str) -> bool {
    input
        .chars()
        .any(|ch| matches!(ch as u32, 0xAC00..=0xD7AF | 0x1100..=0x11FF))
}

fn looks_like_cjk_text(input: &str) -> bool {
    input.chars().any(is_cjk_char)
}

fn looks_like_cyrillic_text(input: &str) -> bool {
    input.chars().any(|ch| {
        matches!(
            ch as u32,
            0x0400..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F
        )
    })
}

fn looks_like_arabic_text(input: &str) -> bool {
    input
        .chars()
        .any(|ch| matches!(ch as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF))
}

fn looks_like_greek_text(input: &str) -> bool {
    input
        .chars()
        .any(|ch| matches!(ch as u32, 0x0370..=0x03FF | 0x1F00..=0x1FFF))
}

fn looks_like_devanagari_text(input: &str) -> bool {
    input
        .chars()
        .any(|ch| matches!(ch as u32, 0x0900..=0x097F | 0xA8E0..=0xA8FF))
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
    )
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
        assert!(prompt.contains("erro"));
    }

    #[test]
    fn search_prompt_mentions_general_context_entities() {
        let prompt = system_prompt(PromptPolicy::SearchQueryBuilder);
        assert!(prompt.contains("nomes proprios"));
        assert!(prompt.contains("servico"));
        assert!(prompt.contains("evento"));
    }

    #[test]
    fn translate_prompt_mentions_markdown_cleanup() {
        let prompt = system_prompt(PromptPolicy::TechnicalTranslatePtBr);
        assert!(prompt.contains("markdown"));
        assert!(prompt.contains("marcadores"));
    }

    #[test]
    fn user_prompt_includes_source_app_context_when_available() {
        let prompt = user_prompt(&Action::Explain, Some("org.gnome.Terminal"), None);
        assert!(prompt.contains("org.gnome.Terminal"));
    }

    #[test]
    fn explain_prompts_include_requested_response_language() {
        let image_prompt = user_prompt(&Action::Explain, None, Some("English"));
        assert!(image_prompt.contains("Idioma da resposta: English"));

        let text_prompt =
            user_prompt_from_text(&Action::Explain, None, Some("Chinese"), "error: failed");
        assert!(text_prompt.contains("Idioma da resposta: Chinese"));
    }

    #[test]
    fn text_prompt_mentions_ocr_and_terminal_context() {
        let prompt = user_prompt_from_text(
            &Action::Explain,
            Some("org.gnome.Terminal"),
            None,
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
            None,
            "fn main() {\n    println!(\"hello\");\n}",
        );
        assert!(prompt.contains("codigo ou configuracao"));
    }

    #[test]
    fn translate_prompt_mentions_cjk_language_hint() {
        let prompt = user_prompt_from_text(
            &Action::TranslatePtBr,
            None,
            None,
            "17の外国語で放送しています。",
        );
        assert!(prompt.contains("japones ou outro idioma CJK"));
        assert!(prompt.contains("traducao natural e completa"));
    }

    #[test]
    fn explain_prompt_mentions_dashboard_guidance() {
        let prompt = user_prompt_from_text(
            &Action::Explain,
            None,
            None,
            "Total CVEs\n0\nCritical\n0\nHigh\n0\nAvg CVSS\nN/A",
        );
        assert!(prompt.contains("painel, dashboard ou tabela curta"));
        assert!(prompt.contains("o que o painel mede"));
    }

    #[test]
    fn translate_prompt_mentions_cyrillic_hint() {
        let prompt = user_prompt_from_text(
            &Action::TranslatePtBr,
            None,
            None,
            "Сервис для граждан за рубежом",
        );
        assert!(prompt.contains("alfabeto cirilico"));
    }

    #[test]
    fn search_text_prompt_mentions_general_entities_for_natural_content() {
        let prompt = user_prompt_from_text(
            &Action::SearchWeb,
            Some("firefox"),
            None,
            "Serviço para cidadãos japoneses no exterior com transmissão de notícias em 17 idiomas.",
        );
        assert!(prompt.contains("pergunta principal"));
        assert!(prompt.contains("nomes proprios"));
        assert!(prompt.contains("servico"));
    }

    #[test]
    fn search_answer_prompt_is_grounded_on_google_ai_overview() {
        let system = search_answer_system_prompt();
        let user = search_answer_user_prompt(
            "O que é JavaScript?",
            "Portuguese (Brazil)",
            "Visão geral criada por IA renderizada no Google",
            "JavaScript é uma linguagem de programação de alto nível para web.",
            "MDN: JavaScript permite páginas interativas.",
        );

        assert!(system.contains("somente o contexto de busca fornecido"));
        assert!(system.contains("Nao invente"));
        assert!(user.contains("GOOGLE_AI_OVERVIEW"));
        assert!(user.contains("JavaScript é uma linguagem"));
        assert!(user.contains("Fontes organicas complementares"));
        assert!(user.contains("Idioma da resposta"));
        assert!(user.contains("somente a resposta final"));
    }

    #[test]
    fn repl_agent_prompt_avoids_default_web_search_behavior() {
        let system = repl_agent_system_prompt();
        let user = repl_agent_user_prompt("olá");

        assert!(system.contains("agente CLI"));
        assert!(system.contains("Nao trate toda mensagem como pesquisa web"));
        assert!(user.contains("Mensagem do usuario no REPL"));
        assert!(user.contains("olá"));
    }
}
