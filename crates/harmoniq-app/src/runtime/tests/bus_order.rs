use std::thread;
use std::time::Duration;

use super::super::bus::{channel, UiCommand};

#[test]
fn fifo_delivery_preserved() {
    let (mut tx, mut rx) = channel(16);
    let expected: Vec<UiCommand> = (0..10)
        .map(|idx| UiCommand::AddTrack {
            name: format!("Track {idx}"),
        })
        .collect();
    for cmd in &expected {
        tx.try_send(cmd.clone()).expect("send");
    }
    let mut received = Vec::new();
    while let Some(cmd) = rx.try_recv() {
        received.push(cmd);
    }
    assert_eq!(received.len(), expected.len());
    for (expected, received) in expected.iter().zip(received.iter()) {
        assert!(matches!(
            (expected, received),
            (
                UiCommand::AddTrack { name: lhs },
                UiCommand::AddTrack { name: rhs },
            ) if lhs == rhs
        ));
    }
}

#[test]
fn no_drops_under_load() {
    let (mut tx, mut rx) = channel(32);
    let producer = thread::spawn(move || {
        for idx in 0..1000 {
            let mut retries = 0;
            loop {
                if tx
                    .try_send(UiCommand::AddTrack {
                        name: format!("Track {idx}"),
                    })
                    .is_ok()
                {
                    break;
                }
                retries += 1;
                if retries > 10 {
                    panic!("unable to enqueue command");
                }
                thread::sleep(Duration::from_micros(50));
            }
        }
    });
    producer.join().expect("producer thread");

    let mut count = 0;
    while let Some(_cmd) = rx.try_recv() {
        count += 1;
    }
    assert_eq!(count, 1000);
}
