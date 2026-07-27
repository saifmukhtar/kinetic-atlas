use std::collections::HashMap;
use std::fs;
use tracing::{error, info, warn};

use crate::types::{AtlasConfig, NetworkConfig};

pub struct TldRegistry {
    pub networks: HashMap<String, NetworkConfig>,
}

impl TldRegistry {
    pub fn new() -> Self {
        Self {
            networks: HashMap::new(),
        }
    }

    pub fn load_from_dir(&mut self, dir: &str, global_config: &AtlasConfig) {
        let path = std::path::Path::new(dir);
        if !path.exists() || !path.is_dir() {
            warn!("Networks directory '{}' not found or is not a directory.", dir);
            return;
        }

        self.networks.clear();

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    match fs::read_to_string(&path) {
                        Ok(content) => match serde_json::from_str::<NetworkConfig>(&content) {
                            Ok(config) => {
                                // Validate and format TLD
                                let clean_tld = format!(".{}", config.tld.trim_start_matches('.'));
                                
                                // Apply Whitelist / Blacklist filtering
                                let tld_name = config.tld.trim_start_matches('.').to_string();
                                let is_whitelisted = global_config.whitelist.is_empty() || global_config.whitelist.contains(&tld_name);
                                let is_blacklisted = !global_config.blacklist.is_empty() && global_config.blacklist.contains(&tld_name);

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
                                        .join("kinetic_global")
                                        .join("proxies")
                                        .join(format!("atlas_{}.json", clean_tld.trim_start_matches('.')));
                                    
                                    let proxy_json = serde_json::json!({
                                        "tld": clean_tld,
                                        "proxy_port": 17002,
                                        "proxy_ip": "127.0.0.1"
                                    });

                                    if let Err(e) = fs::write(&proxy_file, serde_json::to_string_pretty(&proxy_json).unwrap()) {
                                        warn!("Failed to register {} with kinetic-pac: {}", clean_tld, e);
                                    } else {
                                        info!("Registered {} with kinetic-pac", clean_tld);
                                    }
                                }
                            }
                            Err(e) => error!("ATLAS-ERR: Failed to parse {:?}: {}", path, e),
                        },
                        Err(e) => error!("ATLAS-ERR: Failed to read {:?}: {}", path, e),
                    }
                }
            }
        }
    }

    pub fn get_config(&self, tld: &str) -> Option<NetworkConfig> {
        self.networks.get(tld).cloned()
    }
}
