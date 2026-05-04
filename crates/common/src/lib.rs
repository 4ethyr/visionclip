pub mod actions;
pub mod agent;
pub mod audit;
pub mod capture_discovery;
pub mod config;
pub mod error;
pub mod intent;
pub mod ipc;
pub mod language;
pub mod portal;
pub mod router;
pub mod security;
pub mod session;
pub mod status;
pub mod tools;

pub use actions::{
    builtin_action_specs, find_action_spec, ActionPermission, ActionSpec, ConfirmationPolicy,
    RiskLevel,
};
pub use agent::{AgentOrchestrator, AgentTurn, AssistantMessage, SecurityRefusal, UserInput};
pub use audit::{redact_for_audit, AuditEvent, AuditLog};
pub use capture_discovery::{
    discover_capture_backends, discover_rendered_capture_backends,
    likely_gnome_shell_screenshot_available, summarize_capture_backends, CaptureBackendDescriptor,
    CaptureBackendKind,
};
pub use config::AppConfig;
pub use error::{AppError, AppResult};
pub use intent::{IntentDetection, IntentKind};
pub use ipc::{
    decode_message_payload, encode_message_payload, read_message, read_message_payload,
    write_message, Action, ApplicationLaunchJob, CaptureJob, DocumentAskJob, DocumentControlJob,
    DocumentControlKind, DocumentIngestJob, DocumentOpenJob, DocumentReadJob, DocumentSummarizeJob,
    DocumentTranslateJob, HealthCheckJob, JobResult, OpenAction, SearchControlRequest,
    SearchDiagnostics, SearchHit, SearchHitSource, SearchMode, SearchOpenRequest, SearchRequest,
    SearchResponse, SessionType, UrlOpenJob, VisionRequest, VoiceSearchJob,
};
pub use language::{normalize_latin_for_language, AssistantLanguage};
pub use portal::{
    current_desktops, screenshot_portal_backends_for_current_desktop, summarize_portal_backends,
    PortalBackendDescriptor,
};
pub use router::{AgentDecision, ProposedAction};
pub use security::{
    ConfirmationRequest, PermissionEngine, PolicyDecision, PolicyInput, RiskContext, RuntimePolicy,
    SecurityReason,
};
pub use session::{
    AgentContext, ConversationMessage, DocumentContext, MessageRole, SessionId, SessionManager,
    SessionState,
};
pub use status::{
    assistant_status_path, write_assistant_status, AssistantStatusKind, AssistantStatusSnapshot,
};
pub use tools::{ToolCall, ToolDefinition, ToolRegistry, ToolResult, ToolValidationError};
