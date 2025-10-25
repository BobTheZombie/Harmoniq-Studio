#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// CLAP SDK version tuple compiled with these bindings.
pub const CLAP_VERSION_LATEST: clap_version_t = clap_version_t {
    major: CLAP_VERSION_MAJOR,
    minor: CLAP_VERSION_MINOR,
    revision: CLAP_VERSION_REVISION,
};

/// Indicates whether the `ext_audio_ports` bindings are considered stable within this crate.
#[cfg(feature = "ext_audio_ports")]
pub const EXT_AUDIO_PORTS: bool = true;
#[cfg(not(feature = "ext_audio_ports"))]
pub const EXT_AUDIO_PORTS: bool = false;

#[cfg(feature = "ext_note_ports")]
pub const EXT_NOTE_PORTS: bool = true;
#[cfg(not(feature = "ext_note_ports"))]
pub const EXT_NOTE_PORTS: bool = false;

#[cfg(feature = "ext_params")]
pub const EXT_PARAMS: bool = true;
#[cfg(not(feature = "ext_params"))]
pub const EXT_PARAMS: bool = false;

#[cfg(feature = "ext_state")]
pub const EXT_STATE: bool = true;
#[cfg(not(feature = "ext_state"))]
pub const EXT_STATE: bool = false;

#[cfg(feature = "ext_gui")]
pub const EXT_GUI: bool = true;
#[cfg(not(feature = "ext_gui"))]
pub const EXT_GUI: bool = false;

#[cfg(feature = "ext_latency")]
pub const EXT_LATENCY: bool = true;
#[cfg(not(feature = "ext_latency"))]
pub const EXT_LATENCY: bool = false;

#[cfg(feature = "ext_tail")]
pub const EXT_TAIL: bool = true;
#[cfg(not(feature = "ext_tail"))]
pub const EXT_TAIL: bool = false;

#[cfg(feature = "ext_timer_support")]
pub const EXT_TIMER_SUPPORT: bool = true;
#[cfg(not(feature = "ext_timer_support"))]
pub const EXT_TIMER_SUPPORT: bool = false;

#[cfg(feature = "ext_thread_check")]
pub const EXT_THREAD_CHECK: bool = true;
#[cfg(not(feature = "ext_thread_check"))]
pub const EXT_THREAD_CHECK: bool = false;

/// Helper to load a CLAP entry point from a dynamic library symbol.
///
/// # Safety
/// The caller must ensure the provided function pointer originates from a valid CLAP shared
/// library and that the returned entry point is used according to the CLAP ABI.
#[allow(clippy::missing_safety_doc)]
pub unsafe fn load_entry(
    entry: Option<unsafe extern "C" fn(*const ::core::ffi::c_char) -> *const clap_plugin_factory>,
) -> Option<unsafe extern "C" fn(*const ::core::ffi::c_char) -> *const clap_plugin_factory> {
    entry
}
