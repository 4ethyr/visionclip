use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{ReplEvent, ReplEventEnvelope, ReplEventLog, ReplSession, ReplSessionSnapshot};

#[derive(Debug)]
pub struct ReplEventBroker {
    log: ReplEventLog,
    sender: broadcast::Sender<ReplEventEnvelope>,
}

#[derive(Debug)]
pub struct ReplEventSubscription {
    replay: std::vec::IntoIter<ReplEventEnvelope>,
    receiver: broadcast::Receiver<ReplEventEnvelope>,
}

impl ReplEventBroker {
    pub fn new(session_id: Uuid, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            log: ReplEventLog::new(session_id),
            sender,
        }
    }

    pub fn publish(
        &mut self,
        event: ReplEvent,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        let envelope = self.log.append(event, run_id, captured_at_unix_ms);
        let _ = self.sender.send(envelope.clone());
        envelope
    }

    pub fn subscribe_after(&self, sequence: u64) -> ReplEventSubscription {
        ReplEventSubscription {
            replay: self.log.events_after(sequence).into_iter(),
            receiver: self.sender.subscribe(),
        }
    }

    pub fn events_after(&self, sequence: u64) -> Vec<ReplEventEnvelope> {
        self.log.events_after(sequence)
    }

    pub fn last_sequence(&self) -> u64 {
        self.log.last_sequence()
    }

    pub fn replay(&self, session: ReplSession) -> ReplSession {
        self.log.replay(session)
    }

    pub fn snapshot(&self, session: ReplSession) -> ReplSessionSnapshot {
        self.log.snapshot(session)
    }

    pub fn log(&self) -> &ReplEventLog {
        &self.log
    }
}

impl ReplEventSubscription {
    pub async fn next(&mut self) -> Option<ReplEventEnvelope> {
        if let Some(event) = self.replay.next() {
            return Some(event);
        }

        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscription_replays_history_before_live_events() {
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);

        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);
        broker.publish(ReplEvent::RunStarted { run_id }, Some(run_id), 20);

        let mut subscription = broker.subscribe_after(0);
        let first = subscription.next().await.expect("first replay event");
        let second = subscription.next().await.expect("second replay event");

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);

        broker.publish(ReplEvent::RunCompleted { run_id }, Some(run_id), 30);
        let third = subscription.next().await.expect("live event");

        assert_eq!(third.sequence, 3);
        assert!(matches!(third.event, ReplEvent::RunCompleted { .. }));
    }

    #[test]
    fn broker_exposes_incremental_history_and_last_sequence() {
        let session_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);

        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);
        broker.publish(
            ReplEvent::VoiceTranscriptFinal {
                text: "terminal".to_string(),
            },
            None,
            20,
        );

        let events = broker.events_after(1);

        assert_eq!(broker.last_sequence(), 2);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 2);
    }

    #[tokio::test]
    async fn subscription_without_replay_waits_for_live_events() {
        let session_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);
        let mut subscription = broker.subscribe_after(broker.last_sequence());

        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);

        let event =
            tokio::time::timeout(std::time::Duration::from_millis(100), subscription.next())
                .await
                .expect("live event before timeout")
                .expect("open subscription");

        assert_eq!(event.sequence, 1);
        assert!(matches!(event.event, ReplEvent::VoiceListeningStarted));
    }
}
