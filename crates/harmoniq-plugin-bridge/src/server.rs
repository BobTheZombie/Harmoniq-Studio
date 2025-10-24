use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use crate::audio_ring::AudioRing;
use crate::ipc::{BridgeCommand, BridgeEvent};

#[derive(Debug)]
pub struct BridgeServer {
    audio_ring: Arc<Mutex<AudioRing>>,
}

impl BridgeServer {
    pub fn new(buffer_frames: usize) -> Self {
        Self {
            audio_ring: Arc::new(Mutex::new(AudioRing::with_capacity(buffer_frames))),
        }
    }

    pub fn run(
        &self,
        commands: Receiver<BridgeCommand>,
        events: Sender<BridgeEvent>,
    ) -> thread::JoinHandle<()> {
        let audio_ring = Arc::clone(&self.audio_ring);
        thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                match command {
                    BridgeCommand::Create => {
                        let _ = events.send(BridgeEvent::Created);
                    }
                    BridgeCommand::Destroy => {
                        let _ = events.send(BridgeEvent::Destroyed);
                        break;
                    }
                    BridgeCommand::SetParam { id, value } => {
                        let _ = events.send(BridgeEvent::ParameterUpdated { id, value });
                    }
                    BridgeCommand::Process => {
                        {
                            let mut ring = audio_ring.lock();
                            ring.push_frame(0.0, 0.0);
                        }
                        thread::sleep(Duration::from_millis(1));
                        let _ = events.send(BridgeEvent::Processed);
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{command_channel, event_channel};

    #[test]
    fn processing_cycle_emits_events() {
        let server = BridgeServer::new(16);
        let (cmd_tx, cmd_rx) = command_channel();
        let (evt_tx, evt_rx) = event_channel();
        let handle = server.run(cmd_rx, evt_tx);
        cmd_tx.send(BridgeCommand::Create).unwrap();
        cmd_tx.send(BridgeCommand::Process).unwrap();
        cmd_tx.send(BridgeCommand::Destroy).unwrap();
        drop(cmd_tx);
        let events: Vec<_> = evt_rx.iter().collect();
        handle.join().unwrap();
        assert!(events
            .iter()
            .any(|evt| matches!(evt, BridgeEvent::Processed)));
    }
}
