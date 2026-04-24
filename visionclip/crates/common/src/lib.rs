pub mod config;
pub mod error;
pub mod ipc;

pub use config::AppConfig;
pub use error::{AppError, AppResult};
pub use ipc::{read_message, write_message, Action, CaptureJob, JobResult, SessionType};
