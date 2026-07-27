use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Global configuration for the Atlas daemon.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AtlasConfig {
    pub version: String,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub blacklist: Vec<String>,
    pub bind_port: u16,
    pub kinetic_api: String,
    pub kinetic_token: String,
    pub networks_dir: String,
    #[serde(default = "default_registry_url")]
    pub registry_url: Option<String>,
    #[serde(default)]
    pub override_ipfs_gateway: Option<String>,
}

fn default_registry_url() -> Option<String> {
    Some("https://api.github.com/repos/saifmukhtar/kinetic-atlas/contents/networks".to_string())
}

/// Configuration for a specific foreign network.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetworkConfig {
    pub version: String,
    pub network_id: String,
    pub tld: String,
    pub bootstrap_nodes: Vec<String>,
    pub local_bind_ip: String,
    #[serde(default)]
    pub seed_domain: Option<String>,
    #[serde(default)]
    pub ipfs_gateway: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
pub struct RevealPayload {
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnsZone {
    #[serde(default)]
    pub records: HashMap<String, Vec<DnsRecord>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum DnsRecord {
    A(std::net::Ipv4Addr),
    AAAA(std::net::Ipv6Addr),
    CNAME(String),
    TXT(String),
    PeerId(String),
    KID(String),
    IPFS(String),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyRequest {
    pub method: std::sync::Arc<str>,
    pub path: std::sync::Arc<str>,
    pub headers: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    #[serde(with = "serde_bytes_wrapper")]
    pub body: bytes::Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyResponse {
    pub status: u16,
    pub headers: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
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
