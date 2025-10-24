use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use parking_lot::Mutex;

use harmoniq_host_vst3::adapter::{AdapterDescriptor, AdapterKind, SandboxRequest};
use harmoniq_host_vst3::host::{SandboxBroker, Vst3HostBuilder};
use harmoniq_host_vst3::ipc::{BrokerCommand, BrokerEvent, RtMessageKind};
use harmoniq_host_vst3::pdc::PdcEvent;
use harmoniq_host_vst3::ring::SharedAudioRing;
use harmoniq_host_vst3::window::WindowEmbedder;

struct MockBroker {
    commands: Arc<Mutex<Vec<BrokerCommand>>>,
    events: Arc<Mutex<VecDeque<BrokerEvent>>>,
    ring: SharedAudioRing,
}

impl MockBroker {
    fn new() -> Self {
        Self {
            commands: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(VecDeque::new())),
            ring: SharedAudioRing::create(32, 2).expect("failed to create shared ring"),
        }
    }

    fn command_log(&self) -> Arc<Mutex<Vec<BrokerCommand>>> {
        Arc::clone(&self.commands)
    }

    fn event_queue(&self) -> Arc<Mutex<VecDeque<BrokerEvent>>> {
        Arc::clone(&self.events)
    }
}

impl SandboxBroker for MockBroker {
    fn audio_ring(&self) -> &SharedAudioRing {
        &self.ring
    }

    fn audio_ring_mut(&mut self) -> &mut SharedAudioRing {
        &mut self.ring
    }

    fn load_plugin(&mut self, request: SandboxRequest) -> Result<()> {
        self.commands.lock().push(BrokerCommand::LoadPlugin {
            request,
            audio_ring: self.ring.descriptor().clone(),
        });
        Ok(())
    }

    fn process_block(&mut self, frames: u32) -> Result<()> {
        self.commands
            .lock()
            .push(BrokerCommand::ProcessBlock { frames });
        Ok(())
    }

    fn request_state_dump(&mut self) -> Result<()> {
        self.commands.lock().push(BrokerCommand::RequestState);
        Ok(())
    }

    fn request_preset_dump(&mut self) -> Result<()> {
        self.commands.lock().push(BrokerCommand::RequestPresetDump);
        Ok(())
    }

    fn register_rt_channel(&mut self) -> Result<()> {
        self.commands.lock().push(BrokerCommand::RegisterRtChannel);
        Ok(())
    }

    fn kill_plugin(&mut self) -> Result<()> {
        self.commands.lock().push(BrokerCommand::KillPlugin);
        Ok(())
    }

    fn try_next_event(&mut self) -> Option<BrokerEvent> {
        self.events.lock().pop_front()
    }

    fn recv_event(&mut self, _timeout: Duration) -> Option<BrokerEvent> {
        self.try_next_event()
    }
}

#[derive(Default)]
struct MockEmbedder {
    attached: Arc<Mutex<Vec<u64>>>,
    detach_count: Arc<Mutex<u32>>,
}

impl MockEmbedder {
    fn attach_log(&self) -> Arc<Mutex<Vec<u64>>> {
        Arc::clone(&self.attached)
    }

    #[allow(dead_code)]
    fn detach_count(&self) -> Arc<Mutex<u32>> {
        Arc::clone(&self.detach_count)
    }
}

impl WindowEmbedder for MockEmbedder {
    fn attach(&self, window_id: u64) -> Result<()> {
        self.attached.lock().push(window_id);
        Ok(())
    }

    fn detach(&self) -> Result<()> {
        *self.detach_count.lock() += 1;
        Ok(())
    }
}

#[test]
fn load_plugin_forwards_adapter_descriptor() {
    let broker = MockBroker::new();
    let log = broker.command_log();
    let adapter = AdapterDescriptor {
        kind: AdapterKind::OpenVst3Shim,
        library_path: Some("/tmp/openvst3.so".into()),
        extra_args: vec!["--mock".into()],
    };

    let mut host = Vst3HostBuilder::new()
        .adapter(adapter.clone())
        .build_with_broker(broker);

    host.load_plugin("/plugins/test.vst3").unwrap();

    let commands = log.lock();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        BrokerCommand::LoadPlugin { request, .. } => {
            assert_eq!(request.adapter, adapter);
            assert_eq!(
                request.plugin_path,
                std::path::Path::new("/plugins/test.vst3")
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn preset_and_state_events_populate_cache() {
    let broker = MockBroker::new();
    let log = broker.command_log();
    let events = broker.event_queue();
    let mut host = Vst3HostBuilder::new().build_with_broker(broker);

    host.request_state_dump().unwrap();
    host.request_preset_dump().unwrap();

    let commands = log.lock();
    assert!(commands.contains(&BrokerCommand::RequestState));
    assert!(commands.contains(&BrokerCommand::RequestPresetDump));
    drop(commands);

    events.lock().push_back(BrokerEvent::StateDump {
        data: vec![1, 2, 3],
    });
    events.lock().push_back(BrokerEvent::PresetDump {
        data: vec![4, 5, 6],
    });
    host.drain_events();

    assert_eq!(host.latest_state(), Some(&[1, 2, 3][..]));
    assert_eq!(host.latest_preset(), Some(&[4, 5, 6][..]));

    let history: Vec<_> = host.pdc_history().cloned().collect();
    assert_eq!(
        history,
        vec![
            PdcEvent::State(vec![1, 2, 3]),
            PdcEvent::Preset(vec![4, 5, 6]),
        ]
    );
}

#[test]
fn latency_events_update_rt_channel() {
    let broker = MockBroker::new();
    let events = broker.event_queue();
    let mut host = Vst3HostBuilder::new().build_with_broker(broker);

    let channel = host.register_rt_channel().unwrap();
    events
        .lock()
        .push_back(BrokerEvent::LatencyReported { samples: 256 });
    host.drain_events();

    assert_eq!(host.latency_samples(), 256);
    let msg = channel.receiver().try_recv().expect("latency message");
    assert!(matches!(
        msg.kind,
        RtMessageKind::LatencyChanged { samples: 256 }
    ));
}

#[test]
fn editor_host_attaches_embedder() {
    let broker = MockBroker::new();
    let events = broker.event_queue();
    let mut host = Vst3HostBuilder::new().build_with_broker(broker);
    let embedder = MockEmbedder::default();
    let log = embedder.attach_log();

    events
        .lock()
        .push_back(BrokerEvent::EditorWindowCreated { window_id: 4242 });
    host.drain_events();

    host.attach_editor(&embedder).unwrap();

    let attach_log = log.lock();
    assert_eq!(attach_log.as_slice(), &[4242]);
}
