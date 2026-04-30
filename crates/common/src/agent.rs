use crate::{
    actions::RiskLevel,
    audit::{redact_for_audit, AuditLog},
    security::{
        ConfirmationRequest, PermissionEngine, PolicyDecision, PolicyInput, RiskContext,
        RuntimePolicy, SecurityReason,
    },
    session::{AgentContext, ConversationMessage, MessageRole, SessionId, SessionManager},
    tools::{ToolCall, ToolRegistry},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurn {
    pub session_id: SessionId,
    pub input: UserInput,
    pub context: AgentContext,
    pub policy: RuntimePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserInput {
    Text {
        text: String,
    },
    Voice {
        transcript: String,
        confidence: Option<f32>,
    },
    ScreenCapture {
        image_bytes: Vec<u8>,
        user_hint: Option<String>,
    },
    DocumentCommand {
        document_id: Option<String>,
        command: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantMessage {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentDecision {
    FinalAnswer(AssistantMessage),
    ToolCalls(Vec<ToolCall>),
    NeedConfirmation(ConfirmationRequest),
    Refuse(SecurityRefusal),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityRefusal {
    pub reason: String,
}

#[derive(Clone)]
pub struct AgentOrchestrator {
    registry: ToolRegistry,
    permission_engine: PermissionEngine,
    sessions: Arc<Mutex<SessionManager>>,
    audit_log: AuditLog,
}

impl AgentOrchestrator {
    pub fn new(
        registry: ToolRegistry,
        permission_engine: PermissionEngine,
        sessions: SessionManager,
        audit_log: AuditLog,
    ) -> Self {
        Self {
            registry,
            permission_engine,
            sessions: Arc::new(Mutex::new(sessions)),
            audit_log,
        }
    }

    pub fn default_local_first() -> Self {
        Self::new(
            ToolRegistry::builtin(),
            PermissionEngine::default(),
            SessionManager::default(),
            AuditLog::default(),
        )
    }

    pub fn process_turn(&self, turn: AgentTurn) -> AgentDecision {
        self.audit_log.record_tool_event(
            "assistant.turn_started",
            Some(turn.session_id.clone()),
            "agent_turn",
            RiskLevel::Level0,
            "started",
            json!({"input_mode": input_mode(&turn.input)}),
        );
        self.remember_user_input(&turn);

        let Some(call) = deterministic_tool_call(&turn.input) else {
            return AgentDecision::FinalAnswer(AssistantMessage {
                text: "Entrada recebida. Nenhuma ferramenta segura foi necessária.".into(),
            });
        };

        let permission_engine = PermissionEngine::new(turn.policy.clone());
        self.evaluate_tool_call_with_engine(
            &permission_engine,
            &turn.session_id,
            call,
            turn.context.risk_context,
        )
    }

    pub fn evaluate_tool_call(
        &self,
        session_id: &SessionId,
        call: ToolCall,
        context: RiskContext,
    ) -> AgentDecision {
        self.evaluate_tool_call_with_engine(&self.permission_engine, session_id, call, context)
    }

    fn evaluate_tool_call_with_engine(
        &self,
        permission_engine: &PermissionEngine,
        session_id: &SessionId,
        call: ToolCall,
        context: RiskContext,
    ) -> AgentDecision {
        let definition = match self.registry.validate_call(&call) {
            Ok(definition) => definition,
            Err(error) => {
                self.audit_log.record_tool_event(
                    "security.blocked",
                    Some(session_id.clone()),
                    call.name,
                    RiskLevel::Level5,
                    "schema_rejected",
                    json!({"error": error.to_string()}),
                );
                return AgentDecision::Refuse(SecurityRefusal {
                    reason: error.to_string(),
                });
            }
        };

        self.audit_log.record_tool_event(
            "tool.proposed",
            Some(session_id.clone()),
            definition.name.clone(),
            definition.risk_level,
            "proposed",
            json!({"arguments": redact_for_audit(&call.arguments)}),
        );

        let policy_input = PolicyInput {
            tool_name: definition.name.clone(),
            risk_level: definition.risk_level,
            permissions: definition.permissions.clone(),
            confirmation: definition.confirmation,
            arguments: call.arguments.clone(),
            context,
        };

        match permission_engine.evaluate(&policy_input) {
            PolicyDecision::Allow => {
                self.audit_log.record_tool_event(
                    "tool.executed",
                    Some(session_id.clone()),
                    definition.name.clone(),
                    definition.risk_level,
                    "allow",
                    json!({}),
                );
                AgentDecision::ToolCalls(vec![call])
            }
            PolicyDecision::RequireConfirmation(request) => {
                self.audit_log.record_tool_event(
                    "tool.confirmation_requested",
                    Some(session_id.clone()),
                    definition.name.clone(),
                    definition.risk_level,
                    "require_confirmation",
                    json!({"reason": request.reason}),
                );
                AgentDecision::NeedConfirmation(request)
            }
            PolicyDecision::Deny(reason) => {
                self.audit_denial(
                    session_id,
                    definition.name.clone(),
                    definition.risk_level,
                    &reason,
                );
                AgentDecision::Refuse(SecurityRefusal {
                    reason: reason.to_string(),
                })
            }
        }
    }

    pub fn audit_log(&self) -> AuditLog {
        self.audit_log.clone()
    }

    fn remember_user_input(&self, turn: &AgentTurn) {
        let Some(content) = user_input_text(&turn.input) else {
            return;
        };

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.ensure_session(turn.session_id.clone(), &turn.context.locale);
            let _ = sessions.append_message(
                &turn.session_id,
                ConversationMessage {
                    role: MessageRole::User,
                    content,
                },
            );
        }
    }

    fn audit_denial(
        &self,
        session_id: &SessionId,
        tool_name: String,
        risk_level: RiskLevel,
        reason: &SecurityReason,
    ) {
        self.audit_log.record_tool_event(
            "tool.denied",
            Some(session_id.clone()),
            tool_name,
            risk_level,
            "deny",
            json!({"reason": reason.to_string()}),
        );
    }
}

fn deterministic_tool_call(input: &UserInput) -> Option<ToolCall> {
    let text = user_input_text(input)?;
    let normalized = normalize_command(&text);

    if let Some(target) = normalized.strip_prefix("abra ") {
        return Some(ToolCall::new(
            "rule_open_application",
            "open_application",
            json!({"app_name": target.trim()}),
        ));
    }
    if let Some(target) = normalized.strip_prefix("open ") {
        return Some(ToolCall::new(
            "rule_open_application",
            "open_application",
            json!({"app_name": target.trim()}),
        ));
    }
    if let Some(query) = normalized
        .strip_prefix("pesquise ")
        .or_else(|| normalized.strip_prefix("search "))
    {
        return Some(ToolCall::new(
            "rule_search_web",
            "search_web",
            json!({"query": query.trim()}),
        ));
    }

    None
}

fn user_input_text(input: &UserInput) -> Option<String> {
    match input {
        UserInput::Text { text } => Some(text.trim().to_string()),
        UserInput::Voice { transcript, .. } => Some(transcript.trim().to_string()),
        UserInput::DocumentCommand { command, .. } => Some(command.trim().to_string()),
        UserInput::ScreenCapture { .. } => None,
    }
    .filter(|value| !value.is_empty())
}

fn input_mode(input: &UserInput) -> &'static str {
    match input {
        UserInput::Text { .. } => "text",
        UserInput::Voice { .. } => "voice",
        UserInput::ScreenCapture { .. } => "screen_capture",
        UserInput::DocumentCommand { .. } => "document",
    }
}

fn normalize_command(text: &str) -> String {
    text.trim()
        .trim_matches(|ch: char| ch == '.' || ch == '!' || ch == '?')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{security::RiskContext, session::AgentContext};

    #[test]
    fn rule_based_turn_can_propose_registered_tool() {
        let orchestrator = AgentOrchestrator::default_local_first();
        let turn = AgentTurn {
            session_id: SessionId::new(),
            input: UserInput::Text {
                text: "abra terminal".into(),
            },
            context: AgentContext {
                risk_context: RiskContext::user_initiated(),
                ..AgentContext::default()
            },
            policy: RuntimePolicy::default(),
        };

        match orchestrator.process_turn(turn) {
            AgentDecision::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "open_application");
                assert_eq!(calls[0].arguments["app_name"], "terminal");
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn refuses_unknown_tool_call() {
        let orchestrator = AgentOrchestrator::default_local_first();
        let session_id = SessionId::new();
        let decision = orchestrator.evaluate_tool_call(
            &session_id,
            ToolCall::new("bad", "shell", json!({"command": "rm -rf /"})),
            RiskContext::agent_proposed(),
        );

        assert!(matches!(decision, AgentDecision::Refuse(_)));
    }

    #[test]
    fn turn_policy_can_raise_confirmation_threshold() {
        let orchestrator = AgentOrchestrator::default_local_first();
        let turn = AgentTurn {
            session_id: SessionId::new(),
            input: UserInput::Text {
                text: "abra terminal".into(),
            },
            context: AgentContext {
                risk_context: RiskContext::user_initiated(),
                ..AgentContext::default()
            },
            policy: RuntimePolicy {
                confirm_risk_level: RiskLevel::Level1,
                ..RuntimePolicy::default()
            },
        };

        assert!(matches!(
            orchestrator.process_turn(turn),
            AgentDecision::NeedConfirmation(_)
        ));
    }
}
