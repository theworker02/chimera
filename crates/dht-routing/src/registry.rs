//! Function / service registry — DHT-style name → hosting peers (userspace routing).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use transport_quic::protocol::NodeId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub peer: NodeId,
    pub function: String,
    pub tenant: String,
    pub latency_ms: f32,
    pub headroom: f32,
    #[serde(skip, default = "Instant::now")]
    pub last_beat: Instant,
}

#[derive(Clone, Default)]
pub struct ServiceRegistry {
    inner: std::sync::Arc<RwLock<HashMap<String, Vec<ServiceInstance>>>>,
    ttl: Duration,
}

impl ServiceRegistry {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: std::sync::Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    pub fn register(&self, inst: ServiceInstance) {
        let key = format!("{}/{}", inst.tenant, inst.function);
        let mut g = self.inner.write();
        let list = g.entry(key).or_default();
        if let Some(existing) = list.iter_mut().find(|i| i.peer == inst.peer) {
            *existing = inst;
        } else {
            list.push(inst);
        }
    }

    pub fn heartbeat(&self, tenant: &str, function: &str, peer: NodeId, latency_ms: f32, headroom: f32) {
        let key = format!("{tenant}/{function}");
        let mut g = self.inner.write();
        if let Some(list) = g.get_mut(&key) {
            if let Some(i) = list.iter_mut().find(|i| i.peer == peer) {
                i.last_beat = Instant::now();
                i.latency_ms = latency_ms;
                i.headroom = headroom;
            }
        }
    }

    pub fn expire(&self) {
        let ttl = self.ttl;
        let mut g = self.inner.write();
        for list in g.values_mut() {
            list.retain(|i| i.last_beat.elapsed() < ttl);
        }
        g.retain(|_, v| !v.is_empty());
    }

    /// Lowest latency × inverse headroom score (userspace "anycast").
    pub fn route(&self, tenant: &str, function: &str) -> Option<ServiceInstance> {
        self.expire();
        let key = format!("{tenant}/{function}");
        let g = self.inner.read();
        let list = g.get(&key)?;
        list.iter()
            .min_by(|a, b| {
                let sa = a.latency_ms * (1.1 - a.headroom.clamp(0.0, 1.0));
                let sb = b.latency_ms * (1.1 - b.headroom.clamp(0.0, 1.0));
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    pub fn route_failover(&self, tenant: &str, function: &str, skip: &[NodeId]) -> Option<ServiceInstance> {
        self.expire();
        let key = format!("{tenant}/{function}");
        let g = self.inner.read();
        let list = g.get(&key)?;
        let mut ranked: Vec<_> = list
            .iter()
            .filter(|i| !skip.contains(&i.peer))
            .cloned()
            .collect();
        ranked.sort_by(|a, b| {
            let sa = a.latency_ms * (1.1 - a.headroom.clamp(0.0, 1.0));
            let sb = b.latency_ms * (1.1 - b.headroom.clamp(0.0, 1.0));
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.into_iter().next()
    }

    pub fn list(&self, tenant: &str) -> Vec<ServiceInstance> {
        self.expire();
        let g = self.inner.read();
        g.iter()
            .filter(|(k, _)| k.starts_with(&format!("{tenant}/")))
            .flat_map(|(_, v)| v.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn routes_to_lowest_latency() {
        let reg = ServiceRegistry::new(Duration::from_secs(30));
        let a = NodeId(Uuid::new_v4());
        let b = NodeId(Uuid::new_v4());
        reg.register(ServiceInstance {
            peer: a,
            function: "echo".into(),
            tenant: "t".into(),
            latency_ms: 20.0,
            headroom: 0.9,
            last_beat: Instant::now(),
        });
        reg.register(ServiceInstance {
            peer: b,
            function: "echo".into(),
            tenant: "t".into(),
            latency_ms: 5.0,
            headroom: 0.5,
            last_beat: Instant::now(),
        });
        let pick = reg.route("t", "echo").unwrap();
        assert_eq!(pick.peer, b);
    }

    #[test]
    fn failover_skips_peer() {
        let reg = ServiceRegistry::new(Duration::from_secs(30));
        let a = NodeId(Uuid::new_v4());
        let b = NodeId(Uuid::new_v4());
        reg.register(ServiceInstance {
            peer: a,
            function: "f".into(),
            tenant: "t".into(),
            latency_ms: 1.0,
            headroom: 1.0,
            last_beat: Instant::now(),
        });
        reg.register(ServiceInstance {
            peer: b,
            function: "f".into(),
            tenant: "t".into(),
            latency_ms: 10.0,
            headroom: 1.0,
            last_beat: Instant::now(),
        });
        let pick = reg.route_failover("t", "f", &[a]).unwrap();
        assert_eq!(pick.peer, b);
    }
}
