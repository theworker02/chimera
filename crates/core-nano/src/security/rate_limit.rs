//! Anti-DDoS / malicious peer basics: rate limits + scoring.

use hashbrown::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct PeerScore {
    pub successes: u32,
    pub failures: u32,
    pub strikes: u32,
}

impl Default for PeerScore {
    fn default() -> Self {
        Self {
            successes: 0,
            failures: 0,
            strikes: 0,
        }
    }
}

impl PeerScore {
    pub fn reputation(self) -> i32 {
        self.successes as i32 - (self.failures as i32 * 2) - (self.strikes as i32 * 5)
    }

    pub fn is_banned(self) -> bool {
        self.strikes >= 5 || self.reputation() < -20
    }
}

/// Token-bucket style limiter keyed by peer id bytes.
pub struct RateLimiter {
    windows: HashMap<[u8; 16], Window>,
    max_per_window: u32,
    window_ms: u64,
}

struct Window {
    count: u32,
    start_ms: u64,
}

impl RateLimiter {
    pub fn new(max_per_window: u32, window_ms: u64) -> Self {
        Self {
            windows: HashMap::new(),
            max_per_window: max_per_window.max(1),
            window_ms: window_ms.max(1),
        }
    }

    pub fn allow(&mut self, peer: [u8; 16], now_ms: u64) -> bool {
        let w = self.windows.entry(peer).or_insert(Window {
            count: 0,
            start_ms: now_ms,
        });
        if now_ms.saturating_sub(w.start_ms) >= self.window_ms {
            w.count = 0;
            w.start_ms = now_ms;
        }
        if w.count >= self.max_per_window {
            return false;
        }
        w.count += 1;
        true
    }
}

#[derive(Default)]
pub struct PeerBook {
    scores: HashMap<[u8; 16], PeerScore>,
}

impl PeerBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn score_mut(&mut self, peer: [u8; 16]) -> &mut PeerScore {
        self.scores.entry(peer).or_default()
    }

    pub fn record_success(&mut self, peer: [u8; 16]) {
        self.score_mut(peer).successes = self.score_mut(peer).successes.saturating_add(1);
    }

    pub fn record_failure(&mut self, peer: [u8; 16]) {
        let s = self.score_mut(peer);
        s.failures = s.failures.saturating_add(1);
        if s.failures % 3 == 0 {
            s.strikes = s.strikes.saturating_add(1);
        }
    }

    pub fn is_banned(&self, peer: [u8; 16]) -> bool {
        self.scores.get(&peer).map(|s| s.is_banned()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_trips() {
        let mut rl = RateLimiter::new(3, 1000);
        let p = [1u8; 16];
        assert!(rl.allow(p, 0));
        assert!(rl.allow(p, 1));
        assert!(rl.allow(p, 2));
        assert!(!rl.allow(p, 3));
        assert!(rl.allow(p, 1001));
    }
}
