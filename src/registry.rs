use std::collections::HashMap;
use std::fs;
use tracing::{error, info, warn};

use crate::types::{AtlasConfig, NetworkConfig};

#[derive(Default)]
/// Registry of all available network configurations by TLD
pub struct TldRegistry {
    /// Map of TLD to NetworkConfig
    pub networks: HashMap<String, NetworkConfig>,
}

impl TldRegistry {
    /// Creates a new empty registry
    pub fn new() -> Self {
        Self::default()
    }
}

impl TldRegistry {
    /// Loads all network configuration JSON files from a specified directory
    pub fn load_from_dir(&mut self, dir: &str, global_config: &AtlasConfig) {
        let path = std::path::Path::new(dir);
        if !path.exists() || !path.is_dir() {
            warn!(
                "Networks directory '{}' not found or is not a directory.",
                dir
            );
            return;
        }

        self.networks.clear();

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            match serde_json::from_str::<NetworkConfig>(&content) {
                                Ok(config) => {
                                    // Validate and format TLD
                                    let clean_tld =
                                        format!(".{}", config.tld.trim_start_matches('.'));

                                    // Apply Whitelist / Blacklist filtering
                                    let tld_name = config.tld.trim_start_matches('.').to_string();
                                    let is_whitelisted = global_config.whitelist.is_empty()
                                        || global_config.whitelist.contains(&tld_name);
                                    let is_blacklisted = !global_config.blacklist.is_empty()
                                        && global_config.blacklist.contains(&tld_name);

                                    if !is_whitelisted || is_blacklisted {
                                        info!("ATLAS-INFO: Network '{}' is excluded by whitelist/blacklist settings. Skipping.", tld_name);
                                        continue;
                                    }

                                    // Prevent collisions
                                    if self.networks.contains_key(&clean_tld) {
                                        error!("ATLAS-ERR: TLD Collision detected for {}. Skipping {:?}", clean_tld, path);
                                        continue;
                                    }

                                    info!("Loaded network config for TLD: {}", clean_tld);
                                    self.networks.insert(clean_tld.clone(), config);

                                    // Register with kinetic-pac by dropping a JSON file
                                    if let Some(base_dir) = dirs::data_local_dir() {
                                        let proxy_file = base_dir
                                            .join(crate::constants::GLOBAL_DIR_NAME)
                                            .join(crate::constants::PROXIES_DIR_NAME)
                                            .join(format!(
                                                "{}{}.json",
                                                crate::constants::ATLAS_PREFIX,
                                                clean_tld.trim_start_matches('.')
                                            ));

                                        let proxy_json = serde_json::json!({
                                            "tld": clean_tld,
                                            "proxy_port": global_config.bind_port,
                                            "proxy_ip": crate::constants::DEFAULT_BIND_IP
                                        });

                                        if let Err(e) = fs::write(
                                            &proxy_file,
                                            serde_json::to_string_pretty(&proxy_json).unwrap(),
                                        ) {
                                            warn!(
                                                "Failed to register {} with kinetic-pac: {}",
                                                clean_tld, e
                                            );
                                        } else {
                                            info!("Registered {} with kinetic-pac", clean_tld);
                                        }
                                    }
                                }
                                Err(e) => error!("ATLAS-ERR: Failed to parse {:?}: {}", path, e),
                            }
                        }
                        Err(e) => error!("ATLAS-ERR: Failed to read {:?}: {}", path, e),
                    }
                }
            }
        }
    }

    /// Retrieves the network configuration for a given TLD, if available
    pub fn get_config(&self, tld: &str) -> Option<NetworkConfig> {
        self.networks.get(tld).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn dummy_global_config(whitelist: Vec<String>, blacklist: Vec<String>) -> AtlasConfig {
        AtlasConfig {
            version: "1.0".to_string(),
            whitelist,
            blacklist,
            bind_port: 17002,
            kinetic_api: "http://127.0.0.1:16202".to_string(),
            kinetic_token: "".to_string(),
            networks_dir: "".to_string(),
            registry_url: None,
            override_ipfs_gateway: None,
        }
    }

    fn write_dummy_network(dir: &Path, filename: &str, version: &str, tld: &str) {
        let path = dir.join(filename);
        let content = format!(
            r#"{{
            "version": "{}",
            "network_id": "test",
            "tld": "{}",
            "local_bind_ip": "127.0.0.1",
            "bootstrap_nodes": []
        }}"#,
            version, tld
        );
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_load_whitelist() {
        let dir = tempdir().unwrap();
        write_dummy_network(dir.path(), "kin.json", "1.0", "kin");
        write_dummy_network(dir.path(), "test.json", "1.0", "test");

        let mut registry = TldRegistry::new();
        let config = dummy_global_config(vec!["kin".to_string()], vec![]);
        registry.load_from_dir(dir.path().to_str().unwrap(), &config);

        assert!(registry.networks.contains_key(".kin"));
        assert!(!registry.networks.contains_key(".test"));
    }

    #[test]
    fn test_load_blacklist() {
        let dir = tempdir().unwrap();
        write_dummy_network(dir.path(), "kin.json", "1.0", "kin");
        write_dummy_network(dir.path(), "evil.json", "1.0", "evil");

        let mut registry = TldRegistry::new();
        let config = dummy_global_config(vec![], vec!["evil".to_string()]);
        registry.load_from_dir(dir.path().to_str().unwrap(), &config);

        assert!(registry.networks.contains_key(".kin"));
        assert!(!registry.networks.contains_key(".evil"));
    }

    #[test]
    fn test_load_missing_version() {
        let dir = tempdir().unwrap();
        // Missing version field
        let path = dir.path().join("invalid.json");
        let content = r#"{
            "network_id": "test",
            "tld": "kin",
            "local_bind_ip": "127.0.0.1",
            "bootstrap_nodes": []
        }"#;
        fs::write(path, content).unwrap();

        let mut registry = TldRegistry::new();
        let config = dummy_global_config(vec![], vec![]);
        registry.load_from_dir(dir.path().to_str().unwrap(), &config);

        assert!(!registry.networks.contains_key("invalid"));
    }
}
