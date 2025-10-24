use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BridgeCommand {
    Create,
    Destroy,
    SetParam { id: String, value: f32 },
    Process,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BridgeEvent {
    Created,
    Destroyed,
    ParameterUpdated { id: String, value: f32 },
    Processed,
}

pub fn command_channel() -> (Sender<BridgeCommand>, Receiver<BridgeCommand>) {
    crossbeam_channel::unbounded()
}

pub fn event_channel() -> (Sender<BridgeEvent>, Receiver<BridgeEvent>) {
    crossbeam_channel::unbounded()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_transfer_messages() {
        let (tx, rx) = command_channel();
        tx.send(BridgeCommand::Create).unwrap();
        assert_eq!(rx.recv().unwrap(), BridgeCommand::Create);
    }
}
