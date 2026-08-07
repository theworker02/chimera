//! CRDT-backed shared state + vector clocks for optimistic regions.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::protocol::NodeId;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VectorClock {
    pub ticks: HashMap<String, u64>,
}

impl VectorClock {
    pub fn tick(&mut self, node: NodeId) {
        let k = node.0.to_string();
        *self.ticks.entry(k).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (k, v) in &other.ticks {
            let e = self.ticks.entry(k.clone()).or_insert(0);
            *e = (*e).max(*v);
        }
    }

    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut strictly_less = false;
        let keys: HashSet<_> = self.ticks.keys().chain(other.ticks.keys()).cloned().collect();
        for k in keys {
            let a = self.ticks.get(&k).copied().unwrap_or(0);
            let b = other.ticks.get(&k).copied().unwrap_or(0);
            if a > b {
                return false;
            }
            if a < b {
                strictly_less = true;
            }
        }
        strictly_less
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GCounter {
    pub counts: HashMap<String, u64>,
}

impl GCounter {
    pub fn incr(&mut self, node: NodeId, by: u64) {
        *self.counts.entry(node.0.to_string()).or_insert(0) += by;
    }

    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    pub fn merge(&mut self, other: &GCounter) {
        for (k, v) in &other.counts {
            let e = self.counts.entry(k.clone()).or_insert(0);
            *e = (*e).max(*v);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrSet {
    pub elements: HashMap<String, HashSet<String>>,
}

impl OrSet {
    pub fn add(&mut self, value: &str, tag: String) {
        self.elements.entry(value.into()).or_default().insert(tag);
    }

    pub fn remove(&mut self, value: &str) {
        self.elements.remove(value);
    }

    pub fn contains(&self, value: &str) -> bool {
        self.elements.get(value).map(|s| !s.is_empty()).unwrap_or(false)
    }

    pub fn merge(&mut self, other: &OrSet) {
        for (k, tags) in &other.elements {
            self.elements.entry(k.clone()).or_default().extend(tags.iter().cloned());
        }
    }
}

/// Last-writer-wins register (scalar) for DSM metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LwwRegister<T: Clone + Default> {
    pub value: T,
    pub tick: u64,
    pub author: String,
}

impl<T: Clone + Default> LwwRegister<T> {
    pub fn set(&mut self, node: NodeId, tick: u64, value: T) {
        let author = node.0.to_string();
        if (tick, &author) >= (self.tick, &self.author) {
            self.value = value;
            self.tick = tick;
            self.author = author;
        }
    }

    pub fn merge(&mut self, other: &Self) {
        if (other.tick, &other.author) >= (self.tick, &self.author) {
            self.value = other.value.clone();
            self.tick = other.tick;
            self.author = other.author.clone();
        }
    }
}

/// STM-style optimistic region metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StmRegion {
    pub name: String,
    pub clock: VectorClock,
    pub counter: GCounter,
    pub flags: OrSet,
    /// When true, ownership leases required for linearizability.
    pub linearizable: bool,
}
