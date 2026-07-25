use libp2p::{
    identity, kad, noise, tcp, yamux, PeerId, Swarm, SwarmBuilder,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

pub enum NetworkCommand {
    GetRecord {
        domain: String,
        resp: oneshot::Sender<Option<Vec<u8>>>,
    },
    LookupPeer {
        peer_id_str: String,
        resp: oneshot::Sender<Option<std::net::IpAddr>>,
    },
}

pub struct KineticAtlasNode {
    swarm: Swarm<kad::Behaviour<kad::store::MemoryStore>>,
    network_id: String,
}

impl KineticAtlasNode {
    pub fn new(network_id: String, bootstrap_nodes: Vec<String>) -> anyhow::Result<Self> {
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        
        info!("Initializing libp2p for network {} as peer {}", network_id, local_peer_id);

        let protocol_name = libp2p::StreamProtocol::try_from_owned(format!(
            "/{}/kad/2.0.0",
            network_id
        )).map_err(|e| anyhow::anyhow!("Invalid protocol name: {}", e))?;

        let mut swarm = SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_dns()?
            .with_behaviour(move |key| {
                let mut cfg = kad::Config::default();
                cfg.set_protocol_names(vec![protocol_name]);
                
                let store = kad::store::MemoryStore::new(key.public().to_peer_id());
                let mut kademlia = kad::Behaviour::with_config(key.public().to_peer_id(), store, cfg);
                kademlia.set_mode(Some(kad::Mode::Client));
                kademlia
            })?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        for node_str in bootstrap_nodes {
            if let Ok(multiaddr) = node_str.parse::<libp2p::Multiaddr>() {
                if let Some(libp2p::multiaddr::Protocol::P2p(peer_id)) = multiaddr.iter().last() {
                    swarm.behaviour_mut().add_address(&peer_id, multiaddr.clone());
                    if let Err(e) = swarm.dial(multiaddr.clone()) {
                        warn!("Failed to dial bootstrap node {}: {:?}", multiaddr, e);
                    }
                }
            }
        }
        
        if let Err(e) = swarm.behaviour_mut().bootstrap() {
            warn!("Failed to bootstrap Kademlia DHT for {}: {:?}", network_id, e);
        }

        Ok(Self {
            swarm,
            network_id,
        })
    }

    pub async fn run(mut self, mut query_rx: mpsc::Receiver<NetworkCommand>) {
        use libp2p::futures::StreamExt;
        let mut pending_queries = std::collections::HashMap::new();
        let mut pending_dials = std::collections::HashMap::new();

        loop {
            tokio::select! {
                Some(cmd) = query_rx.recv() => {
                    match cmd {
                        NetworkCommand::GetRecord { domain, resp } => {
                            info!("Network {} querying DHT for domain: {}", self.network_id, domain);
                            let key = kad::RecordKey::new(&domain);
                            let query_id = self.swarm.behaviour_mut().get_record(key);
                            pending_queries.insert(query_id, resp);
                        }
                        NetworkCommand::LookupPeer { peer_id_str, resp } => {
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
                    }
                }
                event = self.swarm.select_next_some() => {
                    match event {
                        libp2p::swarm::SwarmEvent::Behaviour(kad::Event::OutboundQueryProgressed { id, result, .. }) => {
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
                        _ => {}
                    }
                }
            }
        }
    }
}
