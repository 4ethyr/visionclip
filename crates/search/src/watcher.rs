#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WatcherState {
    Disabled,
    Ready,
    Overflowed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WatcherStatus {
    pub state: WatcherState,
    pub watched_roots: usize,
    pub watched_dirs: usize,
}

impl Default for WatcherStatus {
    fn default() -> Self {
        Self {
            state: WatcherState::Disabled,
            watched_roots: 0,
            watched_dirs: 0,
        }
    }
}
