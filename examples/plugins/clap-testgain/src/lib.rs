//! A minimal gain plugin used for integration tests.

#[no_mangle]
pub extern "C" fn clap_entry() -> i32 {
    // The real implementation would interact with the CLAP API.
    // We simply return a fixed value to signal "success" for tests.
    0xCA_FE_BA_BE as i32
}

#[cfg(test)]
mod tests {
    #[test]
    fn entry_returns_magic() {
        assert_eq!(super::clap_entry(), 0xCA_FE_BA_BE as i32);
    }
}
