use std::collections::HashMap;

pub struct TimerWheel {
    slots: Vec<Vec<usize>>,
    idx_map: HashMap<usize, usize>,
    cur: usize,
    size: usize,
}

impl TimerWheel {
    pub fn new(size: usize) -> Self {
        Self {
            slots: vec![Vec::new(); size],
            idx_map: HashMap::new(),
            cur: 0,
            size,
        }
    }

    /// Schedule a key to expire `ticks` from now. Returns true on success.
    pub fn schedule(&mut self, key: usize, ticks: usize) -> bool {
        if ticks == 0 {
            return false;
        }
        let slot = (self.cur + (ticks % self.size)) % self.size;
        self.slots[slot].push(key);
        self.idx_map.insert(key, slot);
        true
    }

    /// Remove a key from wheel if present.
    pub fn remove(&mut self, key: usize) {
        if let Some(slot) = self.idx_map.remove(&key)
            && let Some(pos) = self.slots[slot].iter().position(|&k| k == key)
        {
            self.slots[slot].swap_remove(pos);
        }
    }

    /// Advance wheel by one tick, return expired keys.
    pub fn tick(&mut self) -> Vec<usize> {
        let next = (self.cur + 1) % self.size;
        let mut expired = Vec::new();
        std::mem::swap(&mut expired, &mut self.slots[next]);
        // advance current pointer after extracting expired
        self.cur = next;
        for k in &expired {
            self.idx_map.remove(k);
        }
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_and_tick_wraps() {
        let mut w = TimerWheel::new(4);
        assert!(w.schedule(1, 1));
        assert!(w.schedule(2, 3));

        // advance one tick -> should expire key 1
        let e1 = w.tick();
        assert_eq!(e1.len(), 1);
        assert!(e1.contains(&1));

        // advance 2 more ticks to wrap and expire key 2
        w.tick();
        let e2 = w.tick();
        assert_eq!(e2.len(), 1);
        assert!(e2.contains(&2));
    }

    #[test]
    fn remove_key() {
        let mut w = TimerWheel::new(4);
        assert!(w.schedule(5, 2));
        w.remove(5);
        // tick several times and ensure 5 never appears
        for _ in 0..4 {
            let e = w.tick();
            assert!(!e.contains(&5));
        }
    }
}
