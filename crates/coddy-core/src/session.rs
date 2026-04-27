use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ReplMode {
    FloatingTerminal,
    DesktopApp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SessionStatus {
    Idle,
    Listening,
    Transcribing,
    CapturingScreen,
    BuildingContext,
    Thinking,
    Streaming,
    Speaking,
    AwaitingConfirmation,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceState {
    pub enabled: bool,
    pub speaking: bool,
    pub muted: bool,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            enabled: true,
            speaking: false,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextItem {
    pub id: String,
    pub label: String,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplMessage {
    pub id: Uuid,
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplSession {
    pub id: Uuid,
    pub mode: ReplMode,
    pub status: SessionStatus,
    pub policy: crate::AssessmentPolicy,
    pub selected_model: crate::ModelRef,
    pub voice: VoiceState,
    pub screen_context: Option<crate::ScreenUnderstandingContext>,
    pub workspace_context: Vec<ContextItem>,
    pub messages: Vec<ReplMessage>,
    pub active_run: Option<Uuid>,
}

impl ReplSession {
    pub fn new(mode: ReplMode, selected_model: crate::ModelRef) -> Self {
        Self {
            id: Uuid::new_v4(),
            mode,
            status: SessionStatus::Idle,
            policy: crate::AssessmentPolicy::UnknownAssessment,
            selected_model,
            voice: VoiceState::default(),
            screen_context: None,
            workspace_context: Vec::new(),
            messages: Vec::new(),
            active_run: None,
        }
    }

    pub fn transition_to(&mut self, status: SessionStatus) {
        self.status = status;
    }
}
