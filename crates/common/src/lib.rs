pub mod actions;
pub mod config;
pub mod error;
pub mod intent;
pub mod ipc;
pub mod portal;
pub mod router;

pub use actions::{
    builtin_action_specs, find_action_spec, ActionPermission, ActionSpec, RiskLevel,
};
pub use config::AppConfig;
pub use error::{AppError, AppResult};
pub use intent::{IntentDetection, IntentKind};
pub use ipc::{
    read_message, write_message, Action, ApplicationLaunchJob, CaptureJob, HealthCheckJob,
    JobResult, SessionType, VisionRequest, VoiceSearchJob,
};
pub use portal::{
    current_desktops, screenshot_portal_backends_for_current_desktop, summarize_portal_backends,
    PortalBackendDescriptor,
};
pub use router::{AgentDecision, ProposedAction};
