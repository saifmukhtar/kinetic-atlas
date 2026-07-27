pub mod network;
pub mod registry;
pub mod swarm_manager;
pub mod types;
pub mod proxy;
pub mod dns_tree;
pub mod updater;

use registry::TldRegistry;
use swarm_manager::SwarmManager;
use proxy::{start_proxy_server, ProxyState};
use std::sync::Arc;
use tracing::info;
use types::AtlasConfig;

fn get_or_create_config() -> anyhow::Result<AtlasConfig> {
    let base_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("kinetic_atlas");
        
    let _ = std::fs::create_dir_all(&base_dir);
    let atlas_config_path = base_dir.join("atlas.json");
    match std::fs::read_to_string(&atlas_config_path) {
        Ok(content) => {
            let config = serde_json::from_str(&content)?;
            Ok(config)
        }
        Err(_) => {
            tracing::error!("ATLAS-ERR: Failed to read {:?}. Creating default...", atlas_config_path);
            let default_config = AtlasConfig {
                version: "1.0".to_string(),
                whitelist: vec![],
                blacklist: vec![],
                bind_port: 17002, // Changed default to proxy port
                kinetic_api: "http://127.0.0.2:16002".to_string(),
                kinetic_token: "".to_string(),
                networks_dir: base_dir.join("networks").to_string_lossy().to_string(),
                registry_url: Some("https://api.github.com/repos/saifmukhtar/kinetic-atlas/contents/networks".to_string()),
                override_ipfs_gateway: None,
            };
            std::fs::write(
                &atlas_config_path,
                serde_json::to_string_pretty(&default_config)?,
            )?;
            Ok(default_config)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    start_proxy_server(config.bind_port, state).await?;

    Ok(())
}
