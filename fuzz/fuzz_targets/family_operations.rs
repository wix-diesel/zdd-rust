#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/fuzz_support/mod.rs"]
mod fuzz_support;

fuzz_target!(|data: &[u8]| fuzz_support::run_family_operations(data));
