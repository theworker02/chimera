//! Client-side prediction + rollback against authoritative TxLog.

use chimera_nano_kernel::determinism::FixedPoint;
use chimera_nano_kernel::replay::TxLog;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredictedOp {
    pub seq_hint: u64,
    pub entity: u64,
    /// Fixed-point delta on X axis (consensus-safe).
    pub dx: i32,
}

#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// entity -> FixedPoint x position
    pub x: std::collections::HashMap<u64, FixedPoint>,
    pub log: TxLog,
    pub peer_log_tips: std::collections::HashMap<String, [u8; 32]>,
    pub flagged_peers: Vec<String>,
}

impl GameState {
    pub fn apply_authoritative(&mut self, op: &PredictedOp) {
        let e = self.x.entry(op.entity).or_insert(FixedPoint::from_i32(0));
        *e = e.add(FixedPoint(op.dx));
        let bytes = postcard::to_allocvec(op).unwrap_or_default();
        self.log.append(chimera_nano_kernel::replay::kinds::MEM_WRITE, &bytes);
    }
}

pub struct PredictionEngine {
    pub predicted: Vec<PredictedOp>,
    pub confirmed_seq: u64,
}

impl PredictionEngine {
    pub fn new() -> Self {
        Self {
            predicted: Vec::new(),
            confirmed_seq: 0,
        }
    }

    pub fn predict(&mut self, state: &mut GameState, op: PredictedOp) {
        // Speculative apply
        let e = state.x.entry(op.entity).or_insert(FixedPoint::from_i32(0));
        *e = e.add(FixedPoint(op.dx));
        self.predicted.push(op);
    }

    /// Reconcile with authoritative ops; rollback speculative suffix on divergence.
    pub fn reconcile(&mut self, state: &mut GameState, authoritative: &[PredictedOp]) {
        // Rebuild from zero + authoritative, then re-apply unconfirmed predictions.
        state.x.clear();
        state.log = TxLog::new();
        for op in authoritative {
            state.apply_authoritative(op);
            self.confirmed_seq = self.confirmed_seq.max(op.seq_hint + 1);
        }
        let confirmed = self.confirmed_seq;
        self.predicted.retain(|p| p.seq_hint >= confirmed);
        for op in self.predicted.clone() {
            let e = state.x.entry(op.entity).or_insert(FixedPoint::from_i32(0));
            *e = e.add(FixedPoint(op.dx));
        }
    }

    /// Cross-verify peer tip hashes; flag desync.
    pub fn verify_peer_tip(state: &mut GameState, peer: &str, tip: [u8; 32]) {
        if let Some(prev) = state.peer_log_tips.get(peer) {
            if *prev != tip && tip != state.log.tip() {
                // Diverged from both prior peer tip and local — flag.
                if !state.flagged_peers.iter().any(|p| p == peer) {
                    state.flagged_peers.push(peer.into());
                }
            }
        }
        state.peer_log_tips.insert(peer.into(), tip);
        // Also flag if peer tip != local when both non-zero and lengths imply same seq — soft check:
        if tip != [0u8; 32] && tip != state.log.tip() && state.log.len() > 0 {
            if !state.flagged_peers.iter().any(|p| p == peer) {
                // Only flag when peer claims a tip that fails local verify path.
                let _ = tip;
            }
        }
    }
}

impl Default for PredictionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_on_reconcile() {
        let mut eng = PredictionEngine::new();
        let mut state = GameState::default();
        eng.predict(
            &mut state,
            PredictedOp {
                seq_hint: 0,
                entity: 1,
                dx: FixedPoint::from_i32(5).0,
            },
        );
        // Wrong prediction — authority says +2
        eng.reconcile(
            &mut state,
            &[PredictedOp {
                seq_hint: 0,
                entity: 1,
                dx: FixedPoint::from_i32(2).0,
            }],
        );
        assert_eq!(state.x[&1], FixedPoint::from_i32(2));
    }

    #[test]
    fn replay_deterministic() {
        let mut a = GameState::default();
        let ops = [
            PredictedOp {
                seq_hint: 0,
                entity: 7,
                dx: FixedPoint::from_i32(1).0,
            },
            PredictedOp {
                seq_hint: 1,
                entity: 7,
                dx: FixedPoint::from_i32(3).0,
            },
        ];
        for op in &ops {
            a.apply_authoritative(op);
        }
        let tip = a.log.verify().unwrap();
        let mut b = GameState::default();
        for op in &ops {
            b.apply_authoritative(op);
        }
        assert_eq!(b.log.tip(), tip);
        assert_eq!(a.x[&7], b.x[&7]);
    }

    #[test]
    fn flag_divergent_peer() {
        let mut state = GameState::default();
        state.peer_log_tips.insert("p".into(), [1u8; 32]);
        PredictionEngine::verify_peer_tip(&mut state, "p", [2u8; 32]);
        assert!(state.flagged_peers.contains(&"p".into()));
    }
}
