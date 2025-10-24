#![no_main]

use harmoniq_engine::project::fuzz_parse_project;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    fuzz_parse_project(data);
});
