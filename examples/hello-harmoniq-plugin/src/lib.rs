//! Minimal Harmoniq plugin that applies a gain.

use harmoniq_plugin_sdk::{descriptor_ptr, HqHostCallbacks, HqPluginDescriptor, HQ_API_VERSION};

static DESCRIPTOR: HqPluginDescriptor = HqPluginDescriptor {
    abi_version: HQ_API_VERSION,
    name: crate::util::str_to_fixed::<64>("Hello Gain"),
    vendor: crate::util::str_to_fixed::<64>("Harmoniq"),
    audio_inputs: 2,
    audio_outputs: 2,
    parameters: 1,
};

#[no_mangle]
pub extern "C" fn harmoniq_plugin_entry(
    _host: *const HqHostCallbacks,
) -> *const HqPluginDescriptor {
    descriptor_ptr(&DESCRIPTOR)
}

mod util {
    pub const fn str_to_fixed<const N: usize>(value: &str) -> [u8; N] {
        let bytes = value.as_bytes();
        let mut buffer = [0u8; N];
        let mut i = 0;
        while i < bytes.len() && i < N {
            buffer[i] = bytes[i];
            i += 1;
        }
        buffer
    }
}
