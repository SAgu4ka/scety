use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol { Udp, Tcp }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindAddress<A = IpAddr> {
    Wildcard,
    Specific(A),
}

impl<A: Copy + Into<IpAddr>> BindAddress<A> {
    pub fn conflicts_with(self, other: Self) -> bool {
        match (self, other) {
            (BindAddress::Wildcard, _) | (_, BindAddress::Wildcard) => true,
            (BindAddress::Specific(a), BindAddress::Specific(b)) => a.into() == b.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollisionChecker<A = IpAddr> {
    bind_addrs: HashMap<(BindAddress<A>, Protocol), BTreeSet<u16>>,
}

impl<A: Eq + Hash> Default for CollisionChecker<A> {
    fn default() -> Self {
        Self {bind_addrs: HashMap::new()}
    }
}

impl<A: Eq + Hash + Copy + Into<IpAddr>> CollisionChecker<A> {
    pub fn new() -> Self { Self::default() }

    pub fn try_add(&mut self, addr: BindAddress<A>, proto: Protocol, port: u16) -> bool {
        let key = (addr, proto);
        if let Some(ex) = self.bind_addrs.get(&key) {
            if ex.contains(&port) {
                return false;
            }
        }

        for ((ex_addr, ex_proto), ports) in &self.bind_addrs {
            if *ex_proto == proto
                && ports.contains(&port)
                && addr.conflicts_with(*ex_addr)
            {
                return false;
            }
        }

        self.bind_addrs.entry(key).or_default().insert(port);
        true
    }

    pub fn get_ports(&self) -> HashMap<BindAddress<A>, HashMap<Protocol, Vec<u16>>>
    where A: Clone {
        let mut out: HashMap<BindAddress<A>, HashMap<Protocol, Vec<u16>>> = HashMap::new();
        for ((addr, proto), ports) in &self.bind_addrs {
            out.entry(addr.clone()).or_default()
                .entry(*proto).or_default()
                .extend(ports.iter().copied());
        }
    out
    }
}