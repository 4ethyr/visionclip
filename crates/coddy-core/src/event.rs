use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ShortcutSource {
    GnomeMediaKeys,
    TauriGlobalShortcut,
    Cli,
    SystemdUserService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ReplIntent {
    AskTechnicalQuestion,
    ExplainScreen,
    ExplainCode,
    DebugCode,
    SolvePracticeQuestion,
    MultipleChoiceAssist,
    GenerateTestCases,
    ExplainTerminalError,
    SearchDocs,
    OpenApplication,
    OpenWebsite,
    ConfigureModel,
    ManageContext,
    AgenticCodeChange,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolStatus {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReplEvent {
    SessionStarted {
        session_id: Uuid,
    },
    ShortcutTriggered {
        binding: String,
        source: ShortcutSource,
    },
    OverlayShown {
        mode: crate::ReplMode,
    },
    VoiceListeningStarted,
    VoiceTranscriptPartial {
        text: String,
    },
    VoiceTranscriptFinal {
        text: String,
    },
    ScreenCaptured {
        source: crate::ExtractionSource,
        bytes: usize,
    },
    OcrCompleted {
        chars: usize,
        language_hint: Option<String>,
    },
    IntentDetected {
        intent: ReplIntent,
        confidence: f32,
    },
    PolicyEvaluated {
        policy: crate::AssessmentPolicy,
        allowed: bool,
    },
    ModelSelected {
        model: String,
    },
    SearchStarted {
        query: String,
        provider: String,
    },
    SearchContextExtracted {
        provider: String,
        organic_results: usize,
        ai_overview_present: bool,
    },
    TokenDelta {
        run_id: Uuid,
        text: String,
    },
    ToolStarted {
        name: String,
    },
    ToolCompleted {
        name: String,
        status: ToolStatus,
    },
    TtsQueued,
    TtsStarted,
    TtsCompleted,
    RunCompleted {
        run_id: Uuid,
    },
    Error {
        code: String,
        message: String,
    },
}
