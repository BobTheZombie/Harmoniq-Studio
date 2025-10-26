use harmoniq_engine::{reset_rt_allocation_count, rt_allocation_count};

#[test]
fn no_alloc_in_rt_path() {
    let mut engine = harmoniq_engine::engine::Engine::new(48_000, 128, 256);
    let frames = 128u32;
    let mut input = vec![0.0f32; (frames as usize) * 2];
    let mut output = vec![0.0f32; (frames as usize) * 2];

    reset_rt_allocation_count();
    let before = rt_allocation_count();

    unsafe {
        for _ in 0..64 {
            harmoniq_engine::sched::executor::process_block(
                &mut engine as *mut _,
                input.as_ptr(),
                output.as_mut_ptr(),
                frames,
            );
        }
    }

    let after = rt_allocation_count();
    assert_eq!(before, after, "allocations detected during RT processing");
}
