use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};

static CONNECTIONS: OnceLock<Mutex<HashMap<IpAddr, usize>>> = OnceLock::new();

fn connections() -> &'static Mutex<HashMap<IpAddr, usize>> {
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct ConnectionGuard {
    ip: IpAddr,
    tracked: bool,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if !self.tracked {
            return;
        }
        let mut map = connections().lock().unwrap();
        if let Some(count) = map.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                map.remove(&self.ip);
            }
        }
    }
}

pub fn try_acquire(ip: IpAddr, limit: i32) -> Result<ConnectionGuard, ()> {
    if limit < 0 {
        return Ok(ConnectionGuard { ip, tracked: false });
    }

    let limit = limit as usize;
    let mut map = connections().lock().unwrap();
    let count = map.entry(ip).or_insert(0);

    if *count >= limit {
        return Err(());
    }

    *count += 1;
    Ok(ConnectionGuard { ip, tracked: true })
}
