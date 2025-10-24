#![no_main]

use harmoniq_host_vst3::fuzz_roundtrip_ipc;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    fuzz_roundtrip_ipc(data);
});
