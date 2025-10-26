use crate::ipc::bus::EngineEventBus;
use crate::rt::metrics::BlockStat;
use crate::{reset_rt_allocation_count, rt_allocation_count};

#[test]
fn rt_writer_does_not_allocate() {
    reset_rt_allocation_count();
    let (_bus, mut writer, mut reader) = EngineEventBus::new(2048);
    for _ in 0..10_000 {
        writer.push_block_stat(BlockStat {
            ns: 512,
            frames: 128,
            xruns: 0,
        });
    }
    assert_eq!(rt_allocation_count(), 0);
    let mut out = Vec::new();
    reader.drain_block_stats(&mut out);
    assert!(!out.is_empty());
}
