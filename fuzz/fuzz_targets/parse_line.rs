#![no_main]
//! Every line format must parse arbitrary input without panicking.
use fuck_ads_filter::{ListFormat, parse_line};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(line) = std::str::from_utf8(data) {
        for format in [ListFormat::Auto, ListFormat::Adblock, ListFormat::Hosts, ListFormat::Domains] {
            let _ = parse_line(line, format);
        }
    }
});
