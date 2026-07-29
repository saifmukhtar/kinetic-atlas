use crate::registry::TldRegistry;
use crate::types::AtlasConfig;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(serde::Deserialize)]
struct GitHubContent {
    name: String,
    download_url: Option<String>,
    #[serde(rename = "type")]
    item_type: String,
}

/// Starts a background thread to fetch TLD network updates from GitHub
pub fn start_auto_updater(config: Arc<AtlasConfig>, registry: Arc<RwLock<TldRegistry>>) {
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

        let mut fail_delay = 60 * 60; // 1 hour

        loop {
            info!("Auto-Updater: Checking {} for TLD registry updates...", url);

            let sleep_duration = match fetch_and_update(&client, &url, config.clone()).await {
                Ok(count) => {
                    info!(
                        "Auto-Updater: Successfully synced {} files from GitHub.",
                        count
                    );
                    // Hot reload the registry
                    let mut reg = registry.write().await;
                    reg.load_from_dir(&config.networks_dir, &config);
                    info!("Auto-Updater: Hot-reloaded TldRegistry.");
                    fail_delay = 60 * 60; // reset
                    24 * 60 * 60 // sleep 24 hours
                }
                Err(e) => {
                    warn!("Auto-Updater: Failed to sync registry from GitHub: {}", e);
                    let d = fail_delay;
                    fail_delay = (fail_delay * 2).min(24 * 60 * 60); // exp backoff up to 24h
                    d
                }
            };

            tokio::time::sleep(tokio::time::Duration::from_secs(sleep_duration)).await;
        }
    });
}

async fn fetch_and_update(
    client: &reqwest::Client,
    url: &str,
    config: Arc<AtlasConfig>,
) -> Result<usize, crate::error::AtlasError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| crate::error::AtlasError::UpdaterFailed(e.to_string()))?
        .error_for_status()
        .map_err(|e| crate::error::AtlasError::UpdaterFailed(e.to_string()))?;
    let contents: Vec<GitHubContent> = resp
        .json()
        .await
        .map_err(|e| crate::error::AtlasError::UpdaterFailed(e.to_string()))?;

    let mut count = 0;
    let out_dir = std::path::Path::new(&config.networks_dir);

    // Ensure output directory exists
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir).map_err(|e| {
            crate::error::AtlasError::UpdaterFailed(format!(
                "Failed to create output directory {}: {}",
                out_dir.display(),
                e
            ))
        })?;
    }

    for item in contents {
        if item.item_type == "file" && item.name.ends_with(".json") {
            if let Some(download_url) = item.download_url {
                match client.get(&download_url).send().await {
                    Ok(file_resp) => {
                        let file_resp = file_resp
                            .error_for_status()
                            .map_err(|e| crate::error::AtlasError::UpdaterFailed(e.to_string()))?;
                        if let Ok(file_bytes) = file_resp.bytes().await {
                            let out_path = out_dir.join(&item.name);
                            if let Err(e) = std::fs::write(&out_path, &file_bytes) {
                                return Err(crate::error::AtlasError::UpdaterFailed(format!(
                                    "Failed to write file {}: {}",
                                    item.name, e
                                )));
                            } else {
                                count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Auto-Updater: Failed to download {}: {}", download_url, e);
                        return Err(crate::error::AtlasError::UpdaterFailed(e.to_string()));
                    }
                }
            }
        }
    }

    Ok(count)
}
