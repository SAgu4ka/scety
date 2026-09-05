use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

use mio::net::TcpListener as MioTcpListener;
use mio::{Events, Interest, Poll, Token, Waker};
use slab::Slab;
use socket2::{Domain, Protocol, Socket, Type};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use tracing::{debug, error, info};

use crate::l4::config::L4ServiceConfig;
use crate::l4::engine::pipe::PipeFd;
use crate::l4::engine::udp_sessions::UdpSessionMap;
use crate::l4::health::PassiveHealthTracker;
use crate::l4::lb::create_load_balancer;

use mio::unix::SourceFd as MioSourceFd;

struct Conn {
    client_fd: RawFd,
    upstream_fd: RawFd,
    state: ConnState,
    backend: Arc<crate::l4::backend::BackendNode>,
    _connection_guard: crate::l4::backend::ConnectionGuard,
    connected_at: std::time::Instant,
    last_activity: std::time::Instant,
}

#[derive(PartialEq, Eq)]
enum ConnState {
    Connecting,
    Established,
}

fn close_conn(poll: &Poll, conn: &Conn) {
    let mut client_source = MioSourceFd(&conn.client_fd);
    let mut upstream_source = MioSourceFd(&conn.upstream_fd);
    let _ = poll.registry().deregister(&mut client_source);
    let _ = poll.registry().deregister(&mut upstream_source);
    unsafe {
        libc::close(conn.client_fd);
        libc::close(conn.upstream_fd);
    }
}

pub struct MioL4Worker;

impl MioL4Worker {
    pub fn spawn_thread_per_core(
        config: L4ServiceConfig,
        shutdown: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let cores = num_cpus::get();
            info!(service = %config.name, cores = cores, "Starting Thread-per-Core worker");

            let mut handles = Vec::with_capacity(cores);

            for i in 0..cores {
                let cfg = config.clone();
                let sdn = shutdown.clone();
                let handle = thread::spawn(move || {
                    if let Err(e) = Self::worker_loop(i, cfg, sdn) {
                        error!(worker = i, error = %e, "Worker loop exited with error");
                    }
                });
                handles.push(handle);
            }

            for h in handles {
                let _ = h.join();
            }
        })
    }

    fn worker_loop(
        thread_id: usize,
        config: L4ServiceConfig,
        shutdown: Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        if config.protocol == crate::l4::config::L4Protocol::Udp {
            let domain = if config.bind.is_ipv4() {
                Domain::IPV4
            } else {
                Domain::IPV6
            };
            let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
            socket.set_reuse_address(true)?;
            if config.reuse_port {
                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    let fd = socket.as_raw_fd();
                    let val: libc::c_int = 1;
                    unsafe {
                        let _ = libc::setsockopt(
                            fd,
                            libc::SOL_SOCKET,
                            libc::SO_REUSEPORT,
                            &val as *const _ as *const libc::c_void,
                            std::mem::size_of_val(&val) as libc::socklen_t,
                        );
                    }
                }
            }
            socket.set_nonblocking(true)?;
            socket.bind(&config.bind.into())?;

            let raw = socket.into_raw_fd();
            let std_udp = unsafe { std::net::UdpSocket::from_raw_fd(raw) };
            std_udp.set_nonblocking(true)?;
            let mut mio_udp = mio::net::UdpSocket::from_std(std_udp);

            let mut poll = Poll::new()?;
            const UDP_SOCKET: Token = Token(0);
            const WAKER: Token = Token(usize::MAX);
            poll.registry()
                .register(&mut mio_udp, UDP_SOCKET, Interest::READABLE)?;

            let udp_map = Arc::new(std::sync::Mutex::new(UdpSessionMap::new(
                config.idle_timeout,
            )));
            let wheel = Arc::new(std::sync::Mutex::new(
                crate::l4::engine::timer_wheel::TimerWheel::new(64),
            ));

            let nodes: Vec<Arc<crate::l4::backend::BackendNode>> = config
                .upstreams
                .iter()
                .map(|addr| Arc::new(crate::l4::backend::BackendNode::new(*addr)))
                .collect();

            crate::l4::health::HealthOrchestrator::spawn_blocking(
                nodes.clone(),
                Duration::from_secs(5),
                config.connect_timeout,
                shutdown.clone(),
            );

            let lb = create_load_balancer(config.lb_strategy.clone(), nodes);
            let passive_tracker = PassiveHealthTracker::new(3);

            let w = Waker::new(poll.registry(), WAKER)?;
            let waker = Arc::new(w);
            {
                let sdn = shutdown.clone();
                let wk = waker.clone();
                let udp_map_clone = udp_map.clone();
                let wheel_clone = wheel.clone();
                std::thread::spawn(move || {
                    let mut last_evict = std::time::Instant::now();
                    loop {
                        if sdn.load(Ordering::Relaxed) {
                            let _ = wk.wake();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(250));
                        if last_evict.elapsed() >= Duration::from_secs(1) {
                            last_evict = std::time::Instant::now();
                            if let Ok(mut m) = udp_map_clone.lock() {
                                let removed = m.evict_expired();
                                if removed > 0 {
                                    let _ = wk.wake();
                                }
                            }
                            if let Ok(mut w) = wheel_clone.lock() {
                                let expired = w.tick();
                                if !expired.is_empty() {
                                    let _ = wk.wake();
                                }
                            }
                        }
                    }
                });
            }

            let mut events = Events::with_capacity(1024);
            let mut buf = vec![0u8; 65536];

            while !shutdown.load(Ordering::Relaxed) {
                poll.poll(&mut events, Some(Duration::from_millis(200)))?;
                for ev in &events {
                    if ev.token() == WAKER {
                        continue;
                    }

                    if ev.token() == UDP_SOCKET && ev.is_readable() {
                        loop {
                            match mio_udp.recv_from(&mut buf) {
                                Ok((n, src)) => {
                                    let data = &buf[..n];
                                    if let Some(node) = lb.select(src) {
                                        let mut m = udp_map.lock().unwrap();
                                        if let Some(up_fd) = m.touch(src) {
                                            let sret = unsafe {
                                                libc::send(
                                                    up_fd,
                                                    data.as_ptr() as *const libc::c_void,
                                                    data.len(),
                                                    0,
                                                )
                                            };
                                            if sret >= 0 {
                                                passive_tracker.record_success(&node);
                                            } else {
                                                passive_tracker.record_failure(&node);
                                                debug!(worker = thread_id, error = %std::io::Error::last_os_error(), "UDP send failed");
                                            }
                                            if let Some(idx) = m.get_idx(src) {
                                                let _ = wheel.lock().map(|mut w| {
                                                    w.schedule(
                                                        idx,
                                                        config.idle_timeout.as_secs() as usize,
                                                    )
                                                });
                                            }
                                        } else {
                                            let domain = if node.addr.is_ipv4() {
                                                Domain::IPV4
                                            } else {
                                                Domain::IPV6
                                            };
                                            let sock = match Socket::new(
                                                domain,
                                                Type::DGRAM,
                                                Some(Protocol::UDP),
                                            ) {
                                                Ok(s) => s,
                                                Err(e) => {
                                                    error!(error = %e, "Failed to create upstream udp socket");
                                                    continue;
                                                }
                                            };
                                            sock.set_nonblocking(true)?;
                                            let _ = sock.connect(&node.addr.into());
                                            let up_fd = sock.into_raw_fd();

                                            let mut src_fd = MioSourceFd(&up_fd);
                                            let token = Token(1 + up_fd as usize);
                                            poll.registry().register(
                                                &mut src_fd,
                                                token,
                                                Interest::READABLE,
                                            )?;

                                            let idx = m.insert(src, up_fd);
                                            let ticks = config.idle_timeout.as_secs() as usize;
                                            let _ =
                                                wheel.lock().map(|mut w| w.schedule(idx, ticks));

                                            let sret = unsafe {
                                                libc::send(
                                                    up_fd,
                                                    data.as_ptr() as *const libc::c_void,
                                                    data.len(),
                                                    0,
                                                )
                                            };
                                            if sret >= 0 {
                                                passive_tracker.record_success(&node);
                                            } else {
                                                passive_tracker.record_failure(&node);
                                                debug!(worker = thread_id, error = %std::io::Error::last_os_error(), "UDP send failed");
                                            }
                                        }
                                    } else {
                                        debug!(worker = thread_id, client = %src, "No healthy UDP upstreams available");
                                    }
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                                Err(e) => {
                                    error!(worker = thread_id, error = %e, "UDP recv error");
                                    break;
                                }
                            }
                        }
                    } else {
                        let tok = ev.token().0;
                        if tok >= 1 {
                            let up_fd = tok - 1;
                            let mut rbuf = vec![0u8; 65536];
                            let rc = unsafe {
                                libc::recv(
                                    up_fd as i32,
                                    rbuf.as_mut_ptr() as *mut libc::c_void,
                                    rbuf.len(),
                                    0,
                                )
                            };
                            if rc > 0 {
                                let rn = rc as usize;
                                if let Ok(mut m) = udp_map.lock()
                                    && let Some((idx, client_addr)) =
                                        m.find_by_upstream_fd(up_fd as RawFd)
                                {
                                    let _ = mio_udp.send_to(&rbuf[..rn], client_addr);
                                    let _ = m.touch(client_addr);
                                    let ticks = config.idle_timeout.as_secs() as usize;
                                    let _ = wheel.lock().map(|mut w| w.schedule(idx, ticks));
                                }
                            } else if rc == 0 {
                                if let Ok(mut m) = udp_map.lock()
                                    && let Some((i, _)) = m.find_by_upstream_fd(up_fd as RawFd)
                                {
                                    m.remove_by_idx(i);
                                }
                                let mut src = MioSourceFd(&(up_fd as RawFd));
                                let _ = poll.registry().deregister(&mut src);
                                unsafe {
                                    libc::close(up_fd as RawFd);
                                }
                            } else {
                                let err = std::io::Error::last_os_error();
                                if err.raw_os_error() == Some(libc::EAGAIN) {
                                    continue;
                                }
                                error!(worker = thread_id, error = %err, "Error reading upstream udp socket");
                            }
                        }
                    }
                }
                if let Ok(mut w) = wheel.lock() {
                    let expired = w.tick();
                    if !expired.is_empty()
                        && let Ok(mut m) = udp_map.lock()
                    {
                        for idx in expired {
                            if let Some(sess) = m.remove_by_idx(idx) {
                                let mut src = MioSourceFd(&sess.upstream_fd);
                                let _ = poll.registry().deregister(&mut src);
                                unsafe {
                                    libc::close(sess.upstream_fd);
                                }
                            }
                        }
                    }
                }
            }

            info!(worker = thread_id, "UDP worker shutting down");
            return Ok(());
        }

        let domain = if config.bind.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_reuse_address(true)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket.as_raw_fd();
            let val: libc::c_int = 1;
            unsafe {
                let _ = libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEPORT,
                    &val as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&val) as libc::socklen_t,
                );
            }
        }

        socket.set_nonblocking(true)?;
        socket.bind(&config.bind.into())?;
        socket.listen(1024)?;

        let raw = socket.into_raw_fd();
        let std_listener = unsafe { std::net::TcpListener::from_raw_fd(raw) };
        std_listener.set_nonblocking(true)?;
        let mut mio_listener = MioTcpListener::from_std(std_listener);

        let mut poll = Poll::new()?;
        const LISTENER: Token = Token(0);
        const WAKER: Token = Token(usize::MAX);
        poll.registry()
            .register(&mut mio_listener, LISTENER, Interest::READABLE)?;

        let udp_map = Arc::new(Mutex::new(UdpSessionMap::new(Duration::from_secs(60))));
        let wheel = Arc::new(Mutex::new(crate::l4::engine::timer_wheel::TimerWheel::new(
            64,
        )));

        let w = Waker::new(poll.registry(), WAKER)?;
        let waker = std::sync::Arc::new(w);
        {
            let sdn = shutdown.clone();
            let wk = waker.clone();
            let udp_map_clone = udp_map.clone();
            std::thread::spawn(move || {
                let mut last_evict = std::time::Instant::now();
                loop {
                    if sdn.load(Ordering::Relaxed) {
                        let _ = wk.wake();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                    if last_evict.elapsed() >= Duration::from_secs(1) {
                        last_evict = std::time::Instant::now();
                        if let Ok(mut m) = udp_map_clone.lock() {
                            let removed = m.evict_expired();
                            if removed > 0 {
                                let _ = wk.wake();
                                if let Ok(mut w) = wheel.lock() {
                                    let expired = w.tick();
                                    for idx in expired {
                                        let _ = m.remove_by_idx(idx);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        let mut events = Events::with_capacity(1024);

        let mut conns: Slab<Conn> = Slab::with_capacity(1024);

        let nodes: Vec<Arc<crate::l4::backend::BackendNode>> = config
            .upstreams
            .iter()
            .map(|addr| Arc::new(crate::l4::backend::BackendNode::new(*addr)))
            .collect();

        let lb = create_load_balancer(config.lb_strategy.clone(), nodes.clone());

        crate::l4::health::HealthOrchestrator::spawn_blocking(
            nodes.clone(),
            Duration::from_secs(5),
            config.connect_timeout,
            shutdown.clone(),
        );

        let pipe = PipeFd::new()?;

        while !shutdown.load(Ordering::Relaxed) {
            let now = std::time::Instant::now();
            let expired: Vec<usize> = conns
                .iter()
                .filter_map(|(index, conn)| {
                    let timed_out = match conn.state {
                        ConnState::Connecting => {
                            now.duration_since(conn.connected_at) >= config.connect_timeout
                        }
                        ConnState::Established => {
                            now.duration_since(conn.last_activity) >= config.idle_timeout
                        }
                    };
                    timed_out.then_some(index)
                })
                .collect();
            for index in expired {
                if let Some(conn) = conns.get(index) {
                    close_conn(&poll, conn);
                }
                let _ = conns.remove(index);
            }

            poll.poll(&mut events, Some(Duration::from_millis(200)))?;

            for ev in &events {
                if ev.token() == WAKER {
                    continue;
                }
                if ev.token() == LISTENER && ev.is_readable() {
                    loop {
                        match mio_listener.accept() {
                            Ok((stream, addr)) => {
                                let client_fd = stream.as_raw_fd();
                                std::mem::forget(stream);

                                let backend = match lb.select(addr) {
                                    Some(backend) => backend,
                                    None => {
                                        unsafe { libc::close(client_fd) };
                                        continue;
                                    }
                                };
                                let upstream_addr = backend.addr;

                                let domain = if upstream_addr.is_ipv4() {
                                    Domain::IPV4
                                } else {
                                    Domain::IPV6
                                };
                                let usock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
                                usock.set_tcp_nodelay(config.tcp_nodelay)?;
                                usock.set_nonblocking(true)?;
                                let _ = usock.connect(&upstream_addr.into());
                                let upstream_fd = usock.into_raw_fd();

                                let entry = conns.vacant_entry();
                                let key = entry.key();
                                let client_token = Token(1 + key * 2);
                                let upstream_token = Token(1 + key * 2 + 1);

                                let mut src_c = MioSourceFd(&client_fd);
                                let mut src_u = MioSourceFd(&upstream_fd);
                                poll.registry().register(
                                    &mut src_c,
                                    client_token,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;
                                poll.registry().register(
                                    &mut src_u,
                                    upstream_token,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;

                                entry.insert(Conn {
                                    client_fd,
                                    upstream_fd,
                                    state: ConnState::Connecting,
                                    backend: backend.clone(),
                                    _connection_guard: crate::l4::backend::ConnectionGuard::new(
                                        backend,
                                    ),
                                    connected_at: std::time::Instant::now(),
                                    last_activity: std::time::Instant::now(),
                                });
                                debug!(worker = thread_id, upstream = %addr, "Accepted connection, conn_id={}", key);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(e) => {
                                error!(worker = thread_id, error = %e, "Accept error");
                                break;
                            }
                        }
                    }
                } else {
                    let tok = ev.token().0;
                    if tok >= 1 {
                        let idx = (tok - 1) / 2;
                        let is_client = (tok - 1) % 2 == 0;
                        let mut remove_conn: Option<usize> = None;
                        if let Some(mut_conn) = conns.get_mut(idx) {
                            if !is_client
                                && ev.is_writable()
                                && mut_conn.state == ConnState::Connecting
                            {
                                let mut err: libc::c_int = 0;
                                let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
                                let rc = unsafe {
                                    libc::getsockopt(
                                        mut_conn.upstream_fd,
                                        libc::SOL_SOCKET,
                                        libc::SO_ERROR,
                                        &mut err as *mut _ as *mut libc::c_void,
                                        &mut len,
                                    )
                                };
                                if rc == 0 && err == 0 {
                                    mut_conn.state = ConnState::Established;
                                    mut_conn.backend.mark_success(1);
                                    debug!(worker = thread_id, conn = idx, "Upstream connected");
                                } else {
                                    mut_conn.backend.mark_failure(3);
                                    let mut src_c_close = MioSourceFd(&mut_conn.client_fd);
                                    let mut src_u_close = MioSourceFd(&mut_conn.upstream_fd);
                                    let _ = poll.registry().deregister(&mut src_c_close);
                                    let _ = poll.registry().deregister(&mut src_u_close);
                                    unsafe {
                                        libc::close(mut_conn.client_fd);
                                        libc::close(mut_conn.upstream_fd);
                                    }
                                    remove_conn = Some(idx);
                                    debug!(
                                        worker = thread_id,
                                        conn = idx,
                                        "Upstream connect failed"
                                    );
                                }
                            }

                            if ev.is_readable() && mut_conn.state == ConnState::Established {
                                let from = if is_client {
                                    mut_conn.client_fd
                                } else {
                                    mut_conn.upstream_fd
                                };
                                let to = if is_client {
                                    mut_conn.upstream_fd
                                } else {
                                    mut_conn.client_fd
                                };

                                loop {
                                    let n = unsafe {
                                        libc::splice(
                                            from,
                                            std::ptr::null_mut(),
                                            pipe.write_end(),
                                            std::ptr::null_mut(),
                                            65536,
                                            0,
                                        )
                                    };
                                    if n > 0 {
                                        mut_conn.last_activity = std::time::Instant::now();
                                        let mut rem = n;
                                        while rem > 0 {
                                            let w = unsafe {
                                                libc::splice(
                                                    pipe.read_end(),
                                                    std::ptr::null_mut(),
                                                    to,
                                                    std::ptr::null_mut(),
                                                    rem as usize,
                                                    0,
                                                )
                                            };
                                            if w > 0 {
                                                rem -= w;
                                            } else if w == 0 {
                                                let mut src_c_close =
                                                    MioSourceFd(&mut_conn.client_fd);
                                                let mut src_u_close =
                                                    MioSourceFd(&mut_conn.upstream_fd);
                                                let _ =
                                                    poll.registry().deregister(&mut src_c_close);
                                                let _ =
                                                    poll.registry().deregister(&mut src_u_close);
                                                unsafe {
                                                    libc::close(mut_conn.client_fd);
                                                    libc::close(mut_conn.upstream_fd);
                                                }
                                                remove_conn = Some(idx);
                                                debug!(
                                                    worker = thread_id,
                                                    conn = idx,
                                                    "Connection closed by peer"
                                                );
                                                break;
                                            } else {
                                                let e = std::io::Error::last_os_error();
                                                if e.raw_os_error() == Some(libc::EAGAIN) {
                                                    break;
                                                } else {
                                                    let mut src_c_close =
                                                        MioSourceFd(&mut_conn.client_fd);
                                                    let mut src_u_close =
                                                        MioSourceFd(&mut_conn.upstream_fd);
                                                    let _ = poll
                                                        .registry()
                                                        .deregister(&mut src_c_close);
                                                    let _ = poll
                                                        .registry()
                                                        .deregister(&mut src_u_close);
                                                    unsafe {
                                                        libc::close(mut_conn.client_fd);
                                                        libc::close(mut_conn.upstream_fd);
                                                    }
                                                    remove_conn = Some(idx);
                                                    debug!(
                                                        worker = thread_id,
                                                        conn = idx,
                                                        "splice error, closing connection"
                                                    );
                                                    break;
                                                }
                                            }
                                        }
                                        continue;
                                    } else if n == 0 {
                                        let mut src_c_close = MioSourceFd(&mut_conn.client_fd);
                                        let mut src_u_close = MioSourceFd(&mut_conn.upstream_fd);
                                        let _ = poll.registry().deregister(&mut src_c_close);
                                        let _ = poll.registry().deregister(&mut src_u_close);
                                        unsafe {
                                            libc::close(mut_conn.client_fd);
                                            libc::close(mut_conn.upstream_fd);
                                        }
                                        remove_conn = Some(idx);
                                        debug!(
                                            worker = thread_id,
                                            conn = idx,
                                            "Connection closed by peer"
                                        );
                                        break;
                                    } else {
                                        let e = std::io::Error::last_os_error();
                                        if e.raw_os_error() == Some(libc::EAGAIN) {
                                            break;
                                        } else {
                                            let mut src_c_close = MioSourceFd(&mut_conn.client_fd);
                                            let mut src_u_close =
                                                MioSourceFd(&mut_conn.upstream_fd);
                                            let _ = poll.registry().deregister(&mut src_c_close);
                                            let _ = poll.registry().deregister(&mut src_u_close);
                                            unsafe {
                                                libc::close(mut_conn.client_fd);
                                                libc::close(mut_conn.upstream_fd);
                                            }
                                            remove_conn = Some(idx);
                                            debug!(
                                                worker = thread_id,
                                                conn = idx,
                                                "splice error, closing connection"
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(ridx) = remove_conn {
                            let _ = conns.remove(ridx);
                            continue;
                        }
                    }
                }
            }
        }

        info!(worker = thread_id, "Worker shutting down");
        Ok(())
    }
}
