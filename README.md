# Kinetic Atlas

Kinetic Atlas is a standalone sidecar service that integrates with the Kinetic Network. It acts as an intelligent DNS server that leverages the libp2p Kademlia DHT to resolve custom domain names (such as `.kin` and `.fork`) stored securely on the decentralized network.

---

## 🎯 Motivation

The Kinetic Network allows users to register decentralized namespaces and domains directly onto a secure, distributed ledger. However, traditional web browsers, operating systems, and standard internet protocols (like HTTP/HTTPS) expect traditional DNS to resolve domain names.

**Kinetic Atlas bridges this gap.**

Instead of forcing users to build custom plugins or browsers to resolve decentralized `.kin` domains, Atlas spins up a local, standard DNS server (on UDP/TCP port 53 or a custom port). Your local operating system can point its DNS resolver to Atlas. When you type `mywebsite.kin` into your browser, the OS queries Atlas. Atlas then traverses the Kinetic Network's peer-to-peer Distributed Hash Table (DHT), finds the underlying IP addresses, and returns them to the browser just like a traditional DNS server would.

The goal is **zero-friction decentralization**.

---

## ⚙️ How It Works

### 1. The DNS Interface
Atlas utilizes the `hickory-server` library to expose a fully compliant DNS endpoint. When a DNS query (e.g., an `A` record lookup) arrives, Atlas intercepts the request and parses the target domain name.

### 2. Kademlia DHT Swarms
Atlas is not a single monolith—it manages multiple libp2p swarms. Because a user might want to resolve domains from entirely separate side-networks (e.g., the `.kin` mainnet vs the `.fork` devnet), Atlas dynamically loads network configurations from the `networks/` directory. For every configured Top-Level Domain (TLD), Atlas spins up a dedicated `libp2p` node that bootstraps into that specific network's DHT.

### 3. Record Resolution & `PeerId` Lookups
When Atlas queries the DHT for a domain, the network returns a `RevealPayload` containing a `DnsZone`. A zone can contain standard records like `A`, `AAAA`, `CNAME`, `TXT`, and `IPFS`. 

However, decentralized domains often point to dynamically changing IP addresses. For this, Atlas supports a custom **`PeerId`** DNS record. If a domain points to a Libp2p `PeerId`, Atlas performs a secondary, concurrent DHT lookup to find the peer's current public IP address and automatically translates it into an `A` or `AAAA` record for the browser.

### 4. Background Syncing
To stay connected, Atlas runs a background synchronization loop (`sync_with_kinetic`). It communicates with the local `kinetic-daemon` node to announce which TLDs it is currently servicing.

---

## Features

- **Decentralized DNS**: Serves A, AAAA, CNAME, TXT, PeerId, KID, and IPFS records over standard UDP/TCP DNS queries.
- **Kademlia DHT Integration**: Connects to the Kinetic Network via Libp2p to fetch domain mappings.
- **Background Synchronization**: Automatically syncs with the Kinetic Daemon to ensure the local DHT node has up-to-date peer topologies.
- **Concurrent Lookups**: Highly optimized parallel peer lookups for lightning-fast DNS resolution using `tokio::task::JoinSet`.
- **Robust Error Handling**: Handles network faults and malformed JSON payloads gracefully without panicking.

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
