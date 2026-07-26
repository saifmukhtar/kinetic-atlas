use axum::{
    body::Body,
    extract::{Request, State},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use hyper::StatusCode;
use std::sync::Arc;
use tracing::{info, warn};
use libp2p::PeerId;
use std::str::FromStr;

use crate::registry::TldRegistry;
use crate::swarm_manager::SwarmManager;
use crate::network::NetworkCommand;
use crate::types::{DnsZone, DnsRecord, RevealPayload, ProxyRequest};

#[derive(Clone)]
pub struct ProxyState {
    pub registry: Arc<tokio::sync::RwLock<TldRegistry>>,
    pub swarms: SwarmManager,
    pub global_config: Arc<crate::types::AtlasConfig>,
}

pub async fn start_proxy_server(port: u16, state: ProxyState) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/*path", any(handle_proxy_request))
        .route("/", any(handle_proxy_request))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Atlas HTTP Proxy listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_proxy_request(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Response {
    let host = match req.headers().get("host") {
        Some(h) => match h.to_str() {
            Ok(s) => s.split(':').next().unwrap_or(""), // Strip port
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        },
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let domain = host.to_lowercase();
    
    // Extract TLD
    let tld = if let Some(idx) = domain.rfind('.') {
        &domain[idx..]
    } else {
        return (StatusCode::BAD_REQUEST, "Invalid TLD").into_response();
    };

    let clean_tld = tld.to_string();

    info!("Intercepted HTTP request for host: {} (TLD: {})", domain, clean_tld);

    // 1. Check Registry
    let registry_read = state.registry.read().await;
    let config = match registry_read.get_config(&clean_tld) {
        Some(c) => c,
        None => {
            warn!("Unknown TLD: {}. Not found in Atlas registry.", clean_tld);
            return (StatusCode::NOT_FOUND, "Network not supported by Atlas").into_response();
        }
    };
    drop(registry_read);

    // 2. Get or Spawn Swarm
    let channel = match state.swarms.get_or_spawn_swarm(&clean_tld, &config).await {
        Some(tx) => tx,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to connect to network").into_response();
        }
    };

    // Extract Request details
    let method = req.method().as_str().to_string();
    let path = req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("/").to_string();
    
    let mut proxy_headers: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)> = Vec::new();
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            proxy_headers.push((name.as_str().into(), v.into()));
        }
    }

    let body_bytes = match axum::body::to_bytes(req.into_body(), 5 * 1024 * 1024).await { // 5MB limit
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to read request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Failed to read body").into_response();
        }
    };

    // 3. Resolve Domain from DHT
    let (tx, rx) = tokio::sync::oneshot::channel();
    if channel.send(NetworkCommand::GetRecord { domain: domain.clone(), resp: tx }).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to communicate with swarm").into_response();
    }

    let dht_bytes = match rx.await {
        Ok(Some(bytes)) => bytes,
        _ => return (StatusCode::NOT_FOUND, "Domain not found in DHT").into_response(),
    };

    // Parse RevealPayload and DnsZone
    let reveal: RevealPayload = match serde_json::from_slice(&dht_bytes) {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid DHT payload format").into_response(),
    };

    let zone: DnsZone = match serde_json::from_slice(&reveal.payload) {
        Ok(z) => z,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid DNS zone format").into_response(),
    };

    let records = match zone.records.get("@").or_else(|| zone.records.get("www")) {
        Some(r) => r,
        None => return (StatusCode::NOT_FOUND, "No usable records in domain zone").into_response(),
    };

    let mut target_record = None;
    for record in records {
        match record {
            DnsRecord::IPFS(_) | DnsRecord::PeerId(_) => {
                target_record = Some(record.clone());
                break;
            }
            _ => {}
        }
    }

    // 4. Fetch Data
    match target_record {
        Some(DnsRecord::IPFS(cid)) => {
            info!("Routing {} to IPFS: {}", domain, cid);
            let gateway = state.global_config.override_ipfs_gateway.as_deref()
                .unwrap_or_else(|| config.ipfs_gateway.as_deref().unwrap_or("http://127.0.0.1:8080"));
            let ipfs_url = format!("{}/ipfs/{}{}", gateway, cid, if path == "/" { "" } else { &path });
            
            let reqwest_client = reqwest::Client::new();
            let mut builder = reqwest_client.request(
                reqwest::Method::from_str(&method).unwrap_or(reqwest::Method::GET),
                &ipfs_url,
            );
            
            for (k, v) in &proxy_headers {
                if k.as_ref() != "host" {
                    builder = builder.header(k.as_ref(), v.as_ref());
                }
            }
            builder = builder.body(body_bytes);

            match builder.send().await {
                Ok(res) => {
                    let mut builder = axum::response::Response::builder()
                        .status(res.status().as_u16());
                    
                    for (k, v) in res.headers() {
                        builder = builder.header(k.as_str(), v.as_bytes());
                    }
                    
                    let body = res.bytes().await.unwrap_or_default();
                    builder.body(Body::from(body)).unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR).into_response())
                }
                Err(e) => {
                    warn!("IPFS gateway failed: {}", e);
                    (StatusCode::BAD_GATEWAY, "IPFS Gateway Error").into_response()
                }
            }
        }
        Some(DnsRecord::PeerId(peer_str)) => {
            info!("Routing {} to PeerId: {}", domain, peer_str);
            let peer_id = match PeerId::from_str(&peer_str) {
                Ok(p) => p,
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid PeerId in zone").into_response(),
            };

            let proxy_req = ProxyRequest {
                method: method.into(),
                path: path.into(),
                headers: proxy_headers,
                body: body_bytes,
            };

            let (tx, rx) = tokio::sync::oneshot::channel();
            if channel.send(NetworkCommand::SendProxyRequest {
                peer_id,
                request: proxy_req,
                resp: tx,
            }).await.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send proxy request to swarm").into_response();
            }

            match rx.await {
                Ok(Some(proxy_res)) => {
                    let mut builder = axum::response::Response::builder()
                        .status(proxy_res.status);
                    
                    for (k, v) in proxy_res.headers {
                        builder = builder.header(k.as_ref(), v.as_ref());
                    }
                    
                    builder.body(Body::from(proxy_res.body)).unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR).into_response())
                }
                _ => {
                    (StatusCode::BAD_GATEWAY, "P2P Proxy Request Failed or Timed Out").into_response()
                }
            }
        }
        _ => (StatusCode::NOT_IMPLEMENTED, "Unsupported DNS Record Type").into_response(),
    }
}
