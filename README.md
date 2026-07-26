# Kinetic Atlas

Kinetic Atlas is a standalone sidecar service that integrates with the Kinetic Network. It acts as an intelligent DNS server that leverages the libp2p Kademlia DHT to resolve custom domain names (such as `.kin` and `.fork`) stored securely on the decentralized network.

---

## 🎯 Motivation

The Kinetic Network allows users to register decentralized namespaces and domains directly onto a secure, distributed ledger. However, traditional web browsers, operating systems, and standard internet protocols (like HTTP/HTTPS) expect traditional DNS to resolve domain names.

**Kinetic Atlas bridges this gap.**

Instead of forcing users to build custom plugins or browsers to resolve decentralized `.kin` domains, Atlas spins up a local, standard DNS server. Your local operating system can point its DNS resolver to Atlas. When you type `mywebsite.kin` into your browser, the OS queries Atlas. Atlas then traverses the Kinetic Network's peer-to-peer Distributed Hash Table (DHT), finds the underlying IP addresses, and returns them to the browser just like a traditional DNS server would.

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

## 🌐 Kinetic Atlas Network Registry

Since Kinetic allows anyone to fork the network, run their own parallel nodes, and create custom decentralized domain namespaces, we maintain a registry to prevent **network collisions**.

This is similar to Ethereum's ChainID Registry (`chainlist.org`) or the Cosmos Chain Registry. By registering your network, you ensure that:
1. No one else overrides your custom TLD (e.g. `.kin`, `.fork`).
2. You do not accidentally bind to local IP addresses and ports already in use by the main Kinetic network or other popular forks.
3. Your TLD does not conflict with established **ICANN** domains (e.g., `.com`, `.net`, `.org` are strictly prohibited).

### How to Register Your Fork / TLD

We have built a **fully automated, zero-touch registration pipeline** to make adding your network as seamless as possible:

1. **Submit an Issue:** Go to the Issues tab and select the **Register Kinetic Network** template.
2. **Fill out the Form:** Provide your Network Name, TLD, Seed Domain, and Bootstrap Nodes.
3. **Automated Validation:** A GitHub Action will instantly spin up a Python bot (`validate_registration.py`) that will:
   - Verify your requested TLD, Network ID, and Local Bind IP are 100% unique globally.
   - Automatically extract the TCP port from your bootstrap nodes.
   - Ping your Seed Domain URL to prove your network is alive and publicly accessible.
4. **Automated Pull Request:** If your network passes validation, the bot will automatically generate your `networks/<tld>.json` configuration, update the `ATLAS.md` directory table, and open a Pull Request on your behalf!
5. **Merge & Announce:** Once a maintainer clicks Merge, the bot will automatically announce your new network to the official Discord!

#### Daily Uptime Monitoring (The Heartbeat Bot)
Atlas maintains a pristine registry. Every midnight UTC, the `monitor_networks.py` script automatically runs and TCP pings the bootstrap nodes of every registered network.
- If a network is alive, it retains its `🟢 Active` badge in `ATLAS.md`.
- If a network fails the ping for **7 consecutive days**, it is automatically marked as `🔴 Offline` to protect users from dead forks.

---

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
- `networks/`: Directory containing the `.json` configurations for registered TLDs (like `.kin` or `.fork`).
- `fuzz/`: Cargo-fuzz targets to ensure robust network deserialization.

---

## Testing & Fuzzing

Run the standard test suite:
```bash
cargo test
```

Run the robust payload fuzzer (Requires nightly rust):
```bash
cargo +nightly fuzz run fuzz_target_1
```
