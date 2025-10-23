use harmoniq_dsp::biquad::Svf;

#[test]
fn svf_lp_stability() {
    let mut filter = Svf::lowpass(48_000.0, 1_000.0, 0.707);
    let mut y = 0.0;
    for _ in 0..10_000 {
        y = filter.process(1.0);
    }
    assert!(y.is_finite());
}
