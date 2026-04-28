use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelRef {
    pub provider: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ModelRole {
    Chat,
    Ocr,
    Asr,
    Tts,
    Embedding,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContextPolicy {
    NoScreen,
    VisibleScreen,
    WorkspaceOnly,
    ScreenAndWorkspace,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ScreenAssistMode {
    ExplainVisibleScreen,
    ExplainCode,
    DebugError,
    MultipleChoice,
    SummarizeDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplCommand {
    Ask {
        text: String,
        context_policy: ContextPolicy,
    },
    CaptureAndExplain {
        mode: ScreenAssistMode,
        policy: crate::AssessmentPolicy,
    },
    VoiceTurn {
        transcript_override: Option<String>,
    },
    OpenUi {
        mode: crate::ReplMode,
    },
    SelectModel {
        model: ModelRef,
        role: ModelRole,
    },
    DismissConfirmation,
    StopActiveRun,
    StopSpeaking,
}
