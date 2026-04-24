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
        PromptPolicy::StrictOcr => {
            "Extraia apenas o texto visível. Não converse. Não explique. Preserve linhas, símbolos e números."
        }
        PromptPolicy::StrictCode => {
            "Retorne apenas código puro. Não use markdown. Não use cercas ``` . Não adicione prefácio, observações ou comentários extras. Preserve indentação."
        }
        PromptPolicy::TechnicalTranslatePtBr => {
            "Traduza para PT-BR técnico. Preserve termos técnicos quando necessário. Retorne apenas a tradução."
        }
        PromptPolicy::TechnicalExplainShort => {
            "Explique de forma técnica, curta e objetiva em até 4 frases. Priorize função, contexto e impacto operacional."
        }
        PromptPolicy::SearchQueryBuilder => {
            "Gere apenas uma query curta e eficaz para pesquisa web com base no conteúdo da imagem. Sem comentários adicionais."
        }
    }
}

pub fn user_prompt(action: &Action) -> &'static str {
    match action {
        Action::CopyText => "Extraia todo o texto desta captura.",
        Action::ExtractCode => "Transcreva o código desta captura e devolva somente o código.",
        Action::TranslatePtBr => "Traduza o conteúdo desta captura para PT-BR.",
        Action::Explain => "Explique tecnicamente o conteúdo desta captura.",
        Action::SearchWeb => "Gere uma única consulta de pesquisa web com base nesta captura.",
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
}
