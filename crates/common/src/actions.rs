use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    Level0,
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
}

impl RiskLevel {
    pub fn as_u8(self) -> u8 {
        match self {
            RiskLevel::Level0 => 0,
            RiskLevel::Level1 => 1,
            RiskLevel::Level2 => 2,
            RiskLevel::Level3 => 3,
            RiskLevel::Level4 => 4,
            RiskLevel::Level5 => 5,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(RiskLevel::Level0),
            1 => Some(RiskLevel::Level1),
            2 => Some(RiskLevel::Level2),
            3 => Some(RiskLevel::Level3),
            4 => Some(RiskLevel::Level4),
            5 => Some(RiskLevel::Level5),
            _ => None,
        }
    }

    pub fn requires_confirmation(self) -> bool {
        self >= RiskLevel::Level3
    }

    pub fn is_blocked_by_default(self) -> bool {
        self >= RiskLevel::Level5
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ConfirmationPolicy {
    Never,
    OncePerSession,
    OncePerResource,
    Always,
    Disabled,
}

impl ConfirmationPolicy {
    pub fn requires_confirmation(self) -> bool {
        matches!(
            self,
            ConfirmationPolicy::OncePerSession
                | ConfirmationPolicy::OncePerResource
                | ConfirmationPolicy::Always
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionPermission {
    DesktopLaunch,
    Network,
    ScreenCapture,
    WindowRead,
    FileRead,
    FileWrite,
    ClipboardWrite,
    AudioPlayback,
    AudioCapture,
    SystemSettings,
    NetworkSettings,
    EmailDraft,
    EmailSend,
    LocalFilesRead,
    LocalFilesWrite,
    ShellRestricted,
    CloudInference,
    McpToolUse,
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
    pub confirmation: ConfirmationPolicy,
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
        let confirmation = if risk_level.requires_confirmation() {
            ConfirmationPolicy::Always
        } else {
            ConfirmationPolicy::Never
        };
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
            confirmation,
            requires_confirmation: confirmation.requires_confirmation(),
        }
    }

    fn with_confirmation(mut self, confirmation: ConfirmationPolicy) -> Self {
        self.confirmation = confirmation;
        self.requires_confirmation = confirmation.requires_confirmation();
        self
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
            "search_files",
            "Pesquisa metadados de arquivos locais no indice do VisionClip.",
            RiskLevel::Level1,
            vec![ActionPermission::LocalFilesRead],
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "mode": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "hits": {"type": "array"},
                    "elapsed_ms": {"type": "integer"}
                },
                "required": ["hits"]
            }),
            2_000,
        ),
        ActionSpec::new(
            "search_file_content",
            "Pesquisa conteudo local indexado, incluindo documentos e OCR quando habilitados.",
            RiskLevel::Level2,
            vec![ActionPermission::FileRead, ActionPermission::LocalFilesRead],
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "mode": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "hits": {"type": "array"},
                    "elapsed_ms": {"type": "integer"}
                },
                "required": ["hits"]
            }),
            4_000,
        ),
        ActionSpec::new(
            "open_search_result",
            "Abre ou revela um resultado de busca local usando launchers seguros do desktop.",
            RiskLevel::Level2,
            vec![ActionPermission::DesktopLaunch, ActionPermission::FileRead],
            json!({
                "type": "object",
                "properties": {
                    "result_id": {"type": "string"},
                    "action": {"type": "string", "enum": ["open", "reveal", "ask_about", "summarize"]}
                },
                "required": ["result_id", "action"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        )
        .with_confirmation(ConfirmationPolicy::OncePerResource),
        ActionSpec::new(
            "index_add_root",
            "Adiciona uma root explicita ao catalogo de busca local.",
            RiskLevel::Level2,
            vec![ActionPermission::LocalFilesRead],
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "sensitive": {"type": "boolean"}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        )
        .with_confirmation(ConfirmationPolicy::OncePerResource),
        ActionSpec::new(
            "index_sensitive_root",
            "Tenta indexar root sensivel e exige confirmacao explicita.",
            RiskLevel::Level3,
            vec![ActionPermission::FileRead, ActionPermission::LocalFilesRead],
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "reason": {"type": "string"}
                },
                "required": ["path", "reason"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        ),
        ActionSpec::new(
            "send_result_to_cloud",
            "Envia conteudo local recuperado para provedor cloud quando politica permitir.",
            RiskLevel::Level5,
            vec![ActionPermission::CloudInference, ActionPermission::Network],
            json!({
                "type": "object",
                "properties": {
                    "result_id": {"type": "string"},
                    "provider": {"type": "string"}
                },
                "required": ["result_id", "provider"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"}
                },
                "required": ["success"]
            }),
            10_000,
        )
        .with_confirmation(ConfirmationPolicy::Disabled),
        ActionSpec::new(
            "open_url",
            "Abre uma URL http/https no navegador padrao do usuario.",
            RiskLevel::Level1,
            vec![ActionPermission::DesktopLaunch, ActionPermission::Network],
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "label": {"type": "string"}
                },
                "required": ["url"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        ),
        ActionSpec::new(
            "open_document",
            "Busca e abre um documento local do usuario usando apenas launchers de desktop seguros.",
            RiskLevel::Level2,
            vec![ActionPermission::DesktopLaunch, ActionPermission::FileRead],
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 5, "default": 1}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "path": {"type": "string"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            8_000,
        )
        .with_confirmation(ConfirmationPolicy::OncePerResource),
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
        )
        .with_confirmation(ConfirmationPolicy::OncePerSession),
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
            "ingest_document",
            "Ingere documento local escolhido pelo usuario para chunking e leitura.",
            RiskLevel::Level2,
            vec![ActionPermission::FileRead],
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "document_id": {"type": "string"},
                    "chunks": {"type": "integer"}
                },
                "required": ["document_id", "chunks"]
            }),
            10_000,
        )
        .with_confirmation(ConfirmationPolicy::OncePerResource),
        ActionSpec::new(
            "ask_document",
            "Responde pergunta usando contexto local de documento ingerido.",
            RiskLevel::Level1,
            vec![ActionPermission::FileRead],
            json!({
                "type": "object",
                "properties": {
                    "document_id": {"type": "string"},
                    "question": {"type": "string"}
                },
                "required": ["document_id", "question"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "answer": {"type": "string"},
                    "citations": {"type": "array"}
                },
                "required": ["answer"]
            }),
            10_000,
        ),
        ActionSpec::new(
            "summarize_document",
            "Resume documento local ingerido.",
            RiskLevel::Level1,
            vec![ActionPermission::FileRead],
            json!({
                "type": "object",
                "properties": {
                    "document_id": {"type": "string"}
                },
                "required": ["document_id"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "summary": {"type": "string"}
                },
                "required": ["summary"]
            }),
            10_000,
        ),
        ActionSpec::new(
            "read_document_aloud",
            "Le documento local em voz alta com traducao incremental opcional.",
            RiskLevel::Level2,
            vec![ActionPermission::FileRead, ActionPermission::AudioPlayback],
            json!({
                "type": "object",
                "properties": {
                    "document_id": {"type": "string"},
                    "target_language": {"type": "string"}
                },
                "required": ["document_id"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "reading_session_id": {"type": "string"},
                    "started": {"type": "boolean"}
                },
                "required": ["reading_session_id", "started"]
            }),
            10_000,
        )
        .with_confirmation(ConfirmationPolicy::OncePerResource),
        ActionSpec::new(
            "translate_document",
            "Traduz documento local de forma incremental para o idioma alvo.",
            RiskLevel::Level2,
            vec![ActionPermission::FileRead],
            json!({
                "type": "object",
                "properties": {
                    "document_id": {"type": "string"},
                    "target_language": {"type": "string"}
                },
                "required": ["document_id", "target_language"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "translated_chunks": {"type": "integer"},
                    "target_language": {"type": "string"}
                },
                "required": ["translated_chunks", "target_language"]
            }),
            10_000,
        )
        .with_confirmation(ConfirmationPolicy::OncePerResource),
        ActionSpec::new(
            "pause_reading",
            "Pausa leitura de documento em andamento.",
            RiskLevel::Level0,
            vec![ActionPermission::AudioPlayback],
            json!({
                "type": "object",
                "properties": {
                    "reading_session_id": {"type": "string"}
                },
                "required": ["reading_session_id"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "paused": {"type": "boolean"}
                },
                "required": ["paused"]
            }),
            2_000,
        ),
        ActionSpec::new(
            "resume_reading",
            "Retoma leitura de documento pausada.",
            RiskLevel::Level0,
            vec![ActionPermission::AudioPlayback],
            json!({
                "type": "object",
                "properties": {
                    "reading_session_id": {"type": "string"}
                },
                "required": ["reading_session_id"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "resumed": {"type": "boolean"}
                },
                "required": ["resumed"]
            }),
            2_000,
        ),
        ActionSpec::new(
            "stop_reading",
            "Interrompe leitura de documento em andamento.",
            RiskLevel::Level0,
            vec![ActionPermission::AudioPlayback],
            json!({
                "type": "object",
                "properties": {
                    "reading_session_id": {"type": "string"}
                },
                "required": ["reading_session_id"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "stopped": {"type": "boolean"}
                },
                "required": ["stopped"]
            }),
            2_000,
        ),
        ActionSpec::new(
            "set_volume",
            "Ajusta o volume do dispositivo de saída padrão usando template seguro.",
            RiskLevel::Level3,
            vec![ActionPermission::SystemSettings],
            json!({
                "type": "object",
                "properties": {
                    "percent": {"type": "integer", "minimum": 0, "maximum": 150}
                },
                "required": ["percent"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        ),
        ActionSpec::new(
            "set_brightness",
            "Ajusta brilho da tela usando template seguro.",
            RiskLevel::Level3,
            vec![ActionPermission::SystemSettings],
            json!({
                "type": "object",
                "properties": {
                    "percent": {"type": "integer", "minimum": 0, "maximum": 100}
                },
                "required": ["percent"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            5_000,
        ),
        ActionSpec::new(
            "toggle_vpn",
            "Liga ou desliga uma conexão VPN conhecida do NetworkManager.",
            RiskLevel::Level3,
            vec![ActionPermission::NetworkSettings],
            json!({
                "type": "object",
                "properties": {
                    "profile_name": {"type": "string"},
                    "enabled": {"type": "boolean"}
                },
                "required": ["profile_name", "enabled"],
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "properties": {
                    "success": {"type": "boolean"},
                    "message": {"type": "string"}
                },
                "required": ["success", "message"]
            }),
            10_000,
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

    #[test]
    fn open_url_uses_low_risk_desktop_and_network_permissions() {
        let spec = find_action_spec("open_url").expect("action spec");
        assert_eq!(spec.risk_level, RiskLevel::Level1);
        assert!(!spec.requires_confirmation);
        assert!(spec.permissions.contains(&ActionPermission::DesktopLaunch));
        assert!(spec.permissions.contains(&ActionPermission::Network));
    }

    #[test]
    fn open_document_reads_local_files_with_resource_confirmation() {
        let spec = find_action_spec("open_document").expect("action spec");
        assert_eq!(spec.risk_level, RiskLevel::Level2);
        assert_eq!(spec.confirmation, ConfirmationPolicy::OncePerResource);
        assert!(spec.permissions.contains(&ActionPermission::DesktopLaunch));
        assert!(spec.permissions.contains(&ActionPermission::FileRead));
    }

    #[test]
    fn system_setting_tools_require_confirmation() {
        for name in ["set_volume", "set_brightness", "toggle_vpn"] {
            let spec = find_action_spec(name).expect("action spec");
            assert_eq!(spec.risk_level, RiskLevel::Level3);
            assert!(spec.requires_confirmation);
        }

        let vpn = find_action_spec("toggle_vpn").expect("vpn action spec");
        assert!(vpn.permissions.contains(&ActionPermission::NetworkSettings));
    }

    #[test]
    fn document_tools_are_registered_with_safe_risk_levels() {
        let ingest = find_action_spec("ingest_document").expect("ingest document spec");
        assert_eq!(ingest.risk_level, RiskLevel::Level2);
        assert_eq!(ingest.confirmation, ConfirmationPolicy::OncePerResource);
        assert!(ingest.permissions.contains(&ActionPermission::FileRead));

        let read = find_action_spec("read_document_aloud").expect("read document spec");
        assert_eq!(read.risk_level, RiskLevel::Level2);
        assert!(read.permissions.contains(&ActionPermission::AudioPlayback));

        let pause = find_action_spec("pause_reading").expect("pause reading spec");
        assert_eq!(pause.risk_level, RiskLevel::Level0);
    }
}
