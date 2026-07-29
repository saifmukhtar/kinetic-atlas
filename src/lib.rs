#![warn(missing_docs)]

//! Kinetic Atlas Node.
//!
//! Atlas is the Kinetic network's web gateway. It provides seamless access to
//! `.kin` decentralized domains through a standard HTTP proxy and auto-updates
//! its routing tables based on a global TLD registry.

/// Common constants used across the node.
pub mod constants;
/// DNS resolution logic.
pub mod dns_tree;
/// Unified error types and RFC 7807 problem details.
pub mod error;
/// P2P swarm and DHT network logic.
pub mod network;
/// HTTP Proxy handling and request routing.
pub mod proxy;
/// TLD registry parsing and filtering.
pub mod registry;
/// Dynamic management of P2P swarms per TLD.
pub mod swarm_manager;
/// Core node types and configurations.
pub mod types;
/// GitHub-based auto-updater for registry records.
pub mod updater;
