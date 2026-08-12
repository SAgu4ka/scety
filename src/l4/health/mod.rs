pub mod active;
pub mod passive;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::l4::backend::BackendNode;
pub use active::ActiveHealthChecker;
pub use passive::PassiveHealthTracker;

pub struct HealthOrchestrator;

impl HealthOrchestrator {
    pub fn spawn_blocking(
        nodes: Vec<Arc<BackendNode>>,
        interval: Duration,
        timeout: Duration,
        shutdown: Arc<AtomicBool>,
    ) {
        ActiveHealthChecker::start_loop_blocking(nodes, interval, timeout, shutdown);
    }
}
