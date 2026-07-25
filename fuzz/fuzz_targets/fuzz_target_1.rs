#![no_main]
use libfuzzer_sys::fuzz_target;
use kinetic_atlas::types::{RevealPayload, DnsRecord};

fuzz_target!(|data: &[u8]| {
    // Fuzz the RevealPayload deserialization
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<RevealPayload>(s);
    }
    
    // Fuzz the DnsRecord deserialization
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<DnsRecord>(s);
    }
    // Fuzz the DnsZone deserialization
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<kinetic_atlas::types::DnsZone>(s);
    }
});
