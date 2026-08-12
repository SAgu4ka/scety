pub mod mio_worker;
pub mod pipe;
pub mod proxy_protocol;
pub mod timer_wheel;
pub mod udp_sessions;

use tracing::info;

use crate::l4::config::L4ServiceConfig;
use crate::l4::engine::mio_worker::MioL4Worker;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub fn start_l4_listen(configs: Vec<L4ServiceConfig>) -> Vec<Arc<AtomicBool>> {
    start_mio_listen(configs)
}

pub fn start_mio_listen(configs: Vec<L4ServiceConfig>) -> Vec<Arc<AtomicBool>> {
    let mut handles = Vec::new();
    for config in configs {
        info!(bind = %config.bind, service = %config.name, "Initializing L4 engine service");
        let shutdown = Arc::new(AtomicBool::new(false));
        MioL4Worker::spawn_thread_per_core(config, shutdown.clone());
        handles.push(shutdown);
    }
    handles
}
