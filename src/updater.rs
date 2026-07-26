use crate::registry::TldRegistry;
use crate::types::AtlasConfig;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

#[derive(serde::Deserialize)]
struct GitHubContent {
    name: String,
    download_url: Option<String>,
    #[serde(rename = "type")]
    item_type: String,
}

pub fn start_auto_updater(
    config: Arc<AtlasConfig>,
    registry: Arc<RwLock<TldRegistry>>,
) {
    if config.registry_url.is_none() {
        info!("GitHub Auto-Updater is disabled (registry_url is null).");
        return;
    }

    tokio::spawn(async move {
        let url = config.registry_url.as_ref().unwrap().clone();
        
        let client = reqwest::Client::builder()
            .user_agent("Kinetic-Atlas-Daemon/1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        loop {
            info!("Auto-Updater: Checking {} for TLD registry updates...", url);

            match fetch_and_update(&client, &url, &config.networks_dir).await {
                Ok(count) => {
                    info!("Auto-Updater: Successfully synced {} files from GitHub.", count);
                    // Hot reload the registry
                    let mut reg = registry.write().await;
                    reg.load_from_dir(&config.networks_dir);
                    info!("Auto-Updater: Hot-reloaded TldRegistry.");
                }
                Err(e) => {
                    warn!("Auto-Updater: Failed to sync registry from GitHub: {}", e);
                }
            }

            // Sleep for 24 hours before checking again
            tokio::time::sleep(tokio::time::Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

async fn fetch_and_update(client: &reqwest::Client, url: &str, out_dir: &str) -> anyhow::Result<usize> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let contents: Vec<GitHubContent> = resp.json().await?;

    let mut count = 0;
    
    // Ensure output directory exists
    let _ = std::fs::create_dir_all(out_dir);

    for item in contents {
        if item.item_type == "file" && item.name.ends_with(".json") {
            if let Some(download_url) = item.download_url {
                match client.get(&download_url).send().await {
                    Ok(file_resp) => {
                        if let Ok(file_bytes) = file_resp.bytes().await {
                            let out_path = std::path::Path::new(out_dir).join(&item.name);
                            if let Err(e) = std::fs::write(&out_path, &file_bytes) {
                                error!("Auto-Updater: Failed to save {}: {}", item.name, e);
                            } else {
                                count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Auto-Updater: Failed to download {}: {}", download_url, e);
                    }
                }
            }
        }
    }

    Ok(count)
}
