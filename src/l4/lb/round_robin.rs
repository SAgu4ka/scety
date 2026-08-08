use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::LoadBalancer;
use crate::l4::backend::BackendNode;

pub struct RoundRobinLB {
    nodes: Vec<Arc<BackendNode>>,
    index: AtomicUsize,
}

impl RoundRobinLB {
    pub fn new(nodes: Vec<Arc<BackendNode>>) -> Self {
        Self {
            nodes,
            index: AtomicUsize::new(0),
        }
    }
}

impl LoadBalancer for RoundRobinLB {
    fn select(&self, _client_addr: SocketAddr) -> Option<Arc<BackendNode>> {
        let healthy: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| n.is_healthy.load(Ordering::Relaxed))
            .collect();

        if healthy.is_empty() {
            return None;
        }

        let idx = self.index.fetch_add(1, Ordering::Relaxed) % healthy.len();
        Some(healthy[idx].clone())
    }
}
