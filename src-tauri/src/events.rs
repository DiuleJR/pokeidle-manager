use crate::protocol::{BattleEvent, ServerFrame};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum CoreEvent {
    Connected,
    Disconnected(String),
    Welcome,
    StateUpdated,
    Battle(BattleEvent),
    UnknownFrame(String),
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<CoreEvent>,
}
impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.sender.subscribe()
    }
    pub fn publish(&self, event: CoreEvent) {
        if self.sender.send(event).is_err() {
            tracing::trace!("event without consumers");
        }
    }
    pub fn from_frame(frame: &ServerFrame) -> Vec<CoreEvent> {
        match frame {
            ServerFrame::Welcome(_) => vec![CoreEvent::Welcome],
            ServerFrame::State(_) => vec![CoreEvent::StateUpdated],
            ServerFrame::Battle(battle) => battle
                .events
                .iter()
                .cloned()
                .map(CoreEvent::Battle)
                .collect(),
            ServerFrame::Unknown { frame_type, .. } => {
                vec![CoreEvent::UnknownFrame(frame_type.clone())]
            }
            _ => Vec::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn publishes_events() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();
        bus.publish(CoreEvent::Connected);
        assert!(matches!(receiver.recv().await, Ok(CoreEvent::Connected)));
    }
}
