use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ShortcutConflictPolicy {
    IgnoreWhileBusy,
    StopSpeakingAndListen,
    CancelRunAndListen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShortcutDecision {
    StartListening {
        run_id: Uuid,
    },
    IgnoredBusy {
        active_run_id: Uuid,
    },
    StoppedSpeaking {
        previous_run_id: Uuid,
        next_run_id: Uuid,
    },
    CancelledRun {
        previous_run_id: Uuid,
        next_run_id: Uuid,
    },
    Failed {
        reason: String,
    },
}

impl ShortcutDecision {
    pub fn starts_listening(&self) -> bool {
        matches!(
            self,
            ShortcutDecision::StartListening { .. }
                | ShortcutDecision::StoppedSpeaking { .. }
                | ShortcutDecision::CancelledRun { .. }
        )
    }
}
