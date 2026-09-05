#![allow(dead_code)]

pub mod backend;
pub mod config;
pub mod engine;
pub mod health;
pub mod lb;

pub use config::L4ServiceConfig;

pub struct L4Module;

impl crate::core::runtime::ProxyModule for L4Module {
    const PROXY_TYPE: &'static str = "L4";
}

pub struct L4ServiceManager;

impl L4ServiceManager {
    pub fn start(
        configs: Vec<L4ServiceConfig>,
    ) -> Vec<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        crate::l4::engine::start_l4_listen(configs)
    }

    pub fn start_mio(
        configs: Vec<L4ServiceConfig>,
    ) -> Vec<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        crate::l4::engine::start_mio_listen(configs)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::str::FromStr;
    use std::sync::Arc;

    use crate::l4::backend::{BackendNode, ConnectionGuard};
    use crate::l4::engine::proxy_protocol::ProxyProtocol;
    use crate::l4::lb::LoadBalancer;
    use crate::l4::lb::{LeastConnectionsLB, RoundRobinLB};

    #[test]
    fn test_connection_guard_counts() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);
        let node = Arc::new(BackendNode::new(addr));

        assert_eq!(
            node.active_conns.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        {
            let _g = ConnectionGuard::new(node.clone());
            assert_eq!(
                node.active_conns.load(std::sync::atomic::Ordering::Relaxed),
                1
            );
        }
        assert_eq!(
            node.active_conns.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn test_mark_failure_threshold() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);
        let node = BackendNode::new(addr);
        assert!(node.is_healthy.load(std::sync::atomic::Ordering::Relaxed));
        node.mark_failure(2);
        assert!(node.is_healthy.load(std::sync::atomic::Ordering::Relaxed));
        node.mark_failure(2);
        assert!(!node.is_healthy.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_round_robin_selection() {
        let addrs = (10000..10003)
            .map(|p| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p))
            .collect::<Vec<_>>();
        let nodes = addrs
            .iter()
            .map(|a| Arc::new(BackendNode::new(*a)))
            .collect();
        let lb = RoundRobinLB::new(nodes);
        let c1 = lb
            .select(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1))
            .unwrap();
        let c2 = lb
            .select(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1))
            .unwrap();
        let c3 = lb
            .select(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1))
            .unwrap();
        assert_ne!(c1.addr, c2.addr);
        assert_ne!(c2.addr, c3.addr);
    }

    #[test]
    fn test_least_conn_selection() {
        let addrs = (11000..11003)
            .map(|p| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p))
            .collect::<Vec<_>>();
        let nodes = addrs
            .iter()
            .map(|a| Arc::new(BackendNode::new(*a)))
            .collect::<Vec<_>>();

        nodes[0]
            .active_conns
            .fetch_add(5, std::sync::atomic::Ordering::Relaxed);
        let lb = LeastConnectionsLB::new(nodes.clone());
        let sel = lb
            .select(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1))
            .unwrap();
        assert_eq!(sel.addr, nodes[1].addr);
    }

    #[test]
    fn test_proxy_v2_header_lengths() {
        let client = SocketAddr::from_str("127.0.0.1:12345").unwrap();
        let dest = SocketAddr::from_str("127.0.0.1:80").unwrap();
        let h = ProxyProtocol::build_v2_header(client, dest);
        assert_eq!(h.len(), 28);
        assert!(h.starts_with(b"\r\n\r\n\x00\r\nQUIT\n"));
    }
}
