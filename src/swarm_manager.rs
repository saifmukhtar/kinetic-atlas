use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, error};
use std::time::{Duration, Instant};

use crate::network::{KineticAtlasNode, NetworkCommand};
use crate::types::NetworkConfig;

struct SwarmHandle {
    tx: mpsc::Sender<NetworkCommand>,
    last_used: Instant,
}

#[derive(Clone)]
pub struct SwarmManager {
    swarms: Arc<Mutex<HashMap<String, SwarmHandle>>>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl SwarmManager {
    pub fn new(shutdown_tx: tokio::sync::broadcast::Sender<()>) -> Self {
        let manager = Self {
            swarms: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx,
        };

        manager.start_garbage_collector();
        manager
    }

    fn start_garbage_collector(&self) {
        let swarms = self.swarms.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut lock = swarms.lock().await;
                let now = Instant::now();
                // 10 minute timeout
                let timeout = Duration::from_secs(10 * 60);
                
                lock.retain(|tld, handle| {
                    if now.duration_since(handle.last_used) > timeout {
                        info!("Shutting down idle libp2p swarm for {}", tld);
                        false
                    } else {
                        true
                    }
                });
            }
        });
    }

    pub async fn get_or_spawn_swarm(&self, tld: &str, config: &NetworkConfig) -> Option<mpsc::Sender<NetworkCommand>> {
        let mut lock = self.swarms.lock().await;
        
        if let Some(handle) = lock.get_mut(tld) {
            handle.last_used = Instant::now();
            return Some(handle.tx.clone());
        }

        info!("Spawning new on-demand libp2p swarm for {}", tld);

        let (tx, rx) = mpsc::channel(32);
        let mut t_shutdown_rx = self.shutdown_tx.subscribe();
        
        let tld_clone = tld.to_string();
        let mut config_clone = config.clone();

        if let Some(seed_domain) = &config_clone.seed_domain {
            info!("Resolving Kintree seed domain for {}: {}", tld, seed_domain);
            let resolved = crate::dns_tree::resolve_dns_tree(seed_domain).await;
            for addr in resolved {
                config_clone.bootstrap_nodes.push(addr.to_string());
            }
        }

        match KineticAtlasNode::new(config_clone.network_id, config_clone.bootstrap_nodes) {
            Ok(node) => {
                tokio::spawn(async move {
                    tokio::select! {
                        _ = node.run(rx) => {}
                        _ = t_shutdown_rx.recv() => {
                            info!("Shutting down libp2p network node for {}", tld_clone);
                        }
                    }
                });

                lock.insert(tld.to_string(), SwarmHandle {
                    tx: tx.clone(),
                    last_used: Instant::now(),
                });

                Some(tx)
            }
            Err(e) => {
                error!("Failed to initialize libp2p network for {}: {}", tld, e);
                None
            }
        }
    }
}
