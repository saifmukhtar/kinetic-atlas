use axum::{
    body::Body,
    extract::{Request, State},
    response::Response,
    routing::any,
    Router,
};
use libp2p::PeerId;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::AtlasError;
use crate::network::NetworkCommand;
use crate::registry::TldRegistry;
use crate::swarm_manager::SwarmManager;
use crate::types::{DnsRecord, DnsZone, ProxyRequest, RevealPayload};

/// Proxy state
#[derive(Clone)]
pub struct ProxyState {
    /// Registry
    pub registry: Arc<tokio::sync::RwLock<TldRegistry>>,
    /// Swarms
    pub swarms: SwarmManager,
    /// Global config
    pub global_config: Arc<crate::types::AtlasConfig>,
}

/// Starts proxy server
pub async fn start_proxy_server(port: u16, state: ProxyState) -> Result<(), AtlasError> {
    let app = Router::new()
        .route("/*path", any(handle_proxy_request))
        .route("/", any(handle_proxy_request))
        .with_state(state);

    let addr = format!("{}:{}", crate::constants::DEFAULT_BIND_IP, port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| AtlasError::ConfigError(e.to_string()))?;
    info!("Atlas HTTP Proxy listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| AtlasError::ConfigError(e.to_string()))?;
    Ok(())
}

async fn handle_proxy_request(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<Response, AtlasError> {
    let host = match req.headers().get("host") {
        Some(h) => match h.to_str() {
            Ok(s) => s.split(':').next().unwrap_or(""), // Strip port
            Err(_) => {
                return Err(AtlasError::InvalidProxyRequest(
                    "Invalid host header format".into(),
                ))
            }
        },
        None => {
            return Err(AtlasError::InvalidProxyRequest(
                "Missing host header".into(),
            ))
        }
    };

    let full_domain = host.to_lowercase();

    // Extract TLD
    let tld = if let Some(idx) = full_domain.rfind('.') {
        &full_domain[idx..]
    } else {
        return Err(AtlasError::InvalidProxyRequest(
            "Invalid domain structure: No TLD".into(),
        ));
    };

    let clean_tld = tld.to_string();

    info!(
        "Intercepted HTTP request for host: {} (TLD: {})",
        full_domain, clean_tld
    );

    // 1. Check Registry
    let registry_read = state.registry.read().await;
    let config = match registry_read.get_config(&clean_tld) {
        Some(c) => c,
        None => {
            warn!("Unknown TLD: {}. Not found in Atlas registry.", clean_tld);
            return Err(AtlasError::TldBlacklisted);
        }
    };
    drop(registry_read);

    // Extract base domain and subdomain
    let (base_domain, subdomain) = match extract_base_domain_and_subdomain(&full_domain, &clean_tld)
    {
        Some(res) => res,
        None => {
            return Err(AtlasError::InvalidProxyRequest(
                "Could not parse base domain".into(),
            ))
        }
    };

    info!(
        "Resolved base_domain: {}, subdomain: {}",
        base_domain, subdomain
    );

    // 2. Get or Spawn Swarm
    let channel = match state.swarms.get_or_spawn_swarm(&clean_tld, &config).await {
        Some(tx) => tx,
        None => {
            return Err(AtlasError::NetworkInitFailed(format!(
                "Failed to connect to network for TLD {}",
                clean_tld
            )));
        }
    };

    // Extract Request details
    let method = req.method().as_str().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|x| x.as_str())
        .unwrap_or("/")
        .to_string();

    let mut proxy_headers: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)> = Vec::new();
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            proxy_headers.push((name.as_str().into(), v.into()));
        }
    }

    // 3. Resolve Domain from DHT using base_domain
    let (tx, rx) = tokio::sync::oneshot::channel();
    if channel
        .send(NetworkCommand::GetRecord {
            domain: base_domain.clone(),
            resp: tx,
        })
        .await
        .is_err()
    {
        return Err(AtlasError::DnsResolutionFailed(
            "Failed to communicate with swarm".into(),
        ));
    }

    let dht_bytes = match rx.await {
        Ok(Some(bytes)) => bytes,
        _ => {
            return Err(AtlasError::DnsResolutionFailed(
                "Domain not found in DHT".into(),
            ))
        }
    };

    // Parse RevealPayload and DnsZone
    let reveal: RevealPayload = match serde_json::from_slice(&dht_bytes) {
        Ok(r) => r,
        Err(_) => {
            return Err(AtlasError::DnsResolutionFailed(
                "Invalid DHT payload format".into(),
            ));
        }
    };

    let zone: DnsZone = match serde_json::from_slice(&reveal.payload) {
        Ok(z) => z,
        Err(_) => {
            return Err(AtlasError::DnsResolutionFailed(
                "Invalid DNS zone format".into(),
            ));
        }
    };

    let records = match zone.records.get(&subdomain) {
        Some(r) => r,
        None => {
            if subdomain == "@" {
                match zone.records.get("www") {
                    Some(r) => r,
                    None => {
                        return Err(AtlasError::DnsResolutionFailed(
                            "No usable records in domain zone".into(),
                        ));
                    }
                }
            } else {
                return Err(AtlasError::DnsResolutionFailed(
                    "Subdomain not found in zone".into(),
                ));
            }
        }
    };

    let mut target_records = Vec::new();
    for record in records {
        match record {
            DnsRecord::IPFS(_) | DnsRecord::PeerId(_) => {
                target_records.push(record.clone());
            }
            _ => {}
        }
    }

    if target_records.is_empty() {
        return Err(AtlasError::DnsResolutionFailed(
            "No target records found".into(),
        ));
    }

    // Buffer the request body once so we can retry multiple gateways/peers if needed.
    // 1GB limit to avoid the previous 5MB limit breaking uploads.
    let body_bytes = match axum::body::to_bytes(req.into_body(), 1024 * 1024 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(e) => {
            warn!("Failed to read request body: {}", e);
            return Err(AtlasError::InvalidProxyRequest(
                "Failed to read body".into(),
            ));
        }
    };

    let mut last_error = None;

    for target_record in target_records {
        match target_record {
            DnsRecord::IPFS(cid) => {
                info!("Routing {} to IPFS: {}", full_domain, cid);
                let gateway = state
                    .global_config
                    .override_ipfs_gateway
                    .as_deref()
                    .unwrap_or_else(|| {
                        config
                            .ipfs_gateway
                            .as_deref()
                            .unwrap_or(crate::constants::DEFAULT_IPFS_GATEWAY)
                    });
                let ipfs_url = format!(
                    "{}/ipfs/{}{}",
                    gateway,
                    cid,
                    if path == "/" { "" } else { &path }
                );

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

                builder = builder.body(body_bytes.clone());

                match builder.send().await {
                    Ok(res) => {
                        let mut builder =
                            axum::response::Response::builder().status(res.status().as_u16());

                        for (k, v) in res.headers() {
                            builder = builder.header(k.as_str(), v.as_bytes());
                        }

                        // Stream response to fix silent 5MB download corruption
                        let stream = res.bytes_stream();
                        let body = Body::from_stream(stream);
                        return builder
                            .body(body)
                            .map_err(|_| AtlasError::ProxyTargetFailed);
                    }
                    Err(e) => {
                        warn!("IPFS gateway failed: {}", e);
                        last_error = Some("IPFS Gateway Error");
                        continue;
                    }
                }
            }
            DnsRecord::PeerId(peer_str) => {
                info!("Routing {} to PeerId: {}", full_domain, peer_str);
                let peer_id = match PeerId::from_str(&peer_str) {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("Invalid PeerId in zone: {}", peer_str);
                        last_error = Some("Invalid PeerId in zone");
                        continue;
                    }
                };

                let proxy_req = ProxyRequest {
                    method: method.clone().into(),
                    path: path.clone().into(),
                    headers: proxy_headers.clone(),
                    body: body_bytes.clone().into(),
                };

                let (tx, rx) = tokio::sync::oneshot::channel();
                if channel
                    .send(NetworkCommand::SendProxyRequest {
                        peer_id,
                        request: proxy_req,
                        resp: tx,
                    })
                    .await
                    .is_err()
                {
                    last_error = Some("Failed to send proxy request to swarm");
                    continue;
                }

                match rx.await {
                    Ok(Some(proxy_res)) => {
                        let mut builder =
                            axum::response::Response::builder().status(proxy_res.status);

                        for (k, v) in proxy_res.headers {
                            builder = builder.header(k.as_ref(), v.as_ref());
                        }

                        return builder
                            .body(Body::from(proxy_res.body))
                            .map_err(|_| AtlasError::ProxyTargetFailed);
                    }
                    _ => {
                        warn!("P2P Proxy Request Failed or Timed Out");
                        last_error = Some("P2P Proxy Request Failed or Timed Out");
                        continue;
                    }
                }
            }
            _ => continue,
        }
    }

    if let Some(err) = last_error {
        warn!("Proxy targets failed with last error: {}", err);
    }

    Err(AtlasError::ProxyTargetFailed)
}

/// Extracts the base domain and subdomains from a full host string, given a TLD
pub fn extract_base_domain_and_subdomain(
    full_domain: &str,
    clean_tld: &str,
) -> Option<(String, String)> {
    let domain_without_tld = full_domain.trim_end_matches(clean_tld);
    let parts: Vec<&str> = domain_without_tld.split('.').collect();

    if parts.is_empty() || parts[0].is_empty() {
        return None;
    }

    let registered_name = parts.last().unwrap();
    let base_domain = format!("{}{}", registered_name, clean_tld);
    let subdomain = if parts.len() > 1 {
        parts[..parts.len() - 1].join(".")
    } else {
        "@".to_string()
    };

    Some((base_domain, subdomain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_base_domain_and_subdomain("test.kin", ".kin"),
            Some(("test.kin".to_string(), "@".to_string()))
        );
        assert_eq!(
            extract_base_domain_and_subdomain("api.test.kin", ".kin"),
            Some(("test.kin".to_string(), "api".to_string()))
        );
        assert_eq!(
            extract_base_domain_and_subdomain("v1.api.test.kin", ".kin"),
            Some(("test.kin".to_string(), "v1.api".to_string()))
        );
        assert_eq!(extract_base_domain_and_subdomain(".kin", ".kin"), None);
    }
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_extract_domain_proptest(subdomain in "[a-z0-9]+(\\.[a-z0-9]+)*", name in "[a-z0-9]+", tld in "\\.[a-z]+") {
            let full_domain = format!("{}.{}{}", subdomain, name, tld);
            let result = extract_base_domain_and_subdomain(&full_domain, &tld);

            assert!(result.is_some());
            let (base, sub) = result.unwrap();

            assert_eq!(base, format!("{}{}", name, tld));
            assert_eq!(sub, subdomain);
        }

        #[test]
        fn test_extract_domain_no_subdomain(name in "[a-z0-9]+", tld in "\\.[a-z]+") {
            let full_domain = format!("{}{}", name, tld);
            let result = extract_base_domain_and_subdomain(&full_domain, &tld);

            assert!(result.is_some());
            let (base, sub) = result.unwrap();

            assert_eq!(base, format!("{}{}", name, tld));
            assert_eq!(sub, "@");
        }
    }
}
