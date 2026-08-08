use socket2::{Domain, Protocol, Socket, Type};
use std::os::fd::IntoRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, warn};

use crate::l4::backend::BackendNode;

pub struct ActiveHealthChecker;

impl ActiveHealthChecker {
    // Async tokio-based active checks removed — use blocking `start_loop_blocking`.

    /// Blocking variant used by mio worker: runs in a dedicated std thread.
    pub fn start_loop_blocking(
        nodes: Vec<Arc<BackendNode>>,
        interval: Duration,
        timeout: Duration,
        shutdown: Arc<AtomicBool>,
    ) {
        std::thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                let start = std::time::Instant::now();
                for node in &nodes {
                    let addr = node.addr;
                    let is_alive = Self::check_with_zero_linger_sync(addr, timeout);

                    let prev = node.is_healthy.swap(is_alive, Ordering::Relaxed);
                    if prev != is_alive {
                        if is_alive {
                            node.mark_success(1);
                            debug!(upstream = %addr, "Active check PASSED (node recovered)");
                        } else {
                            node.mark_failure(1);
                            warn!(upstream = %addr, "Active check FAILED (node marked down)");
                        }
                    }
                }
                // sleep until next interval, but break early if shutdown
                let elapsed = start.elapsed();
                if elapsed < interval {
                    let sleep_dur = interval - elapsed;
                    let mut slept = Duration::from_millis(0);
                    while slept < sleep_dur {
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                        let to_sleep = std::cmp::min(Duration::from_millis(200), sleep_dur - slept);
                        std::thread::sleep(to_sleep);
                        slept += to_sleep;
                    }
                }
            }
        });
    }

    fn check_with_zero_linger_sync(addr: std::net::SocketAddr, timeout: Duration) -> bool {
        let domain = if addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = match Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let _ = socket.set_linger(Some(Duration::ZERO));
        let _ = socket.set_nonblocking(true);

        match socket.connect(&addr.into()) {
            Ok(_) => true,
            Err(e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {
                let mut pfd = libc::pollfd {
                    fd: socket.into_raw_fd(),
                    events: libc::POLLOUT,
                    revents: 0,
                };
                let ret = unsafe { libc::poll(&mut pfd, 1, timeout.as_millis() as i32) };
                ret > 0
            }
            Err(_) => false,
        }
    }
}
