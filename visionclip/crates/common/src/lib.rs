pub mod config;
pub mod error;
pub mod ipc;
pub mod portal;

pub use config::AppConfig;
pub use error::{AppError, AppResult};
pub use ipc::{
    read_message, write_message, Action, CaptureJob, JobResult, SessionType, VisionRequest,
    VoiceSearchJob,
};
pub use portal::{
    current_desktops, screenshot_portal_backends_for_current_desktop, summarize_portal_backends,
    PortalBackendDescriptor,
};
