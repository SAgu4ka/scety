pub mod ip_hash;
pub mod least_conn;
pub mod round_robin;

use std::net::SocketAddr;
use std::sync::Arc;

use crate::l4::backend::BackendNode;
use crate::l4::config::LbStrategy;
pub use ip_hash::IpHashLB;
pub use least_conn::LeastConnectionsLB;
pub use round_robin::RoundRobinLB;

pub trait LoadBalancer: Send + Sync {
    fn select(&self, client_addr: SocketAddr) -> Option<Arc<BackendNode>>;
}

pub fn create_load_balancer(
    strategy: LbStrategy,
    nodes: Vec<Arc<BackendNode>>,
) -> Arc<dyn LoadBalancer> {
    match strategy {
        LbStrategy::RoundRobin => Arc::new(RoundRobinLB::new(nodes)),
        LbStrategy::LeastConnections => Arc::new(LeastConnectionsLB::new(nodes)),
        LbStrategy::IpHash => Arc::new(IpHashLB::new(nodes)),
    }
}
