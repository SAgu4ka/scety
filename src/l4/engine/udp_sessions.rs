use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};

use slab::Slab;

pub struct UdpSession {
    pub client: SocketAddr,
    pub upstream_fd: RawFd,
    pub last_seen: Instant,
}

pub struct UdpSessionMap {
    slab: Slab<UdpSession>,
    map: HashMap<SocketAddr, usize>,
    timeout: Duration,
}

impl UdpSessionMap {
    pub fn new(timeout: Duration) -> Self {
        Self {
            slab: Slab::with_capacity(1024),
            map: HashMap::with_capacity(1024),
            timeout,
        }
    }

    pub fn len(&self) -> usize {
        self.slab.len()
    }

    pub fn insert(&mut self, client: SocketAddr, upstream_fd: RawFd) -> usize {
        if let Some(&idx) = self.map.get(&client)
            && let Some(sess) = self.slab.get_mut(idx)
        {
            sess.upstream_fd = upstream_fd;
            sess.last_seen = Instant::now();
            return idx;
        }

        let entry = self.slab.vacant_entry();
        let idx = entry.key();
        entry.insert(UdpSession {
            client,
            upstream_fd,
            last_seen: Instant::now(),
        });
        self.map.insert(client, idx);
        idx
    }

    pub fn touch(&mut self, client: SocketAddr) -> Option<RawFd> {
        if let Some(&idx) = self.map.get(&client)
            && let Some(sess) = self.slab.get_mut(idx)
        {
            sess.last_seen = Instant::now();
            return Some(sess.upstream_fd);
        }
        None
    }

    pub fn remove(&mut self, client: SocketAddr) -> Option<UdpSession> {
        if let Some(idx) = self.map.remove(&client) {
            return Some(self.slab.remove(idx));
        }
        None
    }

    pub fn remove_by_idx(&mut self, idx: usize) -> Option<UdpSession> {
        // Slab::remove returns the removed value (not Option), but catch panics via checking contains
        if idx < self.slab.capacity() && self.slab.contains(idx) {
            let sess = self.slab.remove(idx);
            self.map.remove(&sess.client);
            return Some(sess);
        }
        None
    }

    /// Return the slab index for a client address if present.
    pub fn get_idx(&self, client: SocketAddr) -> Option<usize> {
        self.map.get(&client).cloned()
    }

    /// Find a session by its upstream_fd and return (idx, client)
    pub fn find_by_upstream_fd(&self, fd: RawFd) -> Option<(usize, SocketAddr)> {
        for (idx, sess) in self.slab.iter() {
            if sess.upstream_fd == fd {
                return Some((idx, sess.client));
            }
        }
        None
    }

    /// Evict sessions that are idle longer than timeout. Returns number removed.
    pub fn evict_expired(&mut self) -> usize {
        let now = Instant::now();
        let mut removed = Vec::new();
        for (idx, sess) in self.slab.iter() {
            if now.duration_since(sess.last_seen) > self.timeout {
                removed.push((idx, sess.client));
            }
        }
        for (idx, client) in removed.iter() {
            self.slab.remove(*idx);
            self.map.remove(client);
        }
        removed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn insert_and_touch() {
        let mut m = UdpSessionMap::new(Duration::from_secs(1));
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let _ = m.insert(addr, 5);
        assert_eq!(m.len(), 1);
        assert_eq!(m.touch(addr), Some(5));
    }

    #[test]
    fn evict_with_zero_timeout() {
        let mut m = UdpSessionMap::new(Duration::from_secs(0));
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let _ = m.insert(addr, 5);
        assert_eq!(m.len(), 1);
        let removed = m.evict_expired();
        assert_eq!(removed, 1);
        assert_eq!(m.len(), 0);
    }
}
