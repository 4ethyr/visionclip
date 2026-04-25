use crate::actions::{find_action_spec, RiskLevel};
use crate::intent::IntentKind;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposedAction {
    pub name: String,
    #[serde(default)]
    pub arguments: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDecision {
    pub intent: IntentKind,
    pub confidence: f32,
    pub requires_action: bool,
    pub requires_confirmation: bool,
    pub risk_level: u8,
    #[serde(default)]
    pub slots: Map<String, Value>,
    #[serde(default)]
    pub proposed_action: Option<ProposedAction>,
    pub user_response: String,
    pub reasoning_summary: String,
}

impl AgentDecision {
    pub fn clarification(message: impl Into<String>) -> Self {
        Self {
            intent: IntentKind::Clarification,
            confidence: 1.0,
            requires_action: false,
            requires_confirmation: false,
            risk_level: RiskLevel::Level0.as_u8(),
            slots: Map::new(),
            proposed_action: None,
            user_response: message.into(),
            reasoning_summary: "A entrada não teve confiança suficiente para executar ação.".into(),
        }
    }

    pub fn validate_action_contract(&self) -> Result<(), String> {
        let Some(action) = &self.proposed_action else {
            if self.requires_action {
                return Err("decision requires action but proposed_action is missing".into());
            }
            return Ok(());
        };

        let spec = find_action_spec(&action.name)
            .ok_or_else(|| format!("unknown action in decision: {}", action.name))?;
        if self.risk_level != spec.risk_level.as_u8() {
            return Err(format!(
                "risk mismatch for action {}: decision={}, registry={}",
                action.name,
                self.risk_level,
                spec.risk_level.as_u8()
            ));
        }
        if spec.requires_confirmation && !self.requires_confirmation {
            return Err(format!(
                "action {} requires confirmation by registry policy",
                action.name
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_registered_open_application_decision() {
        let decision = AgentDecision {
            intent: IntentKind::OpenApplication,
            confidence: 0.94,
            requires_action: true,
            requires_confirmation: false,
            risk_level: 1,
            slots: Map::new(),
            proposed_action: Some(ProposedAction {
                name: "open_application".into(),
                arguments: Map::new(),
            }),
            user_response: "Abrindo o VS Code.".into(),
            reasoning_summary: "O usuário pediu para abrir um aplicativo.".into(),
        };

        assert!(decision.validate_action_contract().is_ok());
    }

    #[test]
    fn rejects_unconfirmed_screen_capture_decision() {
        let decision = AgentDecision {
            intent: IntentKind::ReadScreen,
            confidence: 0.9,
            requires_action: true,
            requires_confirmation: false,
            risk_level: 2,
            slots: Map::new(),
            proposed_action: Some(ProposedAction {
                name: "capture_screen_context".into(),
                arguments: Map::new(),
            }),
            user_response: "Vou ler a tela.".into(),
            reasoning_summary: "O usuário pediu contexto visual.".into(),
        };

        assert!(decision.validate_action_contract().is_err());
    }
}
