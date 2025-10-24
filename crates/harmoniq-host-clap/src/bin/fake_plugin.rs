use std::env;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use harmoniq_host_clap::ring::SharedAudioRingDescriptor;
use rand::Rng;

fn main() -> Result<()> {
    let behavior = env::var("HARMONIQ_FAKE_PLUGIN_BEHAVIOR").unwrap_or_else(|_| "idle".to_string());
    if let (Ok(path), Ok(frames), Ok(channels)) = (
        env::var("HARMONIQ_PLUGIN_RING_PATH"),
        env::var("HARMONIQ_PLUGIN_RING_FRAMES"),
        env::var("HARMONIQ_PLUGIN_RING_CHANNELS"),
    ) {
        let descriptor = SharedAudioRingDescriptor {
            path: path.into(),
            frames: frames.parse().unwrap_or(0),
            channels: channels.parse().unwrap_or(0),
        };
        if descriptor.frames > 0 && descriptor.channels > 0 {
            let mut rng = rand::thread_rng();
            let mut buffer =
                vec![0.0f32; descriptor.frames as usize * descriptor.channels as usize];
            for sample in &mut buffer {
                *sample = rng.gen_range(-1.0..1.0);
            }
            let _ = descriptor.write_block(&buffer);
        }
    }

    match behavior.as_str() {
        "exit" => Ok(()),
        "crash" => panic!("fake plugin crash requested"),
        _ => loop {
            thread::sleep(Duration::from_millis(100));
        },
    }
}
