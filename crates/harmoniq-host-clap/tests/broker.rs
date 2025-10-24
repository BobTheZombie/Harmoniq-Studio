use std::path::PathBuf;
use std::time::Duration;

use harmoniq_host_clap::host::{ClapHost, HostOptions};
use harmoniq_host_clap::ipc::BrokerEvent;
use harmoniq_host_clap::ring::SharedAudioRing;

fn broker_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_harmoniq-host-clap-broker"))
}

fn fake_plugin_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_harmoniq-host-clap-fake-plugin"))
}

#[test]
fn shared_ring_roundtrip() {
    let mut ring = SharedAudioRing::create(32, 2).expect("ring");
    let mut data = vec![0.0f32; 64];
    for (i, sample) in data.iter_mut().enumerate() {
        *sample = i as f32;
    }
    ring.write_block(&data).expect("write");
    let (copy, _) = ring.descriptor().read_latest_block().expect("read");
    assert_eq!(copy.len(), data.len());
}

#[test]
fn launch_fake_plugin_and_kill() {
    let mut options = HostOptions::default();
    options.broker.executable = broker_executable();
    options.broker.frames = 64;
    options.broker.channels = 2;
    options.event_timeout = Duration::from_secs(1);

    let fake_plugin = fake_plugin_executable();

    let mut host = ClapHost::new(options).expect("host");
    host.load_plugin_path(&fake_plugin)
        .expect("load plugin path");

    assert!(
        wait_for_event(&mut host, |event| matches!(
            event,
            BrokerEvent::PluginLoaded { .. }
        )),
        "expected plugin loaded event"
    );

    host.process_audio(64).expect("process");
    let state = host.request_state().expect("state dump");
    assert!(!state.is_empty(), "state should not be empty");

    host.broker().kill_plugin().expect("kill plugin");
    assert!(
        wait_for_event(&mut host, |event| matches!(
            event,
            BrokerEvent::PluginCrashed { .. }
        )),
        "expected plugin crashed event"
    );
}

fn wait_for_event<F>(host: &mut ClapHost, mut predicate: F) -> bool
where
    F: FnMut(&BrokerEvent) -> bool,
{
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(50));
        let events = host.take_events();
        if events.iter().any(|event| predicate(event)) {
            return true;
        }
    }
    false
}
