//! Minimal VST3-style gain plugin shim for tests.

#[no_mangle]
pub extern "C" fn vst3_main() -> i32 {
    // Returning a fixed signature to emulate a successful VST3 entry point.
    0x57_33_74_33
}

#[cfg(test)]
mod tests {
    #[test]
    fn vst3_entry_returns_signature() {
        assert_eq!(super::vst3_main(), 0x57_33_74_33);
    }
}
