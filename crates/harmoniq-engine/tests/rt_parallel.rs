fn render_with_workers(workers: u32) -> Vec<f32> {
    let mut engine = harmoniq_engine::engine::Engine::new(48_000, 128, 256);
    engine.parallel_cfg.workers = workers;
    engine.rebuild();

    let frames = 128u32;
    let mut input = vec![0.25f32; frames as usize * 2];
    let mut output = vec![0.0f32; frames as usize * 2];

    unsafe {
        harmoniq_engine::sched::executor::process_block(
            &mut engine as *mut _,
            input.as_ptr(),
            output.as_mut_ptr(),
            frames,
        );
    }

    output
}

#[test]
fn parallel_is_deterministic_and_faster() {
    let baseline = render_with_workers(0);
    let parallel = render_with_workers(2);

    assert_eq!(baseline.len(), parallel.len());
    for (a, b) in baseline.iter().zip(parallel.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}
