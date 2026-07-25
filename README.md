# Kinetic Atlas

Kinetic Atlas is a standalone sidecar service that integrates with the Kinetic Network. It acts as an intelligent DNS server that leverages the libp2p Kademlia DHT to resolve custom domain names (such as `.kin` and `.fork`) stored securely on the decentralized network.

## Features

- **Decentralized DNS**: Serves A, AAAA, CNAME, TXT, PeerId, KID, and IPFS records over standard UDP/TCP DNS queries.
- **Kademlia DHT Integration**: Connects to the Kinetic Network via Libp2p to fetch domain mappings.
- **Background Synchronization**: Automatically syncs with the Kinetic Daemon to ensure the local DHT node has up-to-date peer topologies.
- **Concurrent Lookups**: Highly optimized parallel peer lookups for lightning-fast DNS resolution.
- **Robust Error Handling**: Handles network faults and malformed JSON payloads seamlessly.

## Getting Started

### Prerequisites
- Rust 1.70+
- The `kinetic-daemon` running locally.

### Installation & Run

```bash
cargo build --release
cargo run
```

Atlas will automatically generate a default configuration file named `atlas.json` if one is not found. 
You can edit it to point to your specific `kinetic-daemon` URL and configure the port.

### Directory Structure

- `src/main.rs`: The main entry point, DNS server handler, and background sync logic.
- `src/network.rs`: The libp2p swarm and Kademlia DHT peer configuration.
- `src/types.rs`: Shared types for DnsRecord and serialization config.
- `fuzz/`: Cargo-fuzz targets to ensure robust network deserialization.
- `networks/`: Directory for placing `.json` config files for specific TLDs like `.kin` or `.fork`.

## Testing & Fuzzing

Run the test suite:
```bash
cargo test
```

Run the fuzzer (Requires nightly rust):
```bash
cargo +nightly fuzz run fuzz_target_1
```
