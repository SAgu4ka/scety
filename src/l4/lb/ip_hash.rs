use std::hash::{BuildHasher, BuildHasherDefault, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;

use super::LoadBalancer;
use crate::l4::backend::BackendNode;

#[derive(Debug)]
pub struct IpHashLB {
    nodes: Vec<Arc<BackendNode>>,
    hasher_builder: BuildHasherDefault<fxhash::FxHasher>,
}

impl IpHashLB {
    pub fn new(nodes: Vec<Arc<BackendNode>>) -> Self {
        Self {
            nodes,
            hasher_builder: Default::default(),
        }
    }
}

impl LoadBalancer for IpHashLB {
    fn select(&self, client_addr: SocketAddr) -> Option<Arc<BackendNode>> {
        use std::net::IpAddr;
        let healthy: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| n.is_healthy.load(std::sync::atomic::Ordering::Relaxed))
            .collect();
        if healthy.is_empty() {
            return None;
        }

        let ip_bytes = match client_addr.ip() {
            IpAddr::V4(a) => a.octets().to_vec(),
            IpAddr::V6(a) => a.octets().to_vec(),
        };

        let mut hasher = self.hasher_builder.build_hasher();
        hasher.write(&ip_bytes);
        let h = hasher.finish() as usize;
        Some(healthy[h % healthy.len()].clone())
    }
}
