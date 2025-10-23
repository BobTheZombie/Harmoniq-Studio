//! Harmoniq plugin API definitions shared between the host and plugins.

use std::ffi::c_void;

/// Identifier describing the ABI version supported by a plugin.
pub const HQ_API_VERSION: u32 = 1;

/// Descriptor exported by every Harmoniq plugin.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HqPluginDescriptor {
    /// ABI version the plugin targets.
    pub abi_version: u32,
    /// Plugin name shown in the UI.
    pub name: [u8; 64],
    /// Vendor name.
    pub vendor: [u8; 64],
    /// Number of audio input channels.
    pub audio_inputs: u32,
    /// Number of audio output channels.
    pub audio_outputs: u32,
    /// Number of exposed parameters.
    pub parameters: u32,
}

impl HqPluginDescriptor {
    /// Creates a descriptor suitable for stubs and tests.
    pub fn stub() -> Self {
        let mut name = [0u8; 64];
        let mut vendor = [0u8; 64];
        name[..11].copy_from_slice(b"HQ Stub\0");
        vendor[..8].copy_from_slice(b"Harmoniq\0");
        Self {
            abi_version: HQ_API_VERSION,
            name,
            vendor,
            audio_inputs: 2,
            audio_outputs: 2,
            parameters: 0,
        }
    }
}

/// Audio buffer passed to plugin process callbacks.
#[repr(C)]
pub struct HqAudioBus {
    /// Pointer to channel pointers.
    pub channels: *mut *mut f32,
    /// Number of frames available.
    pub frames: u32,
}

/// Host callbacks available to plugins.
#[repr(C)]
pub struct HqHostCallbacks {
    /// Posts a parameter change back to the host.
    pub post_param_change: Option<extern "C" fn(param: u32, value: f32)>,
    /// Sends a MIDI event to the host.
    pub post_midi: Option<extern "C" fn(status: u8, data1: u8, data2: u8)>,
    /// User data pointer.
    pub user_data: *mut c_void,
}

/// Entry point symbol looked up by the host.
pub type EntryPoint =
    unsafe extern "C" fn(host: *const HqHostCallbacks) -> *const HqPluginDescriptor;

/// Convenience helper returning a pointer to a descriptor for FFI exports.
pub fn descriptor_ptr(descriptor: &'static HqPluginDescriptor) -> *const HqPluginDescriptor {
    descriptor as *const HqPluginDescriptor
}

/// Helper converting a string to a fixed-size array.
const fn str_to_fixed<const N: usize>(value: &str) -> [u8; N] {
    let bytes = value.as_bytes();
    let mut buffer = [0u8; N];
    let mut i = 0;
    while i < bytes.len() && i < N {
        buffer[i] = bytes[i];
        i += 1;
    }
    buffer
}

/// No-op default entry point useful for unit tests.
#[no_mangle]
pub extern "C" fn harmoniq_plugin_entry(
    _host: *const HqHostCallbacks,
) -> *const HqPluginDescriptor {
    static DESCRIPTOR: HqPluginDescriptor = HqPluginDescriptor {
        abi_version: HQ_API_VERSION,
        name: str_to_fixed::<64>("Harmoniq Test"),
        vendor: str_to_fixed::<64>("Harmoniq"),
        audio_inputs: 2,
        audio_outputs: 2,
        parameters: 0,
    };
    &DESCRIPTOR
}
