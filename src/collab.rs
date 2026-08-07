//! Collaborative sessions — LWW-Text CRDT with sub-ms local apply + async merge.

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::mem::crdt::VectorClock;
use crate::protocol::{now_ms, NodeId};

/// Last-writer-wins character tape with per-position clocks (simple shared notes CRDT).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LwwText {
    /// Sequence of (char, node, tick) — merge keeps higher (tick, node) per index via full replace ops.
    pub ops: Vec<TextOp>,
    pub clock: VectorClock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextOp {
    pub id: String,
    pub author: String,
    pub tick: u64,
    pub kind: TextOpKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "t")]
pub enum TextOpKind {
    Insert { index: usize, ch: char },
    Delete { index: usize },
    /// Full snapshot set (used for small notes panels).
    Set { text: String },
}

impl LwwText {
    pub fn apply_local(&mut self, node: NodeId, kind: TextOpKind) -> TextOp {
        self.clock.tick(node);
        let tick = self.clock.ticks.get(&node.0.to_string()).copied().unwrap_or(1);
        let op = TextOp {
            id: format!("{}:{tick}", node.0),
            author: node.0.to_string(),
            tick,
            kind,
        };
        self.apply_op(&op);
        op
    }

    pub fn apply_op(&mut self, op: &TextOp) {
        // Merge clock
        let mut vc = VectorClock::default();
        vc.ticks.insert(op.author.clone(), op.tick);
        self.clock.merge(&vc);
        match &op.kind {
            TextOpKind::Set { text: _ } => {
                // Keep only latest Set by (tick, author)
                let replace = self
                    .ops
                    .iter()
                    .filter(|o| matches!(o.kind, TextOpKind::Set { .. }))
                    .all(|o| (op.tick, &op.author) >= (o.tick, &o.author));
                if replace || !self.ops.iter().any(|o| matches!(o.kind, TextOpKind::Set { .. })) {
                    self.ops.retain(|o| !matches!(o.kind, TextOpKind::Set { .. }));
                    self.ops.push(op.clone());
                }
            }
            _ => {
                if !self.ops.iter().any(|o| o.id == op.id) {
                    self.ops.push(op.clone());
                }
            }
        }
    }

    pub fn merge(&mut self, other: &LwwText) {
        for op in &other.ops {
            self.apply_op(op);
        }
        self.clock.merge(&other.clock);
    }

    pub fn render(&self) -> String {
        // Prefer latest Set; else replay inserts/deletes in tick order
        if let Some(op) = self
            .ops
            .iter()
            .filter(|o| matches!(o.kind, TextOpKind::Set { .. }))
            .max_by_key(|o| (o.tick, o.author.clone()))
        {
            if let TextOpKind::Set { text } = &op.kind {
                return text.clone();
            }
        }
        let mut ops = self.ops.clone();
        ops.sort_by(|a, b| (a.tick, &a.author).cmp(&(b.tick, &b.author)));
        let mut chars: Vec<char> = Vec::new();
        for op in ops {
            match op.kind {
                TextOpKind::Insert { index, ch } => {
                    let i = index.min(chars.len());
                    chars.insert(i, ch);
                }
                TextOpKind::Delete { index } => {
                    if index < chars.len() {
                        chars.remove(index);
                    }
                }
                TextOpKind::Set { text } => {
                    chars = text.chars().collect();
                }
            }
        }
        chars.into_iter().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabSession {
    pub name: String,
    pub doc: LwwText,
    pub updated_ms: u64,
}

#[derive(Clone, Default)]
pub struct CollabHub {
    sessions: std::sync::Arc<RwLock<HashMap<String, CollabSession>>>,
}

impl CollabHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(&self, name: &str) -> CollabSession {
        let mut g = self.sessions.write();
        g.entry(name.into())
            .or_insert_with(|| CollabSession {
                name: name.into(),
                doc: LwwText::default(),
                updated_ms: now_ms(),
            })
            .clone()
    }

    pub fn apply(&self, name: &str, node: NodeId, kind: TextOpKind) -> CollabSession {
        let mut g = self.sessions.write();
        let s = g.entry(name.into()).or_insert_with(|| CollabSession {
            name: name.into(),
            doc: LwwText::default(),
            updated_ms: now_ms(),
        });
        s.doc.apply_local(node, kind);
        s.updated_ms = now_ms();
        s.clone()
    }

    pub fn merge_remote(&self, name: &str, remote: &LwwText) -> CollabSession {
        let mut g = self.sessions.write();
        let s = g.entry(name.into()).or_insert_with(|| CollabSession {
            name: name.into(),
            doc: LwwText::default(),
            updated_ms: now_ms(),
        });
        s.doc.merge(remote);
        s.updated_ms = now_ms();
        s.clone()
    }

    pub fn list(&self) -> Vec<String> {
        self.sessions.read().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn concurrent_edits_converge() {
        let a = NodeId(Uuid::new_v4());
        let b = NodeId(Uuid::new_v4());
        let mut doc_a = LwwText::default();
        let mut doc_b = LwwText::default();
        let op_a = doc_a.apply_local(a, TextOpKind::Set { text: "hello".into() });
        let op_b = doc_b.apply_local(b, TextOpKind::Set { text: "world".into() });
        doc_a.apply_op(&op_b);
        doc_b.apply_op(&op_a);
        // Same merge inputs → same render (LWW by tick/author)
        assert_eq!(doc_a.render(), doc_b.render());
        let hub = CollabHub::new();
        hub.apply("notes", a, TextOpKind::Set { text: "mesh".into() });
        hub.apply("notes", b, TextOpKind::Set { text: "shell".into() });
        let s = hub.get_or_create("notes");
        assert!(!s.doc.render().is_empty());
    }
}
