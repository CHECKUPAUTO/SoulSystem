#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz the config/Settings parser to ensure no panics on arbitrary TOML input.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Must never panic on any TOML input
        let _ = toml::from_str::<toml::Value>(s);
    }
});
