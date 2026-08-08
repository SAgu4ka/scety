use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::LoadBalancer;
use crate::l4::backend::BackendNode;

pub struct LeastConnectionsLB {
    nodes: Vec<Arc<BackendNode>>,
}

impl LeastConnectionsLB {
    pub fn new(nodes: Vec<Arc<BackendNode>>) -> Self {
        Self { nodes }
    }
}

impl LoadBalancer for LeastConnectionsLB {
    fn select(&self, _client_addr: SocketAddr) -> Option<Arc<BackendNode>> {
        self.nodes
            .iter()
            .filter(|n| n.is_healthy.load(Ordering::Relaxed))
            .min_by_key(|n| n.active_conns.load(Ordering::Relaxed))
            .cloned()
    }
}
