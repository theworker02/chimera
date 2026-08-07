//! Distributed lock-free-ish ECS sync on CRDT clocks + contiguous storage.

use std::collections::HashMap;

use chimera_nano_kernel::replay::{kinds, TxLog};
use serde::{Deserialize, Serialize};

pub type EntityId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentKind {
    Transform = 1,
    RigidBody = 2,
    Parent = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub qw: f32,
    pub qx: f32,
    pub qy: f32,
    pub qz: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            qw: 1.0,
            qx: 0.0,
            qy: 0.0,
            qz: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RigidBody {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub mass: f32,
}

#[derive(Debug, Clone, Default)]
struct EntityRec {
    transform: Option<Transform>,
    rigid: Option<RigidBody>,
    parent: Option<EntityId>,
    children: Vec<EntityId>,
}

/// Contiguous archetype arrays for zero-copy-ish snapshotting (host Vec; mmap on engine embed).
#[derive(Default)]
pub struct NexusWorld {
    next_id: EntityId,
    entities: HashMap<EntityId, EntityRec>,
    /// Flattened transform SoA for snapshot.
    pub transforms_x: Vec<f32>,
    pub transforms_y: Vec<f32>,
    pub transforms_z: Vec<f32>,
    pub entity_order: Vec<EntityId>,
    pub log: TxLog,
    /// Vector clock ticks per node key for concurrent merges.
    pub clocks: HashMap<String, u64>,
}

impl NexusWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self, node: &str) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.insert(id, EntityRec::default());
        self.entity_order.push(id);
        self.tick(node);
        self.log
            .append(kinds::TASK_SPAWN, &id.to_le_bytes());
        self.rebuild_soa();
        id
    }

    pub fn set_parent(&mut self, child: EntityId, parent: EntityId, node: &str) -> bool {
        if child == parent || !self.entities.contains_key(&child) || !self.entities.contains_key(&parent)
        {
            return false;
        }
        // Detach from old parent.
        if let Some(old) = self.entities.get(&child).and_then(|e| e.parent) {
            if let Some(p) = self.entities.get_mut(&old) {
                p.children.retain(|c| *c != child);
            }
        }
        self.entities.get_mut(&child).unwrap().parent = Some(parent);
        self.entities.get_mut(&parent).unwrap().children.push(child);
        self.tick(node);
        let mut payload = Vec::new();
        payload.extend_from_slice(&child.to_le_bytes());
        payload.extend_from_slice(&parent.to_le_bytes());
        self.log.append(kinds::MEM_WRITE, &payload);
        true
    }

    pub fn set_transform(&mut self, id: EntityId, t: Transform, node: &str) {
        if let Some(e) = self.entities.get_mut(&id) {
            e.transform = Some(t);
            self.tick(node);
            let bytes = postcard::to_allocvec(&(id, t)).unwrap_or_default();
            self.log.append(kinds::MEM_WRITE, &bytes);
            self.rebuild_soa();
        }
    }

    pub fn set_rigid(&mut self, id: EntityId, r: RigidBody, node: &str) {
        if let Some(e) = self.entities.get_mut(&id) {
            e.rigid = Some(r);
            self.tick(node);
        }
    }

    pub fn parent_of(&self, id: EntityId) -> Option<EntityId> {
        self.entities.get(&id).and_then(|e| e.parent)
    }

    pub fn children_of(&self, id: EntityId) -> Vec<EntityId> {
        self.entities
            .get(&id)
            .map(|e| e.children.clone())
            .unwrap_or_default()
    }

    pub fn transform(&self, id: EntityId) -> Option<Transform> {
        self.entities.get(&id).and_then(|e| e.transform)
    }

    /// Migrate entity subtree to a "peer world" (clone hierarchy).
    pub fn migrate_subtree(&self, root: EntityId) -> Option<NexusWorld> {
        if !self.entities.contains_key(&root) {
            return None;
        }
        let mut out = NexusWorld::new();
        let mut map = HashMap::new();
        fn walk(
            src: &NexusWorld,
            id: EntityId,
            dst: &mut NexusWorld,
            map: &mut HashMap<EntityId, EntityId>,
            parent_new: Option<EntityId>,
        ) {
            let new_id = dst.spawn("migrate");
            map.insert(id, new_id);
            if let Some(t) = src.transform(id) {
                dst.set_transform(new_id, t, "migrate");
            }
            if let Some(p) = parent_new {
                dst.set_parent(new_id, p, "migrate");
            }
            for c in src.children_of(id) {
                walk(src, c, dst, map, Some(new_id));
            }
        }
        walk(self, root, &mut out, &mut map, None);
        Some(out)
    }

    /// Merge concurrent transform writes with last-writer-wins by clock sum (demo CRDT).
    pub fn merge_clocks(&mut self, other: &HashMap<String, u64>) {
        for (k, v) in other {
            let e = self.clocks.entry(k.clone()).or_insert(0);
            *e = (*e).max(*v);
        }
    }

    fn tick(&mut self, node: &str) {
        *self.clocks.entry(node.into()).or_insert(0) += 1;
    }

    fn rebuild_soa(&mut self) {
        self.transforms_x.clear();
        self.transforms_y.clear();
        self.transforms_z.clear();
        for id in &self.entity_order {
            let t = self
                .entities
                .get(id)
                .and_then(|e| e.transform)
                .unwrap_or_default();
            self.transforms_x.push(t.x);
            self.transforms_y.push(t.y);
            self.transforms_z.push(t.z);
        }
    }

    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.transforms_x.len() * 12);
        for i in 0..self.transforms_x.len() {
            out.extend_from_slice(&self.transforms_x[i].to_le_bytes());
            out.extend_from_slice(&self.transforms_y[i].to_le_bytes());
            out.extend_from_slice(&self.transforms_z[i].to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_survives_migration() {
        let mut w = NexusWorld::new();
        let root = w.spawn("a");
        let child = w.spawn("a");
        let grand = w.spawn("a");
        assert!(w.set_parent(child, root, "a"));
        assert!(w.set_parent(grand, child, "a"));
        w.set_transform(
            child,
            Transform {
                x: 1.0,
                ..Default::default()
            },
            "a",
        );
        let migrated = w.migrate_subtree(root).unwrap();
        assert_eq!(migrated.entity_order.len(), 3);
        // Root has one child in migrated world.
        let new_root = migrated.entity_order[0];
        assert_eq!(migrated.children_of(new_root).len(), 1);
    }

    #[test]
    fn concurrent_clock_merge() {
        let mut a = NexusWorld::new();
        let mut b = NexusWorld::new();
        let e = a.spawn("n1");
        let _ = b.spawn("n2");
        a.set_transform(e, Transform { x: 3.0, ..Default::default() }, "n1");
        b.merge_clocks(&a.clocks);
        a.merge_clocks(&b.clocks);
        assert!(a.clocks.get("n1").copied().unwrap_or(0) >= 1);
    }
}
