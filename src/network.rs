#![allow(missing_docs)]
use libp2p::{
    identify, identity, kad, noise, request_response, tcp, yamux, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::error::AtlasError;
use crate::types::{ProxyRequest, ProxyResponse};

/// Command sent to a swarm background task
pub enum NetworkCommand {
    /// Retrieve a DHT record
    GetRecord {
        /// Domain string
        domain: String,
        /// Response channel
        resp: oneshot::Sender<Option<Vec<u8>>>,
    },
    /// Look up a peer's IP
    LookupPeer {
        /// Peer ID string
        peer_id_str: String,
        /// Response channel
        resp: oneshot::Sender<Option<std::net::IpAddr>>,
    },
    /// Send a proxy request over libp2p
    SendProxyRequest {
        /// Target Peer ID
        peer_id: PeerId,
        /// Proxy Request payload
        request: ProxyRequest,
        /// Response channel
        resp: oneshot::Sender<Option<ProxyResponse>>,
    },
}

/// The libp2p network behaviour combining DHT and Request-Response
#[allow(missing_docs)]
#[derive(libp2p::swarm::NetworkBehaviour)]
pub struct AtlasBehaviour {
    /// Kademlia DHT
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    /// Request-Response Protocol
    pub request_response: request_response::cbor::Behaviour<ProxyRequest, ProxyResponse>,
    /// Identify Protocol
    pub identify: identify::Behaviour,
}

/// The kinetic atlas node
pub struct KineticAtlasNode {
    swarm: Swarm<AtlasBehaviour>,
    network_id: String,
}

impl KineticAtlasNode {
    /// Create a new KineticAtlasNode
    pub fn new(network_id: String, bootstrap_nodes: Vec<String>) -> Result<Self, AtlasError> {
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());

        info!(
            "Initializing libp2p for network {} as peer {}",
            network_id, local_peer_id
        );

        let kad_protocol_name =
            StreamProtocol::try_from_owned(format!("/{}/kad/2.0.0", network_id)).map_err(|e| {
                AtlasError::NetworkInitFailed(format!("Invalid protocol name: {}", e))
            })?;

        let proxy_protocol_name =
            StreamProtocol::try_from_owned(format!("/{}/proxy/1.0.0", network_id)).map_err(
                |e| AtlasError::NetworkInitFailed(format!("Invalid proxy protocol name: {}", e)),
            )?;

        let mut swarm = SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| AtlasError::NetworkInitFailed(format!("Transport config failed: {}", e)))?
            .with_dns()
            .map_err(|e| AtlasError::NetworkInitFailed(format!("DNS config failed: {}", e)))?
            .with_behaviour(move |key| {
                // Kademlia Configuration
                let mut kad_cfg = kad::Config::default();
                kad_cfg.set_protocol_names(vec![kad_protocol_name]);
                kad_cfg.set_max_packet_size(102400); // 100 KB
                kad_cfg.set_query_timeout(Duration::from_secs(5));

                let store = kad::store::MemoryStore::new(key.public().to_peer_id());
                let mut kademlia =
                    kad::Behaviour::with_config(key.public().to_peer_id(), store, kad_cfg);
                kademlia.set_mode(Some(kad::Mode::Client));

                // Request-Response Configuration
                let protocols =
                    std::iter::once((proxy_protocol_name, request_response::ProtocolSupport::Full));
                let rr_config = request_response::Config::default()
                    .with_request_timeout(Duration::from_secs(15));

                let request_response = request_response::cbor::Behaviour::new(protocols, rr_config);

                // Identify Configuration
                let identify_config = identify::Config::new("/atlas/1.0.0".into(), key.public());
                let identify = identify::Behaviour::new(identify_config);

                AtlasBehaviour {
                    kad: kademlia,
                    request_response,
                    identify,
                }
            })
            .map_err(|e| AtlasError::NetworkInitFailed(format!("Behaviour config failed: {}", e)))?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let mut dials = 0;
        let max_dials = 8; // Dial at most 8 nodes explicitly to prevent outbound spam
        for node_str in bootstrap_nodes {
            if let Ok(multiaddr) = node_str.parse::<libp2p::Multiaddr>() {
                if let Some(libp2p::multiaddr::Protocol::P2p(peer_id)) = multiaddr.iter().last() {
                    // Always add to routing table
                    swarm
                        .behaviour_mut()
                        .kad
                        .add_address(&peer_id, multiaddr.clone());

                    if dials < max_dials {
                        if let Err(e) = swarm.dial(multiaddr.clone()) {
                            warn!("Failed to dial bootstrap node {}: {:?}", multiaddr, e);
                        } else {
                            dials += 1;
                        }
                    }
                }
            }
        }

        if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
            warn!(
                "Failed to bootstrap Kademlia DHT for {}: {:?}",
                network_id, e
            );
        }

        Ok(Self { swarm, network_id })
    }

    /// Starts the node's background event loop
    pub async fn run(mut self, mut query_rx: mpsc::Receiver<NetworkCommand>) {
        use libp2p::futures::StreamExt;
        let mut pending_queries = std::collections::HashMap::new();
        let mut pending_dials = std::collections::HashMap::new();
        let mut pending_requests = std::collections::HashMap::new();

        loop {
            tokio::select! {
                cmd_opt = query_rx.recv() => {
                    match cmd_opt {
                        Some(NetworkCommand::GetRecord { domain, resp }) => {
                            info!("Network {} querying DHT for domain: {}", self.network_id, domain);
                            let key = kad::RecordKey::new(&domain);
                            let query_id = self.swarm.behaviour_mut().kad.get_record(key);
                            pending_queries.insert(query_id, resp);
                        }
                        Some(NetworkCommand::LookupPeer { peer_id_str, resp }) => {
                            if let Ok(peer_id) = peer_id_str.parse::<PeerId>() {
                                info!("Network {} dialing peer {} for IP resolution", self.network_id, peer_id);
                                if self.swarm.dial(peer_id).is_ok() {
                                    pending_dials.insert(peer_id, resp);
                                } else {
                                    let _ = resp.send(None);
                                }
                            } else {
                                let _ = resp.send(None);
                            }
                        }
                        Some(NetworkCommand::SendProxyRequest { peer_id, request, resp }) => {
                            info!("Network {} sending proxy request to peer {}", self.network_id, peer_id);
                            let req_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, request);
                            pending_requests.insert(req_id, resp);
                        }
                        None => {
                            info!("Network {} channel closed. Shutting down libp2p node.", self.network_id);
                            break;
                        }
                    }
                }
                event = self.swarm.select_next_some() => {
                    match event {
                        libp2p::swarm::SwarmEvent::Behaviour(AtlasBehaviourEvent::Kad(kad::Event::OutboundQueryProgressed { id, result, .. })) => {
                            if let Some(resp_tx) = pending_queries.remove(&id) {
                                match result {
                                    kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))) => {
                                        let _ = resp_tx.send(Some(record.record.value));
                                    }
                                    _ => {
                                        let _ = resp_tx.send(None);
                                    }
                                }
                            }
                        }
                        libp2p::swarm::SwarmEvent::Behaviour(AtlasBehaviourEvent::RequestResponse(request_response::Event::Message { peer: _, message })) => {
                            match message {
                                request_response::Message::Response { request_id, response } => {
                                    if let Some(resp_tx) = pending_requests.remove(&request_id) {
                                        let _ = resp_tx.send(Some(response));
                                    }
                                }
                                request_response::Message::Request { .. } => {
                                    // Atlas is a client only, it doesn't respond to incoming proxy requests
                                }
                            }
                        }
                        libp2p::swarm::SwarmEvent::Behaviour(AtlasBehaviourEvent::RequestResponse(request_response::Event::OutboundFailure { request_id, error, .. })) => {
                            warn!("Proxy request failed: {:?}", error);
                            if let Some(resp_tx) = pending_requests.remove(&request_id) {
                                let _ = resp_tx.send(None);
                            }
                        }
                        libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            if let Some(resp_tx) = pending_dials.remove(&peer_id) {
                                let multiaddr = endpoint.get_remote_address();
                                let mut ip = None;
                                for protocol in multiaddr.iter() {
                                    match protocol {
                                        libp2p::multiaddr::Protocol::Ip4(ipv4) => ip = Some(std::net::IpAddr::V4(ipv4)),
                                        libp2p::multiaddr::Protocol::Ip6(ipv6) => ip = Some(std::net::IpAddr::V6(ipv6)),
                                        _ => {}
                                    }
                                }
                                let _ = resp_tx.send(ip);
                            }
                        }
                        libp2p::swarm::SwarmEvent::OutgoingConnectionError { peer_id: Some(peer_id), .. } => {
                            if let Some(resp_tx) = pending_dials.remove(&peer_id) {
                                let _ = resp_tx.send(None);
                            }
                        }
                        libp2p::swarm::SwarmEvent::Behaviour(AtlasBehaviourEvent::Identify(event)) => {
                            info!("Identify event: {:?}", event);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
