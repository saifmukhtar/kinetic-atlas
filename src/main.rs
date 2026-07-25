pub use kinetic_atlas::network;
use kinetic_atlas::types::*;
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
    ServerFuture,
};
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use kinetic_atlas::types::{AtlasConfig, NetworkConfig, RevealPayload, DnsZone, DnsRecord, CachedResponse};

#[derive(Clone)]
struct AtlasDnsHandler {
    network_channels: Arc<HashMap<String, mpsc::Sender<network::NetworkCommand>>>,
    cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
}

impl AtlasDnsHandler {
    async fn resolve_answers(
        &self,
        request: &Request,
        clean_name: &str,
        channel: &mpsc::Sender<network::NetworkCommand>,
    ) -> Option<Vec<hickory_proto::rr::Record>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        if channel.send(network::NetworkCommand::GetRecord { domain: clean_name.to_string(), resp: resp_tx }).await.is_err() {
            return None;
        }

        let payload = match tokio::time::timeout(Duration::from_secs(5), resp_rx).await {
            Ok(Ok(Some(payload))) => payload,
            Ok(Ok(None)) => {
                warn!("DHT lookup returned no record for {}", clean_name);
                return None;
            }
            Ok(Err(_)) => {
                warn!("DHT lookup channel closed for {}", clean_name);
                return None;
            }
            Err(_) => {
                warn!("DHT lookup timed out for {}", clean_name);
                return None;
            }
        };

        let reveal = match serde_json::from_slice::<RevealPayload>(&payload) {
            Ok(r) => r,
            Err(_) => return None,
        };

        let zone = match serde_json::from_slice::<DnsZone>(&reveal.payload) {
            Ok(z) => z,
            Err(_) => return None,
        };

        let records = match zone.records.get("@") {
            Some(r) => r,
            None => return None,
        };

        let mut answers = Vec::new();
        let mut join_set = tokio::task::JoinSet::new();

        for record in records {
            match record {
                DnsRecord::A(ip) => {
                    answers.push(hickory_proto::rr::Record::from_rdata(
                        request.query().name().into(),
                        60,
                        hickory_proto::rr::RData::A(hickory_proto::rr::rdata::A(*ip)),
                    ));
                }
                DnsRecord::AAAA(ip) => {
                    answers.push(hickory_proto::rr::Record::from_rdata(
                        request.query().name().into(),
                        60,
                        hickory_proto::rr::RData::AAAA(hickory_proto::rr::rdata::AAAA(*ip)),
                    ));
                }
                DnsRecord::CNAME(cname) => {
                    if let Ok(name) = hickory_proto::rr::Name::from_utf8(cname) {
                        answers.push(hickory_proto::rr::Record::from_rdata(
                            request.query().name().into(),
                            60,
                            hickory_proto::rr::RData::CNAME(hickory_proto::rr::rdata::CNAME(name)),
                        ));
                    }
                }
                DnsRecord::TXT(txt) => {
                    answers.push(hickory_proto::rr::Record::from_rdata(
                        request.query().name().into(),
                        60,
                        hickory_proto::rr::RData::TXT(hickory_proto::rr::rdata::TXT::new(vec![txt.clone()])),
                    ));
                }
                DnsRecord::KID(kid) => {
                    answers.push(hickory_proto::rr::Record::from_rdata(
                        request.query().name().into(),
                        60,
                        hickory_proto::rr::RData::TXT(hickory_proto::rr::rdata::TXT::new(vec![format!("did={}", kid)])),
                    ));
                }
                DnsRecord::IPFS(cid) => {
                    answers.push(hickory_proto::rr::Record::from_rdata(
                        request.query().name().into(),
                        60,
                        hickory_proto::rr::RData::TXT(hickory_proto::rr::rdata::TXT::new(vec![format!("dnslink=/ipfs/{}", cid)])),
                    ));
                }
                DnsRecord::PeerId(peer_id_str) => {
                    let peer_id_str = peer_id_str.clone();
                    let channel = channel.clone();
                    let query_name_into: hickory_proto::rr::Name = request.query().name().into();
                    join_set.spawn(async move {
                        let (p_tx, p_rx) = oneshot::channel();
                        if channel.send(network::NetworkCommand::LookupPeer { peer_id_str, resp: p_tx }).await.is_ok() {
                            if let Ok(Ok(Some(ip))) = tokio::time::timeout(Duration::from_secs(3), p_rx).await {
                                match ip {
                                    std::net::IpAddr::V4(ipv4) => {
                                        return Some(hickory_proto::rr::Record::from_rdata(
                                            query_name_into,
                                            60,
                                            hickory_proto::rr::RData::A(hickory_proto::rr::rdata::A(ipv4)),
                                        ));
                                    }
                                    std::net::IpAddr::V6(ipv6) => {
                                        return Some(hickory_proto::rr::Record::from_rdata(
                                            query_name_into,
                                            60,
                                            hickory_proto::rr::RData::AAAA(hickory_proto::rr::rdata::AAAA(ipv6)),
                                        ));
                                    }
                                }
                            }
                        }
                        None
                    });
                }
                _ => {}
            }
        }

        while let Some(res) = join_set.join_next().await {
            if let Ok(Some(record)) = res {
                answers.push(record);
            }
        }

        Some(answers)
    }
}

#[async_trait::async_trait]
impl RequestHandler for AtlasDnsHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        let query = request.query();
        let query_name = query.name().to_string();
        let mut clean_name = query_name.to_lowercase();
        if clean_name.ends_with('.') {
            clean_name.pop();
        }
        
        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = *request.header();
        header.set_message_type(hickory_proto::op::MessageType::Response);

        let tld = if let Some(idx) = clean_name.rfind('.') {
            &clean_name[idx..]
        } else {
            &clean_name
        };

        if let Some(channel) = self.network_channels.get(tld) {
            let mut cached_answers = None;
            
            // Check cache safely
            match self.cache.read() {
                Ok(cache_reader) => {
                    if let Some(cached) = cache_reader.get(&clean_name) {
                        if Instant::now() < cached.expires_at {
                            cached_answers = Some(cached.answers.clone());
                        }
                    }
                },
                Err(_) => {
                    warn!("Cache lock is poisoned.");
                }
            }

            if let Some(answers) = cached_answers {
                info!("Cache hit for {}", query_name);
                header.set_response_code(hickory_proto::op::ResponseCode::NoError);
                let response = builder.build(header, answers.iter(), &[], &[], &[]);
                let mut h = header;
                h.set_response_code(hickory_proto::op::ResponseCode::ServFail);
                return response_handle
                    .send_response(response)
                    .await
                    .unwrap_or_else(|_| h.into());
            }

            info!("Query for {} matches network TLD {} (Cache miss)", query_name, tld);
            
            if let Some(answers) = self.resolve_answers(request, &clean_name, channel).await {
                if answers.is_empty() {
                    header.set_response_code(hickory_proto::op::ResponseCode::NXDomain);
                    let mut h = header;
                    h.set_response_code(hickory_proto::op::ResponseCode::ServFail);
                    return response_handle
                        .send_response(builder.build(header, &[], &[], &[], &[]))
                        .await
                        .unwrap_or_else(|_| h.into());
                }

                // Store in cache with 60s TTL
                match self.cache.write() {
                    Ok(mut cache_writer) => {
                        cache_writer.insert(clean_name.clone(), CachedResponse {
                            answers: answers.clone(),
                            expires_at: Instant::now() + Duration::from_secs(60),
                        });
                    },
                    Err(_) => warn!("Cache lock poisoned, not caching result."),
                }

                header.set_response_code(hickory_proto::op::ResponseCode::NoError);
                let response = builder.build(header, answers.iter(), &[], &[], &[]);
                let mut h = header;
                h.set_response_code(hickory_proto::op::ResponseCode::ServFail);
                return response_handle
                    .send_response(response)
                    .await
                    .unwrap_or_else(|_| h.into());
            }
        }
        
        // If we reach here, no network matched or the lookup failed.
        header.set_response_code(hickory_proto::op::ResponseCode::NXDomain);
        let mut h = header;
        h.set_response_code(hickory_proto::op::ResponseCode::ServFail);
        response_handle
            .send_response(builder.build(header, &[], &[], &[], &[]))
            .await
            .unwrap_or_else(|_| h.into())
    }
}

async fn sync_with_kinetic(config: &AtlasConfig, networks: &HashMap<String, NetworkConfig>) {
    let client = reqwest::Client::new();
    let url = format!("{}/internal/atlas/sync", config.kinetic_api);
    
    let tlds: Vec<String> = networks.keys().cloned().collect();
    
    loop {
        let mut req = client.post(&url).json(&tlds);
        if !config.kinetic_token.is_empty() {
            req = req.bearer_auth(&config.kinetic_token);
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Successfully synced {} TLDs to Kinetic Daemon.", tlds.len());
                break;
            }
            Ok(resp) => {
                warn!("Kinetic Daemon responded with {}. Retrying in 5s...", resp.status());
            }
            Err(e) => {
                warn!("Failed to connect to Kinetic Daemon: {}. Retrying in 5s...", e);
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn load_networks(dir: &str) -> HashMap<String, NetworkConfig> {
    let mut networks = HashMap::new();
    let path = std::path::Path::new(dir);
    if !path.exists() || !path.is_dir() {
        warn!("Networks directory '{}' not found or is not a directory.", dir);
        return networks;
    }

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<NetworkConfig>(&content) {
                        Ok(config) => {
                            info!("Loaded network config for TLD: {}", config.tld);
                            networks.insert(format!(".{}", config.tld.clone()), config);
                        }
                        Err(e) => error!("ATLAS-ERR: Failed to parse {:?}: {}", path, e),
                    },
                    Err(e) => error!("ATLAS-ERR: Failed to read {:?}: {}", path, e),
                }
            }
        }
    }
    networks
}

fn get_or_create_config() -> anyhow::Result<AtlasConfig> {
    let atlas_config_path = "atlas.json";
    match fs::read_to_string(atlas_config_path) {
        Ok(content) => {
            let config = serde_json::from_str(&content)?;
            Ok(config)
        }
        Err(_) => {
            error!("ATLAS-ERR: Failed to read {}. Creating default...", atlas_config_path);
            let default_config = AtlasConfig {
                bind_port: 34291,
                kinetic_api: "http://127.0.0.1:5352".to_string(),
                kinetic_token: "".to_string(),
                networks_dir: "./networks".to_string(),
            };
            fs::write(
                atlas_config_path,
                serde_json::to_string_pretty(&default_config)?,
            )?;
            Ok(default_config)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Kinetic Atlas Daemon...");

    let config = get_or_create_config()?;
    let networks = load_networks(&config.networks_dir);

    let sync_config = config.clone();
    let sync_networks = networks.clone();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    
    tokio::spawn(async move {
        tokio::select! {
            _ = sync_with_kinetic(&sync_config, &sync_networks) => {}
            _ = shutdown_rx.recv() => {
                info!("Shutting down background sync task...");
            }
        }
    });

    let mut network_channels = HashMap::new();
    
    // Spawn network swarms in parallel tasks
    let mut network_handles = Vec::new();
    for (tld, net_config) in networks.into_iter() {
        let (tx, rx) = mpsc::channel(32);
        network_channels.insert(tld.clone(), tx);
        
        let mut t_shutdown_rx = shutdown_tx.subscribe();
        network_handles.push(tokio::spawn(async move {
            match network::KineticAtlasNode::new(net_config.network_id.clone(), net_config.bootstrap_nodes.clone()) {
                Ok(mut node) => {
                    tokio::select! {
                        _ = node.run(rx) => {}
                        _ = t_shutdown_rx.recv() => {
                            info!("Shutting down libp2p network node for {}", tld);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to initialize libp2p network for {}: {}", tld, e);
                }
            }
        }));
    }

    let handler = AtlasDnsHandler {
        network_channels: Arc::new(network_channels),
        cache: Arc::new(RwLock::new(HashMap::new())),
    };

    let mut server = ServerFuture::new(handler);
    
    let udp_socket = tokio::net::UdpSocket::bind(format!("127.0.0.1:{}", config.bind_port)).await?;
    server.register_socket(udp_socket);
    info!("Atlas DNS listening on 127.0.0.1:{} (UDP)", config.bind_port);

    let tcp_listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", config.bind_port)).await?;
    server.register_listener(tcp_listener, Duration::from_secs(5));
    info!("Atlas DNS listening on 127.0.0.1:{} (TCP)", config.bind_port);

    // Wait for ctrl-c
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("Received Ctrl-C, gracefully shutting down Atlas...");
            let _ = shutdown_tx.send(());
        }
    });

    server.block_until_done().await?;
    info!("Atlas DNS server has stopped.");
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_clean() {
        let domain = "foo.kinetic.";
        let mut clean_name = domain.to_lowercase();
        if clean_name.ends_with('.') {
            clean_name.pop();
        }
        assert_eq!(clean_name, "foo.kinetic");
    }
}
