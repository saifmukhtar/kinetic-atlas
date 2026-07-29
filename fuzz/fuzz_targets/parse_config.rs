#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the NetworkConfig deserialization
    if let Ok(_) = serde_json::from_slice::<kinetic_atlas::types::NetworkConfig>(data) {
        // Just verify it doesn't panic
    }
});
