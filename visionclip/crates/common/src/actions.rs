use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    Level0,
    Level1,
    Level2,
    Level3,
    Level4,
}

impl RiskLevel {
    pub fn as_u8(self) -> u8 {
        match self {
            RiskLevel::Level0 => 0,
            RiskLevel::Level1 => 1,
            RiskLevel::Level2 => 2,
            RiskLevel::Level3 => 3,
            RiskLevel::Level4 => 4,
        }
    }

    pub fn requires_confirmation(self) -> bool {
        self >= RiskLevel::Level2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionPermission {
    DesktopLaunch,
    Network,
    ScreenCapture,
    AudioPlayback,
    LocalFilesRead,
    LocalFilesWrite,
    ShellRestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub backoff_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionSpec {
    pub name: String,
    pub description: String,
    pub risk_level: RiskLevel,
    pub permissions: Vec<ActionPermission>,
    pub input_schema: Value,
    pub output_schema: Value,
    pub timeout_ms: u64,
    pub retry_policy: RetryPolicy,
    pub requires_confirmation: bool,
}

impl ActionSpec {
    fn new(
        name: &str,
        description: &str,
        risk_level: RiskLevel,
        permissions: Vec<ActionPermission>,
        input_schema: Value,
        output_schema: Value,
        timeout_ms: u64,
    ) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            risk_level,
            permissions,
            input_schema,
            output_schema,
            timeout_ms,
            retry_policy: RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
            },
            requires_confirmation: risk_level.requires_confirmation(),
        }
    }
}

pub fn builtin_action_specs() -> Vec<ActionSpec> {
    vec![
        ActionSpec::new(
            "open_application",
            "Abre um aplicativo Linux instalado usando resolução segura de .desktop.",
            RiskLevel::Level1,
            vec![ActionPermission::DesktopLaunch],
            json!({
                "type": "object",
                "properties": {
                    "app_name": {"type": "string"},
                    "launch_mode": {"type": "string", "enum": ["default", "new_window", "reuse"], "default": "default"}
                },
                "required": ["app_name"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "resolved_app": {"type": "string"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        ),
        ActionSpec::new(
            "search_web",
            "Pesquisa informações na web usando provedores configurados e fontes citáveis.",
            RiskLevel::Level0,
            vec![ActionPermission::Network],
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 10, "default": 5}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "results": {"type": "array"},
                    "summary": {"type": "string"}
                },
                "required": ["query", "results"]
            }),
            12_000,
        ),
        ActionSpec::new(
            "capture_screen_context",
            "Captura texto visível por acessibilidade, DOM permitido, screenshot ou OCR local.",
            RiskLevel::Level2,
            vec![ActionPermission::ScreenCapture],
            json!({
                "type": "object",
                "properties": {
                    "mode": {"type": "string", "enum": ["visible_text", "screenshot_ocr", "browser_context"]}
                },
                "required": ["mode"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "visible_text": {"type": "string"},
                    "source_urls": {"type": "array", "items": {"type": "string"}},
                    "extraction_method": {"type": "string"}
                }
            }),
            15_000,
        ),
        ActionSpec::new(
            "speak_text",
            "Enfileira texto para síntese de voz local ou remota.",
            RiskLevel::Level0,
            vec![ActionPermission::AudioPlayback],
            json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string"},
                    "language": {"type": "string"}
                },
                "required": ["text"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "queued": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["queued"]
            }),
            5_000,
        ),
        ActionSpec::new(
            "run_safe_command",
            "Executa apenas comandos locais allowlistados, nunca shell arbitrário gerado por LLM.",
            RiskLevel::Level3,
            vec![ActionPermission::ShellRestricted],
            json!({
                "type": "object",
                "properties": {
                    "command_id": {"type": "string"},
                    "arguments": {"type": "object"}
                },
                "required": ["command_id"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "stdout": {"type": "string"},
                    "stderr": {"type": "string"}
                },
                "required": ["success"]
            }),
            10_000,
        ),
    ]
}

pub fn find_action_spec(name: &str) -> Option<ActionSpec> {
    builtin_action_specs()
        .into_iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_capture_requires_confirmation() {
        let spec = find_action_spec("capture_screen_context").expect("action spec");
        assert_eq!(spec.risk_level, RiskLevel::Level2);
        assert!(spec.requires_confirmation);
    }

    #[test]
    fn llm_shell_action_is_high_risk_and_restricted() {
        let spec = find_action_spec("run_safe_command").expect("action spec");
        assert_eq!(spec.risk_level.as_u8(), 3);
        assert!(spec
            .permissions
            .contains(&ActionPermission::ShellRestricted));
    }
}
