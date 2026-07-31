use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Global configuration for the Atlas daemon.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AtlasConfig {
    /// Version
    pub version: String,
    /// Whitelist
    #[serde(default)]
    pub whitelist: Vec<String>,
    /// Blacklist
    #[serde(default)]
    pub blacklist: Vec<String>,
    /// Bind port
    pub bind_port: u16,
    /// Kinetic API URL
    pub kinetic_api: String,
    /// Kinetic Token
    pub kinetic_token: String,
    /// Networks directory
    pub networks_dir: String,
    /// Registry URL
    #[serde(default = "default_registry_url")]
    pub registry_url: Option<String>,
    /// Override IPFS gateway
    #[serde(default)]
    pub override_ipfs_gateway: Option<String>,
    /// Ed25519 public key (hex) to verify github network updates
    #[serde(default)]
    pub updater_public_key: Option<String>,
}

fn default_registry_url() -> Option<String> {
    Some("https://api.github.com/repos/saifmukhtar/kinetic-atlas/contents/networks".to_string())
}

/// Configuration for a specific foreign network.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetworkConfig {
    /// Version
    pub version: String,
    /// Network ID
    pub network_id: String,
    /// TLD
    pub tld: String,
    /// Bootstrap nodes
    pub bootstrap_nodes: Vec<String>,
    /// Local bind IP
    pub local_bind_ip: String,
    /// API port
    #[serde(default)]
    pub api_port: Option<u16>,
    /// Repository URL
    #[serde(default)]
    pub repo: Option<String>,
    /// Seed domain
    #[serde(default)]
    pub seed_domain: Option<String>,
    /// IPFS gateway
    #[serde(default)]
    pub ipfs_gateway: Option<String>,
}


/// DNS Zone
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnsZone {
    /// Records
    #[serde(default)]
    pub records: HashMap<String, Vec<DnsRecord>>,
}

/// DNS Record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum DnsRecord {
    /// IPv4
    A(std::net::Ipv4Addr),
    /// IPv6
    AAAA(std::net::Ipv6Addr),
    /// CNAME
    CNAME(String),
    /// TXT
    TXT(String),
    /// PeerId
    PeerId(String),
    /// KID
    KID(String),
    /// IPFS
    IPFS(String),
    /// Other
    #[serde(other)]
    Other,
}

/// Proxy Request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyRequest {
    /// Method
    pub method: std::sync::Arc<str>,
    /// Path
    pub path: std::sync::Arc<str>,
    /// Headers
    pub headers: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    /// Body
    #[serde(with = "serde_bytes_wrapper")]
    pub body: bytes::Bytes,
}

/// Proxy Response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyResponse {
    /// Status
    pub status: u16,
    /// Headers
    pub headers: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    /// Body
    #[serde(with = "serde_bytes_wrapper")]
    pub body: bytes::Bytes,
}

pub(crate) mod serde_bytes_wrapper {
    use bytes::Bytes;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_bytes::serialize(bytes.as_ref(), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let b: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Ok(Bytes::from(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    proptest! {
        #[test]
        fn test_network_config_deserialization(
            version in ".*",
            network_id in ".*",
            tld in ".*"
        ) {
            let json = json!({
                "version": version,
                "network_id": network_id,
                "tld": tld,
                "local_bind_ip": "127.0.0.1",
                "bootstrap_nodes": []
            });
            let config: Result<NetworkConfig, _> = serde_json::from_value(json);
            // It should parse successfully as long as types are correct strings
            assert!(config.is_ok());

            let config = config.unwrap();
            assert_eq!(config.version, version);
            assert_eq!(config.network_id, network_id);
            assert_eq!(config.tld, tld);
        }

        #[test]
        fn test_atlas_config_deserialization(
            version in ".*",
            bind_port in any::<u16>(),
            kinetic_api in ".*"
        ) {
            let json = json!({
                "version": version,
                "whitelist": [],
                "blacklist": [],
                "bind_port": bind_port,
                "kinetic_api": kinetic_api,
                "kinetic_token": "",
                "networks_dir": "./networks"
            });
            let config: Result<AtlasConfig, _> = serde_json::from_value(json);
            assert!(config.is_ok());

            let config = config.unwrap();
            assert_eq!(config.version, version);
            assert_eq!(config.bind_port, bind_port);
            assert_eq!(config.kinetic_api, kinetic_api);
        }
    }
}
