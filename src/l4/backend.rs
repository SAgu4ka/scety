use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tracing::{debug, warn};

#[derive(Debug)]
pub struct BackendNode {
    pub addr: SocketAddr,
    pub weight: u32,
    pub is_healthy: AtomicBool,
    pub active_conns: AtomicUsize,
    pub consecutive_successes: AtomicUsize,
    pub consecutive_failures: AtomicUsize,
    pub total_conns: AtomicU64,
}

impl BackendNode {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            weight: 1,
            is_healthy: AtomicBool::new(true),
            active_conns: AtomicUsize::new(0),
            consecutive_successes: AtomicUsize::new(0),
            consecutive_failures: AtomicUsize::new(0),
            total_conns: AtomicU64::new(0),
        }
    }

    /// Called when an operation to this backend succeeded.
    /// If `rise_threshold` consecutive successes observed, node is marked healthy.
    pub fn mark_success(&self, rise_threshold: usize) {
        // clear failures, increment successes with release semantics
        self.consecutive_failures.store(0, Ordering::Release);
        let succ = self.consecutive_successes.fetch_add(1, Ordering::AcqRel) + 1;
        if succ >= rise_threshold {
            self.is_healthy.store(true, Ordering::Release);
            self.consecutive_successes.store(0, Ordering::Release);
            debug!(upstream = %self.addr, "Backend marked healthy after {} successes", rise_threshold);
        }
    }

    /// Called when an operation to this backend failed.
    /// If `fall_threshold` consecutive failures observed, node is marked unhealthy.
    pub fn mark_failure(&self, fall_threshold: usize) {
        // clear successes, increment failures with acquire/release semantics
        self.consecutive_successes.store(0, Ordering::Release);
        let fails = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
        if fails >= fall_threshold {
            self.is_healthy.store(false, Ordering::Release);
            warn!(upstream = %self.addr, "Backend marked unhealthy after {} failures", fall_threshold);
        }
    }
}

pub struct ConnectionGuard {
    node: Arc<BackendNode>,
}

impl ConnectionGuard {
    pub fn new(node: Arc<BackendNode>) -> Self {
        node.active_conns.fetch_add(1, Ordering::Relaxed);
        node.total_conns.fetch_add(1, Ordering::Relaxed);
        Self { node }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.node.active_conns.fetch_sub(1, Ordering::Relaxed);
    }
}
