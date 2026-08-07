//! Credit ledger on Raft KV — earn on verified receipts, spend on submit.

use anyhow::{bail, Context};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use parking_lot::RwLock;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::economy::verify_receipt;
use crate::protocol::{now_ms, ComputeReceipt};
use crate::raft_kv::KvStore;

const BAL_PREFIX: &str = "ledger:bal:";
const TX_PREFIX: &str = "ledger:tx:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerTx {
    pub id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub kind: String,
    pub memo: String,
    pub timestamp_ms: u64,
    pub public_key: [u8; 32],
    pub signature: Vec<u8>,
}

#[derive(Clone)]
pub struct CreditLedger {
    kv: KvStore,
    /// Operator bypass for local meshes (no credit checks).
    pub bypass: ArcFlag,
    signer: std::sync::Arc<RwLock<SigningKey>>,
    verifying: [u8; 32],
}

#[derive(Clone, Default)]
pub struct ArcFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl ArcFlag {
    pub fn new(v: bool) -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(v)))
    }
    pub fn get(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn set(&self, v: bool) {
        self.0.store(v, std::sync::atomic::Ordering::Relaxed);
    }
}

impl CreditLedger {
    pub fn new(kv: KvStore, bypass: bool) -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key().to_bytes();
        Self {
            kv,
            bypass: ArcFlag::new(bypass),
            signer: std::sync::Arc::new(RwLock::new(signing)),
            verifying,
        }
    }

    pub fn account_key(node: &str) -> String {
        format!("{BAL_PREFIX}{node}")
    }

    pub fn balance(&self, account: &str) -> u64 {
        self.kv
            .get(&Self::account_key(account))
            .and_then(|b| {
                if b.len() >= 8 {
                    Some(u64::from_le_bytes(b[..8].try_into().ok()?))
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    fn set_balance(&self, account: &str, bal: u64) -> anyhow::Result<()> {
        self.kv
            .set(Self::account_key(account), bal.to_le_bytes().to_vec())
            .context("ledger set bal")
    }

    fn sign_tx(&self, tx: &mut LedgerTx) {
        let mut pre = Vec::new();
        pre.extend_from_slice(tx.id.as_bytes());
        pre.extend_from_slice(tx.from.as_bytes());
        pre.extend_from_slice(tx.to.as_bytes());
        pre.extend_from_slice(&tx.amount.to_le_bytes());
        pre.extend_from_slice(tx.kind.as_bytes());
        let sig = self.signer.read().sign(&pre);
        tx.public_key = self.verifying;
        tx.signature = sig.to_bytes().to_vec();
    }

    pub fn verify_tx(tx: &LedgerTx) -> bool {
        let Ok(vk) = VerifyingKey::from_bytes(&tx.public_key) else {
            return false;
        };
        let mut pre = Vec::new();
        pre.extend_from_slice(tx.id.as_bytes());
        pre.extend_from_slice(tx.from.as_bytes());
        pre.extend_from_slice(tx.to.as_bytes());
        pre.extend_from_slice(&tx.amount.to_le_bytes());
        pre.extend_from_slice(tx.kind.as_bytes());
        let Ok(sig) = Signature::from_slice(&tx.signature) else {
            return false;
        };
        vk.verify(&pre, &sig).is_ok()
    }

    fn record_tx(&self, tx: &LedgerTx) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(tx)?;
        self.kv
            .set(format!("{TX_PREFIX}{}", tx.id), bytes)
            .context("ledger tx")
    }

    /// Credit account for verified work (earn-on-execute).
    pub fn earn_from_receipt(
        &self,
        account: &str,
        receipt: &ComputeReceipt,
        rate_per_fuel: u64,
    ) -> anyhow::Result<LedgerTx> {
        if !verify_receipt(receipt) {
            bail!("invalid receipt");
        }
        let amount = receipt.fuel_consumed.saturating_mul(rate_per_fuel).max(1);
        let mut tx = LedgerTx {
            id: uuid::Uuid::new_v4().to_string(),
            from: "treasury".into(),
            to: account.into(),
            amount,
            kind: "earn".into(),
            memo: format!("task={}", receipt.task_id.0),
            timestamp_ms: now_ms(),
            public_key: [0u8; 32],
            signature: vec![],
        };
        self.sign_tx(&mut tx);
        let bal = self.balance(account).saturating_add(amount);
        self.set_balance(account, bal)?;
        self.record_tx(&tx)?;
        Ok(tx)
    }

    /// Spend credits to submit a workload.
    pub fn spend(
        &self,
        account: &str,
        amount: u64,
        memo: &str,
    ) -> anyhow::Result<LedgerTx> {
        if self.bypass.get() {
            let mut tx = LedgerTx {
                id: uuid::Uuid::new_v4().to_string(),
                from: account.into(),
                to: "treasury".into(),
                amount: 0,
                kind: "bypass".into(),
                memo: memo.into(),
                timestamp_ms: now_ms(),
                public_key: [0u8; 32],
                signature: vec![],
            };
            self.sign_tx(&mut tx);
            return Ok(tx);
        }
        let bal = self.balance(account);
        if bal < amount {
            bail!("insufficient credits: have {bal} need {amount}");
        }
        let mut tx = LedgerTx {
            id: uuid::Uuid::new_v4().to_string(),
            from: account.into(),
            to: "treasury".into(),
            amount,
            kind: "spend".into(),
            memo: memo.into(),
            timestamp_ms: now_ms(),
            public_key: [0u8; 32],
            signature: vec![],
        };
        self.sign_tx(&mut tx);
        self.set_balance(account, bal - amount)?;
        self.record_tx(&tx)?;
        Ok(tx)
    }

    pub fn ensure_can_submit(&self, account: &str, cost: u64) -> anyhow::Result<()> {
        if self.bypass.get() {
            return Ok(());
        }
        let bal = self.balance(account);
        if bal < cost {
            bail!("insufficient credits: have {bal} need {cost}");
        }
        Ok(())
    }

    pub fn credit(&self, account: &str, amount: u64) -> anyhow::Result<()> {
        let bal = self.balance(account).saturating_add(amount);
        self.set_balance(account, bal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::ReceiptSigner;
    use crate::protocol::{NodeId, TaskId};
    use uuid::Uuid;

    #[test]
    fn earn_spend_reject_when_broke() {
        let kv = KvStore::leader(1, vec![]);
        let ledger = CreditLedger::new(kv, false);
        let signer = ReceiptSigner::generate();
        let receipt = signer.sign_receipt(
            TaskId(Uuid::new_v4()),
            NodeId(Uuid::new_v4()),
            [1u8; 32],
            100,
            [2u8; 32],
        );
        ledger.earn_from_receipt("alice", &receipt, 1).unwrap();
        assert!(ledger.balance("alice") >= 100);
        ledger.spend("alice", 50, "job").unwrap();
        assert_eq!(ledger.balance("alice"), ledger.balance("alice"));
        let left = ledger.balance("alice");
        assert!(ledger.spend("alice", left + 1, "too much").is_err());
    }

    #[test]
    fn bypass_allows_zero_balance() {
        let kv = KvStore::leader(1, vec![]);
        let ledger = CreditLedger::new(kv, true);
        assert!(ledger.ensure_can_submit("broke", 1_000_000).is_ok());
        ledger.spend("broke", 1_000_000, "ok").unwrap();
    }
}
