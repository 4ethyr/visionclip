use crate::security::RiskContext;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    pub fn new() -> Self {
        Self(format!("sess_{}", Uuid::new_v4()))
    }

    pub fn from_request_id(request_id: Uuid) -> Self {
        Self(format!("sess_{request_id}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentContext {
    pub document_id: String,
    pub title: Option<String>,
    pub current_chunk_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentContext {
    pub locale: String,
    pub active_app: Option<String>,
    pub active_window_title: Option<String>,
    pub current_document: Option<DocumentContext>,
    pub current_task_id: Option<String>,
    pub recent_messages: Vec<ConversationMessage>,
    pub risk_context: RiskContext,
}

impl AgentContext {
    pub fn new(locale: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
            active_app: None,
            active_window_title: None,
            current_document: None,
            current_task_id: None,
            recent_messages: Vec::new(),
            risk_context: RiskContext::default(),
        }
    }
}

impl Default for AgentContext {
    fn default() -> Self {
        Self::new("pt-BR")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionState {
    pub id: SessionId,
    pub started_at_unix_ms: u64,
    pub last_active_unix_ms: u64,
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub struct SessionManager {
    timeout_ms: u64,
    max_history_messages: usize,
    sessions: HashMap<SessionId, SessionState>,
}

impl SessionManager {
    pub fn new(timeout: Duration, max_history_messages: usize) -> Self {
        Self {
            timeout_ms: timeout.as_millis() as u64,
            max_history_messages,
            sessions: HashMap::new(),
        }
    }

    pub fn create_session(&mut self, locale: impl Into<String>) -> SessionId {
        self.create_session_at(locale, unix_ms_now())
    }

    pub fn create_session_at(&mut self, locale: impl Into<String>, now_ms: u64) -> SessionId {
        let id = SessionId::new();
        self.insert_session_at(id.clone(), locale, now_ms);
        id
    }

    pub fn ensure_session(&mut self, id: SessionId, locale: impl Into<String>) -> SessionId {
        self.ensure_session_at(id, locale, unix_ms_now())
    }

    pub fn ensure_session_at(
        &mut self,
        id: SessionId,
        locale: impl Into<String>,
        now_ms: u64,
    ) -> SessionId {
        if !self.sessions.contains_key(&id) {
            self.insert_session_at(id.clone(), locale, now_ms);
        }
        id
    }

    fn insert_session_at(&mut self, id: SessionId, locale: impl Into<String>, now_ms: u64) {
        let state = SessionState {
            id: id.clone(),
            started_at_unix_ms: now_ms,
            last_active_unix_ms: now_ms,
            context: AgentContext::new(locale),
        };
        self.sessions.insert(id.clone(), state);
    }

    pub fn get(&self, id: &SessionId) -> Option<&SessionState> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &SessionId) -> Option<&mut SessionState> {
        self.sessions.get_mut(id)
    }

    pub fn touch(&mut self, id: &SessionId) {
        self.touch_at(id, unix_ms_now());
    }

    pub fn touch_at(&mut self, id: &SessionId, now_ms: u64) {
        if let Some(state) = self.sessions.get_mut(id) {
            state.last_active_unix_ms = now_ms;
        }
    }

    pub fn append_message(&mut self, id: &SessionId, message: ConversationMessage) -> Option<()> {
        let max_history_messages = self.max_history_messages;
        let state = self.sessions.get_mut(id)?;
        state.context.recent_messages.push(message);
        if state.context.recent_messages.len() > max_history_messages {
            let extra = state.context.recent_messages.len() - max_history_messages;
            state.context.recent_messages.drain(0..extra);
        }
        state.last_active_unix_ms = unix_ms_now();
        Some(())
    }

    pub fn expire_inactive(&mut self) -> Vec<SessionId> {
        self.expire_inactive_at(unix_ms_now())
    }

    pub fn expire_inactive_at(&mut self, now_ms: u64) -> Vec<SessionId> {
        let expired = self
            .sessions
            .iter()
            .filter_map(|(id, state)| {
                let inactive_ms = now_ms.saturating_sub(state.last_active_unix_ms);
                (inactive_ms > self.timeout_ms).then_some(id.clone())
            })
            .collect::<Vec<_>>();

        for id in &expired {
            self.sessions.remove(id);
        }

        expired
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(20 * 60), 30)
    }
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
    fn creates_session_with_default_context() {
        let mut manager = SessionManager::new(Duration::from_secs(60), 10);
        let id = manager.create_session_at("pt-BR", 1_000);

        let state = manager.get(&id).expect("session");
        assert_eq!(state.context.locale, "pt-BR");
        assert_eq!(state.started_at_unix_ms, 1_000);
        assert_eq!(state.last_active_unix_ms, 1_000);
    }

    #[test]
    fn updates_context_and_preserves_document_context() {
        let mut manager = SessionManager::new(Duration::from_secs(60), 10);
        let id = manager.create_session_at("pt-BR", 1_000);
        let state = manager.get_mut(&id).expect("session");
        state.context.current_document = Some(DocumentContext {
            document_id: "doc_1".into(),
            title: Some("Manual".into()),
            current_chunk_index: Some(7),
        });

        let state = manager.get(&id).expect("session");
        assert_eq!(
            state.context.current_document.as_ref().unwrap().document_id,
            "doc_1"
        );
        assert_eq!(
            state
                .context
                .current_document
                .as_ref()
                .unwrap()
                .current_chunk_index,
            Some(7)
        );
    }

    #[test]
    fn expires_inactive_sessions() {
        let mut manager = SessionManager::new(Duration::from_secs(10), 10);
        let old = manager.create_session_at("pt-BR", 1_000);
        let current = manager.create_session_at("pt-BR", 9_000);

        let expired = manager.expire_inactive_at(12_001);

        assert_eq!(expired, vec![old.clone()]);
        assert!(manager.get(&old).is_none());
        assert!(manager.get(&current).is_some());
    }

    #[test]
    fn caps_recent_messages() {
        let mut manager = SessionManager::new(Duration::from_secs(60), 2);
        let id = manager.create_session_at("pt-BR", 1_000);

        for content in ["one", "two", "three"] {
            manager.append_message(
                &id,
                ConversationMessage {
                    role: MessageRole::User,
                    content: content.into(),
                },
            );
        }

        let messages = &manager.get(&id).unwrap().context.recent_messages;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "two");
        assert_eq!(messages[1].content, "three");
    }
}
