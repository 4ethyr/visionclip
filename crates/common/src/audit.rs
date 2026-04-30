use crate::{actions::RiskLevel, session::SessionId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEvent {
    pub id: String,
    pub captured_at_unix_ms: u64,
    pub session_id: Option<SessionId>,
    pub event_type: String,
    pub risk_level: Option<RiskLevel>,
    pub tool_name: Option<String>,
    pub provider: Option<String>,
    pub decision: Option<String>,
    #[serde(default)]
    pub data: Value,
}

impl AuditEvent {
    pub fn new(event_type: impl Into<String>) -> Self {
        Self {
            id: format!("evt_{}", Uuid::new_v4()),
            captured_at_unix_ms: unix_ms_now(),
            session_id: None,
            event_type: event_type.into(),
            risk_level: None,
            tool_name: None,
            provider: None,
            decision: None,
            data: Value::Object(Default::default()),
        }
    }

    pub fn tool_event(
        event_type: impl Into<String>,
        session_id: Option<SessionId>,
        tool_name: impl Into<String>,
        risk_level: RiskLevel,
        decision: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            tool_name: Some(tool_name.into()),
            risk_level: Some(risk_level),
            decision: Some(decision.into()),
            ..Self::new(event_type)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuditLog {
    events: Arc<Mutex<Vec<AuditEvent>>>,
}

impl AuditLog {
    pub fn record(&self, event: AuditEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    pub fn record_tool_event(
        &self,
        event_type: impl Into<String>,
        session_id: Option<SessionId>,
        tool_name: impl Into<String>,
        risk_level: RiskLevel,
        decision: impl Into<String>,
        data: Value,
    ) {
        let mut event =
            AuditEvent::tool_event(event_type, session_id, tool_name, risk_level, decision);
        event.data = data;
        self.record(event);
    }

    pub fn events(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.events.lock().map(|events| events.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn redact_for_audit(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    if is_sensitive_key(key) {
                        (key.clone(), json!("<redacted>"))
                    } else {
                        (key.clone(), redact_for_audit(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(redact_for_audit).collect()),
        Value::String(text) if looks_like_secret(text) => json!("<redacted>"),
        _ => value.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("api_key")
        || key.contains("apikey")
        || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("senha")
}

fn looks_like_secret(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.starts_with("sk-")
        || lowered.starts_with("or-")
        || lowered.contains("api_key=")
        || lowered.contains("authorization: bearer")
}

fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_tool_event() {
        let audit = AuditLog::default();
        let session_id = SessionId::new();

        audit.record_tool_event(
            "tool.executed",
            Some(session_id.clone()),
            "open_url",
            RiskLevel::Level1,
            "allow",
            json!({"url": "https://example.com"}),
        );

        let events = audit.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, Some(session_id));
        assert_eq!(events[0].tool_name.as_deref(), Some("open_url"));
    }

    #[test]
    fn redacts_sensitive_values() {
        let redacted = redact_for_audit(&json!({
            "api_key": "sk-secret",
            "nested": {"token": "abc"},
            "query": "rust"
        }));

        assert_eq!(redacted["api_key"], "<redacted>");
        assert_eq!(redacted["nested"]["token"], "<redacted>");
        assert_eq!(redacted["query"], "rust");
    }
}
