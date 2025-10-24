#![cfg(all(feature = "openasio", target_os = "linux"))]

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use harmoniq_engine::backend::openasio::OpenAsioBackend;
use harmoniq_engine::backend::{EngineRt, StreamConfig};
use harmoniq_engine::buffers::{AudioView, AudioViewMut};

struct NullRt {
    blocks: AtomicUsize,
    target: usize,
    signal: mpsc::Sender<()>,
}

impl NullRt {
    fn new(target: usize, signal: mpsc::Sender<()>) -> Self {
        Self {
            blocks: AtomicUsize::new(0),
            target: target.max(1),
            signal,
        }
    }
}

impl EngineRt for NullRt {
    fn process(
        &mut self,
        _inputs: AudioView<'_>,
        mut outputs: AudioViewMut<'_>,
        frames: u32,
    ) -> bool {
        if let Some(out) = outputs.interleaved_mut() {
            out.fill(0.0);
        } else if let Some(mut planar) = outputs.planar() {
            let frames_available = planar.frames();
            for &plane_ptr in planar.planes().iter() {
                if plane_ptr.is_null() {
                    continue;
                }
                for idx in 0..frames_available {
                    unsafe {
                        *plane_ptr.add(idx) = 0.0;
                    }
                }
            }
        }

        let processed = self.blocks.fetch_add(1, Ordering::SeqCst) + 1;
        if processed >= self.target {
            let _ = self.signal.send(());
        }
        true
    }
}

fn find_driver() -> Option<String> {
    if let Ok(path) = std::env::var("OPENASIO_TEST_DRIVER") {
        if Path::new(&path).exists() {
            return Some(path);
        }
    }

    let candidates = [
        "target/debug/libopenasio_driver_cpal.so",
        "target/release/libopenasio_driver_cpal.so",
    ];
    for candidate in candidates {
        if Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

#[test]
fn openasio_cpal_headless_blocks() {
    let Some(driver_path) = find_driver() else {
        eprintln!("OpenASIO CPAL test driver not found; skipping smoke test");
        return;
    };

    let desired = StreamConfig::new(48_000, 128, 0, 2, true);
    let (tx, rx) = mpsc::channel();
    let mut backend = OpenAsioBackend::new(driver_path, None, desired);
    backend
        .start(Box::new(NullRt::new(16, tx)))
        .expect("start OpenASIO backend");

    let received = rx.recv_timeout(Duration::from_secs(3));
    backend.stop();

    assert!(
        received.is_ok(),
        "OpenASIO backend failed to deliver callbacks in time"
    );
}
