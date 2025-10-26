#[test]
fn pdc_alignment_unchanged_parallel() {
    let mut engine = harmoniq_engine::engine::Engine::new(48_000, 128, 256);
    engine.rebuild();

    assert_eq!(engine.graph.parallel_safe.len(), engine.graph.nodes.len());
    assert!(!engine.graph.depths.is_empty());

    let start_pos = engine.sample_pos;
    let frames = 128u32;
    let mut buffer = vec![0.0f32; frames as usize * 2];

    unsafe {
        harmoniq_engine::sched::executor::process_block(
            &mut engine as *mut _,
            core::ptr::null(),
            buffer.as_mut_ptr(),
            frames,
        );
    }

    assert_eq!(engine.sample_pos, start_pos + frames as u64);
}
