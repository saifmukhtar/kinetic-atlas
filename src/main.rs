#![warn(missing_docs)]

//! Main entry point for the Kinetic Atlas daemon.
//!
//! This binary initializes the configuration, loads the TLD registry, starts the P2P
//! swarm manager, and binds the HTTP proxy server to the configured port.

use kinetic_atlas::constants;
use kinetic_atlas::error::AtlasError;
use kinetic_atlas::proxy::{start_proxy_server, ProxyState};
use kinetic_atlas::registry::TldRegistry;
use kinetic_atlas::swarm_manager::SwarmManager;
use kinetic_atlas::types::AtlasConfig;
use kinetic_atlas::updater;
use std::sync::Arc;
use tracing::info;

fn get_or_create_config() -> Result<AtlasConfig, AtlasError> {
    let base_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("kinetic_atlas");

    let _ = std::fs::create_dir_all(&base_dir);
    let atlas_config_path = base_dir.join("atlas.json");
    match std::fs::read_to_string(&atlas_config_path) {
        Ok(content) => {
            let config = serde_json::from_str(&content)
                .map_err(|e| AtlasError::ConfigError(e.to_string()))?;
            Ok(config)
        }
        Err(_) => {
            tracing::error!(
                "ATLAS-ERR: Failed to read {:?}. Creating default...",
                atlas_config_path
            );
            let default_config = AtlasConfig {
                version: "1.0".to_string(),
                whitelist: vec![],
                blacklist: vec![],
                bind_port: constants::DEFAULT_PROXY_PORT,
                kinetic_api: constants::DEFAULT_KINETIC_API.to_string(),
                kinetic_token: "".to_string(),
                networks_dir: base_dir.join("networks").to_string_lossy().to_string(),
                registry_url: Some(
                    "https://api.github.com/repos/saifmukhtar/kinetic-atlas/contents/networks"
                        .to_string(),
                ),
                override_ipfs_gateway: None,
            };
            std::fs::write(
                &atlas_config_path,
                serde_json::to_string_pretty(&default_config)
                    .map_err(|e| AtlasError::ConfigError(e.to_string()))?,
            )
            .map_err(|e| {
                AtlasError::ConfigError(format!("Failed to write default config: {}", e))
            })?;
            Ok(default_config)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), AtlasError> {
    tracing_subscriber::fmt::init();
    info!("Starting Kinetic Atlas HTTP Proxy Daemon...");

    let config = get_or_create_config()?;

    // Load registry from GitHub-style folder
    let mut registry = TldRegistry::new();
    registry.load_from_dir(&config.networks_dir, &config);
    let registry = Arc::new(tokio::sync::RwLock::new(registry));

    // Setup global broadcast for shutdown signals
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let swarms = SwarmManager::new(shutdown_tx.clone());

    let config = Arc::new(config);
    let state = ProxyState {
        registry: registry.clone(),
        swarms,
        global_config: config.clone(),
    };

    // Wait for ctrl-c in the background
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("Received Ctrl-C, gracefully shutting down Atlas swarms...");
            let _ = shutdown_tx.send(());
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            std::process::exit(0);
        }
    });

    updater::start_auto_updater(config.clone(), registry.clone());

    start_proxy_server(config.bind_port, state)
        .await
        .map_err(|e| AtlasError::ConfigError(e.to_string()))?;

    Ok(())
}
