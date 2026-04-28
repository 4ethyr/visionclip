use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ReplEvent, ReplSession};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplEventEnvelope {
    pub sequence: u64,
    pub session_id: Uuid,
    pub run_id: Option<Uuid>,
    pub captured_at_unix_ms: u64,
    pub event: ReplEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplSessionSnapshot {
    pub session: ReplSession,
    pub last_sequence: u64,
}

#[derive(Debug, Clone)]
pub struct ReplEventLog {
    session_id: Uuid,
    events: Vec<ReplEventEnvelope>,
    next_sequence: u64,
}

impl ReplEventEnvelope {
    pub fn new(
        sequence: u64,
        session_id: Uuid,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
        event: ReplEvent,
    ) -> Self {
        Self {
            sequence,
            session_id,
            run_id,
            captured_at_unix_ms,
            event,
        }
    }
}

impl ReplEventLog {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            events: Vec::new(),
            next_sequence: 1,
        }
    }

    pub fn append(
        &mut self,
        event: ReplEvent,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        let envelope = ReplEventEnvelope::new(
            self.next_sequence,
            self.session_id,
            run_id,
            captured_at_unix_ms,
            event,
        );
        self.next_sequence += 1;
        self.events.push(envelope.clone());
        envelope
    }

    pub fn events_after(&self, sequence: u64) -> Vec<ReplEventEnvelope> {
        self.events
            .iter()
            .filter(|event| event.sequence > sequence)
            .cloned()
            .collect()
    }

    pub fn last_sequence(&self) -> u64 {
        self.next_sequence.saturating_sub(1)
    }

    pub fn replay(&self, mut session: ReplSession) -> ReplSession {
        for envelope in &self.events {
            session.apply_event(&envelope.event);
        }
        session
    }

    pub fn snapshot(&self, session: ReplSession) -> ReplSessionSnapshot {
        ReplSessionSnapshot {
            session: self.replay(session),
            last_sequence: self.last_sequence(),
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AssessmentPolicy, ModelRef, ReplIntent, ReplMessage, ReplMode, SessionStatus,
        ShortcutSource, ToolStatus,
    };

    #[test]
    fn event_log_sequences_events_and_replays_session_state() {
        let selected_model = ModelRef {
            provider: "ollama".to_string(),
            name: "gemma4-e2b".to_string(),
        };
        let mut session = ReplSession::new(ReplMode::FloatingTerminal, selected_model);
        let session_id = session.id;
        let run_id = Uuid::new_v4();
        let mut log = ReplEventLog::new(session_id);

        log.append(
            ReplEvent::SessionStarted { session_id },
            None,
            1_775_000_000_000,
        );
        log.append(
            ReplEvent::ShortcutTriggered {
                binding: "Shift+CapsLk".to_string(),
                source: ShortcutSource::GnomeMediaKeys,
            },
            None,
            1_775_000_000_001,
        );
        log.append(
            ReplEvent::OverlayShown {
                mode: ReplMode::FloatingTerminal,
            },
            None,
            1_775_000_000_002,
        );
        log.append(ReplEvent::VoiceListeningStarted, None, 1_775_000_000_003);
        log.append(
            ReplEvent::VoiceTranscriptFinal {
                text: "Quem foi Rousseau?".to_string(),
            },
            None,
            1_775_000_000_004,
        );
        log.append(
            ReplEvent::IntentDetected {
                intent: ReplIntent::SearchDocs,
                confidence: 0.82,
            },
            Some(run_id),
            1_775_000_000_005,
        );
        log.append(
            ReplEvent::RunStarted { run_id },
            Some(run_id),
            1_775_000_000_006,
        );
        log.append(
            ReplEvent::TokenDelta {
                run_id,
                text: "Jean-Jacques Rousseau foi".to_string(),
            },
            Some(run_id),
            1_775_000_000_007,
        );
        log.append(
            ReplEvent::MessageAppended {
                message: ReplMessage {
                    id: Uuid::new_v4(),
                    role: "assistant".to_string(),
                    text: "Jean-Jacques Rousseau foi um filósofo iluminista.".to_string(),
                },
            },
            Some(run_id),
            1_775_000_000_008,
        );
        log.append(
            ReplEvent::RunCompleted { run_id },
            Some(run_id),
            1_775_000_000_009,
        );

        session = log.replay(session);

        assert_eq!(log.last_sequence(), 10);
        assert_eq!(session.status, SessionStatus::Idle);
        assert_eq!(session.active_run, None);
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, "assistant");
    }

    #[test]
    fn event_log_returns_incremental_events_after_sequence() {
        let mut log = ReplEventLog::new(Uuid::new_v4());

        let first = log.append(ReplEvent::VoiceListeningStarted, None, 10);
        let second = log.append(
            ReplEvent::VoiceTranscriptPartial {
                text: "terminal".to_string(),
            },
            None,
            20,
        );

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert_eq!(log.events_after(1), vec![second]);
        assert!(log.events_after(2).is_empty());
    }

    #[test]
    fn snapshot_includes_last_sequence_and_reduced_session() {
        let selected_model = ModelRef {
            provider: "ollama".to_string(),
            name: "gemma4-e2b".to_string(),
        };
        let session = ReplSession::new(ReplMode::DesktopApp, selected_model);
        let mut log = ReplEventLog::new(session.id);

        log.append(
            ReplEvent::PolicyEvaluated {
                policy: AssessmentPolicy::Practice,
                allowed: true,
            },
            None,
            1,
        );
        log.append(
            ReplEvent::ToolCompleted {
                name: "search_web".to_string(),
                status: ToolStatus::Succeeded,
            },
            None,
            2,
        );

        let snapshot = log.snapshot(session);

        assert_eq!(snapshot.last_sequence, 2);
        assert_eq!(snapshot.session.policy, AssessmentPolicy::Practice);
        assert_eq!(snapshot.session.status, SessionStatus::Thinking);
    }
}
