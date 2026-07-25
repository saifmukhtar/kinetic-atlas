# Kinetic Atlas Network Registry

Welcome to the **Kinetic Atlas Network Registry**. 

Since Kinetic allows anyone to fork the network, run their own parallel nodes, and create custom decentralized domain namespaces, we maintain this registry to prevent **network collisions**.

If you are a fork operator or are launching a new TLD (Top-Level Domain) on Kinetic Atlas, you must submit a PR to this repository containing your network's details.

---

## Why does this exist?

In decentralized ecosystems, keeping a standardized public registry is best practice. It is similar to:
- **Ethereum's ChainID Registry** (chainlist.org): Ensures no two EVM chains use the same Chain ID, preventing replay attacks.
- **Cosmos Chain Registry**: A database of all active Cosmos app-chains, their RPC nodes, and native tokens.
- **SLIP-44**: The registry for cryptocurrency coin types in HD wallets.

By registering your network here, you ensure that:
1. No one else overrides your custom TLD (e.g. `.kin`, `.fork`).
2. You do not accidentally bind to local IP addresses and ports (e.g., `127.0.0.2`) already in use by the main Kinetic network or other popular forks.
3. Your TLD does not conflict with established **ICANN** domains (e.g., `.com`, `.net`, `.org` are strictly prohibited).

---

## 📝 Network Submission Form (Template)

To register your network, please create a JSON file in the `networks/` directory of this repository (e.g., `networks/myfork.json`) and fill out the following template. 

### Rules for Submission:
- **No ICANN TLDs**: You cannot use `.com`, `.org`, `.net`, `.io`, etc. Use unique decentralized identifiers.
- **Unique Network ID**: Ensure your `network_id` is unique and not already taken by another fork in the `networks/` folder.
- **Local Bind IPs**: The default Kinetic mainnet reserves `127.0.0.2` for its local DNS testing daemon. Please choose an iterative IP (e.g., `127.0.0.3`, `127.0.0.4`) for your fork's defaults to avoid port/IP exhaustion and collisions on user machines.

### Example Submission (`networks/fork.json`)

```json
{
  "network_name": "Kinetic Developer Fork",
  "network_id": "kinetic-dev-fork-01",
  "tld": ".fork",
  
  "contact_details": {
    "operator": "Your Name / Organization",
    "email": "operator@example.com",
    "repo_url": "https://github.com/your-org/kinetic-fork"
  },

  "default_configs": {
    "local_bind_ip": "127.0.0.3",
    "local_dns_port": 34292,
    "bootstrap_peers": [
      "/ip4/198.51.100.1/tcp/6170/p2p/12D3Koo...",
      "/ip4/198.51.100.2/tcp/6170/p2p/12D3Koo..."
    ]
  }
}
```

## How to Submit

1. Fork this repository.
2. Copy the template above into a new file: `networks/<your-tld>.json`.
3. Update the values to match your network.
4. Submit a Pull Request (PR) to this repository.
5. Once merged, Atlas instances globally can pull this configuration, allowing seamless DNS resolution for your new TLD!
