use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Global configuration for the Atlas daemon.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AtlasConfig {
    pub bind_port: u16,
    pub kinetic_api: String,
    pub kinetic_token: String,
    pub networks_dir: String,
}

/// Configuration for a specific foreign network.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetworkConfig {
    pub network_id: String,
    pub tld: String,
    pub bootstrap_nodes: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
pub struct RevealPayload {
    pub payload: Vec<u8>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
pub struct DnsZone {
    #[serde(default)]
    pub records: HashMap<String, Vec<DnsRecord>>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
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

#[derive(Clone)]
pub struct CachedResponse {
    pub answers: Vec<hickory_proto::rr::Record>,
    pub expires_at: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_atlas_config_serialization() -> anyhow::Result<()> {
        let config = AtlasConfig {
            bind_port: 5353,
            kinetic_api: "http://localhost".to_string(),
            kinetic_token: "secret".to_string(),
            networks_dir: "./networks".to_string(),
        };
        let serialized = serde_json::to_string(&config)?;
        let deserialized: AtlasConfig = serde_json::from_str(&serialized)?;
        assert_eq!(config.bind_port, deserialized.bind_port);
        assert_eq!(config.kinetic_api, deserialized.kinetic_api);
        Ok(())
    }

    #[test]
    fn test_reveal_payload_empty() -> anyhow::Result<()> {
        let json = r#"{"payload": []}"#;
        let p: RevealPayload = serde_json::from_str(json)?;
        assert!(p.payload.is_empty());
        Ok(())
    }

    proptest! {
        #[test]
        fn prop_dns_record_serialization(
            a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255,
            s1 in "\\PC*", s2 in "\\PC*"
        ) {
            let record_a = DnsRecord::A(Ipv4Addr::new(a, b, c, d));
            let serialized = serde_json::to_string(&record_a).unwrap();
            let deserialized: DnsRecord = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(record_a, deserialized);

            let record_cname = DnsRecord::CNAME(s1.clone());
            let serialized = serde_json::to_string(&record_cname).unwrap();
            let deserialized: DnsRecord = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(record_cname, deserialized);
            
            let record_txt = DnsRecord::TXT(s2.clone());
            let serialized = serde_json::to_string(&record_txt).unwrap();
            let deserialized: DnsRecord = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(record_txt, deserialized);
        }
    }
}
