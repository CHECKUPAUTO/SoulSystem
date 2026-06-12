#![no_main]
use libfuzzer_sys::fuzz_target;

// Fuzz target for arXiv XML/HTML parser
// Target function: avid_scout::feed::parse_arxiv_feed
// Note: adapt path if the function signature differs
fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        // avid_scout::feed_parser::parse_arxiv_feed(text);
        // Currently a no-op — uncomment when the function is identified
        let _ = text.len();
    }
});
