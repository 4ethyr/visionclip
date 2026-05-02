use crate::actions::{ActionPermission, ConfirmationPolicy, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskContext {
    pub user_initiated: bool,
    pub sensitive_context: bool,
    pub cloud_allowed: bool,
}

impl RiskContext {
    pub fn user_initiated() -> Self {
        Self {
            user_initiated: true,
            sensitive_context: false,
            cloud_allowed: false,
        }
    }

    pub fn agent_proposed() -> Self {
        Self {
            user_initiated: false,
            sensitive_context: false,
            cloud_allowed: false,
        }
    }

    pub fn sensitive_local_only() -> Self {
        Self {
            user_initiated: false,
            sensitive_context: true,
            cloud_allowed: false,
        }
    }
}

impl Default for RiskContext {
    fn default() -> Self {
        Self::agent_proposed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePolicy {
    pub confirm_risk_level: RiskLevel,
    pub block_dangerous_actions: bool,
    pub block_cloud_for_sensitive_context: bool,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            confirm_risk_level: RiskLevel::Level3,
            block_dangerous_actions: true,
            block_cloud_for_sensitive_context: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyInput {
    pub tool_name: String,
    pub risk_level: RiskLevel,
    pub permissions: Vec<ActionPermission>,
    pub confirmation: ConfirmationPolicy,
    #[serde(default)]
    pub arguments: Value,
    pub context: RiskContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    RequireConfirmation(ConfirmationRequest),
    Deny(SecurityReason),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfirmationRequest {
    pub id: String,
    pub tool_name: String,
    pub risk_level: RiskLevel,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityReason {
    DangerousRiskLevel,
    ConfirmationDisabled,
    ArbitraryShellBlocked,
    CloudSensitiveContextBlocked,
    InvalidUrl(String),
    UnknownSafeCommand(String),
}

impl fmt::Display for SecurityReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityReason::DangerousRiskLevel => {
                write!(formatter, "dangerous risk level is blocked by default")
            }
            SecurityReason::ConfirmationDisabled => write!(formatter, "tool is disabled by policy"),
            SecurityReason::ArbitraryShellBlocked => {
                write!(formatter, "arbitrary shell command is blocked")
            }
            SecurityReason::CloudSensitiveContextBlocked => {
                write!(
                    formatter,
                    "cloud inference is blocked for sensitive context"
                )
            }
            SecurityReason::InvalidUrl(reason) => write!(formatter, "invalid URL: {reason}"),
            SecurityReason::UnknownSafeCommand(command_id) => {
                write!(formatter, "safe command `{command_id}` is not allowlisted")
            }
        }
    }
}

impl std::error::Error for SecurityReason {}

#[derive(Debug, Clone, Default)]
pub struct PermissionEngine {
    policy: RuntimePolicy,
}

impl PermissionEngine {
    pub fn new(policy: RuntimePolicy) -> Self {
        Self { policy }
    }

    pub fn evaluate(&self, input: &PolicyInput) -> PolicyDecision {
        if input.confirmation == ConfirmationPolicy::Disabled {
            return PolicyDecision::Deny(SecurityReason::ConfirmationDisabled);
        }

        if self.policy.block_dangerous_actions && input.risk_level.is_blocked_by_default() {
            return PolicyDecision::Deny(SecurityReason::DangerousRiskLevel);
        }

        if self.shell_is_arbitrary(input) {
            return PolicyDecision::Deny(SecurityReason::ArbitraryShellBlocked);
        }

        if let Some(command_id) = self.unallowlisted_safe_command(input) {
            return PolicyDecision::Deny(SecurityReason::UnknownSafeCommand(command_id));
        }

        if let Some(reason) = invalid_open_url_reason(input) {
            return PolicyDecision::Deny(SecurityReason::InvalidUrl(reason));
        }

        if self.policy.block_cloud_for_sensitive_context
            && input.context.sensitive_context
            && !input.context.cloud_allowed
            && input
                .permissions
                .contains(&ActionPermission::CloudInference)
        {
            return PolicyDecision::Deny(SecurityReason::CloudSensitiveContextBlocked);
        }

        if must_confirm(input, &self.policy) {
            return PolicyDecision::RequireConfirmation(ConfirmationRequest {
                id: format!("confirm_{}", Uuid::new_v4()),
                tool_name: input.tool_name.clone(),
                risk_level: input.risk_level,
                reason: confirmation_reason(input),
            });
        }

        PolicyDecision::Allow
    }

    fn shell_is_arbitrary(&self, input: &PolicyInput) -> bool {
        if !input
            .permissions
            .contains(&ActionPermission::ShellRestricted)
        {
            return false;
        }

        contains_key(&input.arguments, &["command", "shell", "script", "argv"])
            || contains_suspicious_shell_text(&input.arguments)
    }

    fn unallowlisted_safe_command(&self, input: &PolicyInput) -> Option<String> {
        if input.tool_name != "run_safe_command" {
            return None;
        }

        let command_id = input
            .arguments
            .get("command_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if is_allowlisted_safe_command(command_id) {
            None
        } else {
            Some(command_id.to_string())
        }
    }
}

fn must_confirm(input: &PolicyInput, policy: &RuntimePolicy) -> bool {
    if input.permissions.contains(&ActionPermission::EmailSend)
        || input
            .permissions
            .contains(&ActionPermission::NetworkSettings)
    {
        return true;
    }

    if input.risk_level >= policy.confirm_risk_level {
        return true;
    }

    match input.confirmation {
        ConfirmationPolicy::Always => true,
        ConfirmationPolicy::OncePerResource | ConfirmationPolicy::OncePerSession => {
            !input.context.user_initiated
        }
        ConfirmationPolicy::Never | ConfirmationPolicy::Disabled => false,
    }
}

fn confirmation_reason(input: &PolicyInput) -> String {
    if input.permissions.contains(&ActionPermission::EmailSend) {
        return "envio de e-mail sempre exige confirmação".into();
    }
    if input
        .permissions
        .contains(&ActionPermission::NetworkSettings)
    {
        return "alteração de rede/VPN exige confirmação".into();
    }
    if input.risk_level >= RiskLevel::Level3 {
        return format!("risco {} exige confirmação", input.risk_level.as_u8());
    }
    match input.confirmation {
        ConfirmationPolicy::OncePerSession => "confirmação exigida uma vez por sessão".into(),
        ConfirmationPolicy::OncePerResource => "confirmação exigida para este recurso".into(),
        ConfirmationPolicy::Always => "confirmação sempre exigida".into(),
        ConfirmationPolicy::Never | ConfirmationPolicy::Disabled => {
            "confirmação exigida pela política".into()
        }
    }
}

fn contains_key(value: &Value, keys: &[&str]) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            keys.iter().any(|blocked| key.eq_ignore_ascii_case(blocked))
                || contains_key(value, keys)
        }),
        Value::Array(values) => values.iter().any(|value| contains_key(value, keys)),
        _ => false,
    }
}

fn contains_suspicious_shell_text(value: &Value) -> bool {
    match value {
        Value::String(text) => {
            let lowered = text.to_ascii_lowercase();
            lowered.contains("sh -c")
                || lowered.contains("bash -c")
                || lowered.contains("bash -lc")
                || lowered.contains("rm -rf")
                || lowered.contains("curl |")
                || lowered.contains("wget |")
                || lowered.contains("&&")
                || lowered.contains(';')
        }
        Value::Array(values) => values.iter().any(contains_suspicious_shell_text),
        Value::Object(object) => object.values().any(contains_suspicious_shell_text),
        _ => false,
    }
}

fn is_allowlisted_safe_command(command_id: &str) -> bool {
    matches!(
        command_id,
        "set_volume" | "set_brightness" | "vpn_up" | "vpn_down" | "toggle_vpn"
    )
}

fn invalid_open_url_reason(input: &PolicyInput) -> Option<String> {
    if input.tool_name != "open_url" {
        return None;
    }

    let url = input.arguments.get("url").and_then(Value::as_str)?.trim();
    validate_http_url(url).err()
}

fn validate_http_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("empty URL".into());
    }
    if url.contains(char::is_whitespace) {
        return Err("URL contains whitespace".into());
    }
    if url.chars().any(char::is_control) {
        return Err("URL contains control characters".into());
    }
    let lowered = url.to_ascii_lowercase();
    if lowered.starts_with("javascript:") {
        return Err("javascript URLs are blocked".into());
    }
    if lowered.starts_with("file:") {
        return Err("file URLs are blocked".into());
    }
    if !(lowered.starts_with("https://") || lowered.starts_with("http://")) {
        return Err("only http and https URLs are allowed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn risk_zero_allows_without_confirmation() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "speak_text".into(),
            risk_level: RiskLevel::Level0,
            permissions: vec![ActionPermission::AudioPlayback],
            confirmation: ConfirmationPolicy::Never,
            arguments: json!({"text": "ok"}),
            context: RiskContext::agent_proposed(),
        };

        assert_eq!(engine.evaluate(&input), PolicyDecision::Allow);
    }

    #[test]
    fn risk_three_requires_confirmation() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "set_volume".into(),
            risk_level: RiskLevel::Level3,
            permissions: vec![ActionPermission::SystemSettings],
            confirmation: ConfirmationPolicy::Never,
            arguments: json!({"percent": 50}),
            context: RiskContext::agent_proposed(),
        };

        assert!(matches!(
            engine.evaluate(&input),
            PolicyDecision::RequireConfirmation(_)
        ));
    }

    #[test]
    fn risk_five_is_blocked() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "dangerous_shell".into(),
            risk_level: RiskLevel::Level5,
            permissions: vec![ActionPermission::ShellRestricted],
            confirmation: ConfirmationPolicy::Always,
            arguments: json!({"command_id": "rm"}),
            context: RiskContext::agent_proposed(),
        };

        assert_eq!(
            engine.evaluate(&input),
            PolicyDecision::Deny(SecurityReason::DangerousRiskLevel)
        );
    }

    #[test]
    fn arbitrary_shell_is_blocked() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "run_safe_command".into(),
            risk_level: RiskLevel::Level3,
            permissions: vec![ActionPermission::ShellRestricted],
            confirmation: ConfirmationPolicy::Always,
            arguments: json!({"command": "bash -lc 'rm -rf /'"}),
            context: RiskContext::agent_proposed(),
        };

        assert_eq!(
            engine.evaluate(&input),
            PolicyDecision::Deny(SecurityReason::ArbitraryShellBlocked)
        );
    }

    #[test]
    fn cloud_sensitive_context_is_blocked_by_default() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "cloud_chat".into(),
            risk_level: RiskLevel::Level1,
            permissions: vec![ActionPermission::CloudInference],
            confirmation: ConfirmationPolicy::Never,
            arguments: json!({"prompt": "terminal log"}),
            context: RiskContext::sensitive_local_only(),
        };

        assert_eq!(
            engine.evaluate(&input),
            PolicyDecision::Deny(SecurityReason::CloudSensitiveContextBlocked)
        );
    }

    #[test]
    fn safe_command_uses_allowlist_and_still_requires_confirmation() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "run_safe_command".into(),
            risk_level: RiskLevel::Level3,
            permissions: vec![ActionPermission::ShellRestricted],
            confirmation: ConfirmationPolicy::Always,
            arguments: json!({"command_id": "set_volume", "arguments": {"percent": 50}}),
            context: RiskContext::agent_proposed(),
        };

        assert!(matches!(
            engine.evaluate(&input),
            PolicyDecision::RequireConfirmation(_)
        ));
    }

    #[test]
    fn invalid_url_is_blocked_by_policy() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "open_url".into(),
            risk_level: RiskLevel::Level1,
            permissions: vec![ActionPermission::DesktopLaunch, ActionPermission::Network],
            confirmation: ConfirmationPolicy::Never,
            arguments: json!({"url": "javascript:alert(1)"}),
            context: RiskContext::user_initiated(),
        };

        assert!(matches!(
            engine.evaluate(&input),
            PolicyDecision::Deny(SecurityReason::InvalidUrl(_))
        ));
    }

    #[test]
    fn vpn_network_change_always_requires_confirmation() {
        let engine = PermissionEngine::default();
        let input = PolicyInput {
            tool_name: "toggle_vpn".into(),
            risk_level: RiskLevel::Level3,
            permissions: vec![ActionPermission::NetworkSettings],
            confirmation: ConfirmationPolicy::Never,
            arguments: json!({"profile_name": "work", "enabled": true}),
            context: RiskContext::user_initiated(),
        };

        assert!(matches!(
            engine.evaluate(&input),
            PolicyDecision::RequireConfirmation(_)
        ));
    }
}
