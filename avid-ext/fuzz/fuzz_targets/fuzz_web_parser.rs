#![no_main]
use libfuzzer_sys::fuzz_target;

// Fuzz target for web page parser
// Target function: avid_scout::extractor::parse_html
fuzz_target!(|data: &[u8]| {
    if let Ok(html) = std::str::from_utf8(data) {
        // Parse without throwing
        let _ = avid_scout::parse_web_page::parse_web_page(html);
    }
});
