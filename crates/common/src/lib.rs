pub mod actions;
pub mod capture_discovery;
pub mod config;
pub mod error;
pub mod intent;
pub mod ipc;
pub mod portal;
pub mod router;

pub use actions::{
    builtin_action_specs, find_action_spec, ActionPermission, ActionSpec, RiskLevel,
};
pub use capture_discovery::{
    discover_capture_backends, discover_rendered_capture_backends,
    likely_gnome_shell_screenshot_available, summarize_capture_backends, CaptureBackendDescriptor,
    CaptureBackendKind,
};
pub use config::AppConfig;
pub use error::{AppError, AppResult};
pub use intent::{IntentDetection, IntentKind};
pub use ipc::{
    read_message, write_message, Action, ApplicationLaunchJob, CaptureJob, HealthCheckJob,
    JobResult, ReplCommandJob, ReplEventStreamJob, ReplEventsJob, ReplSessionSnapshotJob,
    SessionType, UrlOpenJob, VisionRequest, VoiceSearchJob,
};
pub use portal::{
    current_desktops, screenshot_portal_backends_for_current_desktop, summarize_portal_backends,
    PortalBackendDescriptor,
};
pub use router::{AgentDecision, ProposedAction};

pub use coddy_core::{
    resolve_voice_turn_intent, ContextPolicy, ModelRef, ReplCommand, ReplEvent, ReplEventBroker,
    ReplEventEnvelope, ReplEventLog, ReplEventSubscription, ReplIntent, ReplMessage, ReplMode,
    ReplSession, ReplSessionSnapshot, SearchResultContext, SessionStatus, ShortcutConflictPolicy,
    ShortcutDecision, ToolStatus, VoiceTurnIntent,
};
