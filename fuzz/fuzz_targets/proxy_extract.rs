#![no_main]

use libfuzzer_sys::fuzz_target;
use kinetic_atlas::proxy::extract_base_domain_and_subdomain;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Just fuzz the extract_base_domain_and_subdomain logic
        // We simulate extracting `.kin` from random UTF-8 strings
        let _ = extract_base_domain_and_subdomain(s, ".kin");
        let _ = extract_base_domain_and_subdomain(s, ".anything");
    }
});
