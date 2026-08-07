//! Declarative resource-limit policies (JSON).
//! Status: working for JSON rules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePolicy {
    pub max_fuel: Option<u64>,
    pub max_memory_mib: Option<u64>,
    pub max_parallel: Option<u32>,
    pub deny: bool,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self { max_fuel: Some(50_000_000), max_memory_mib: Some(64), max_parallel: Some(4), deny: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequest {
    pub fuel: u64,
    pub memory_mib: u64,
    pub parallel: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
}

pub fn parse_policy_json(s: &str) -> anyhow::Result<ResourcePolicy> {
    Ok(serde_json::from_str(s)?)
}

pub fn evaluate(policy: &ResourcePolicy, req: &PolicyRequest) -> PolicyDecision {
    if policy.deny {
        return PolicyDecision::Deny { reason: "policy deny".into() };
    }
    if let Some(m) = policy.max_fuel {
        if req.fuel > m {
            return PolicyDecision::Deny { reason: format!("fuel {} > max {m}", req.fuel) };
        }
    }
    if let Some(m) = policy.max_memory_mib {
        if req.memory_mib > m {
            return PolicyDecision::Deny { reason: format!("memory {} > max {m}", req.memory_mib) };
        }
    }
    if let Some(m) = policy.max_parallel {
        if req.parallel > m {
            return PolicyDecision::Deny { reason: format!("parallel {} > max {m}", req.parallel) };
        }
    }
    PolicyDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_over_fuel() {
        let p = parse_policy_json(r#"{"max_fuel":1000,"deny":false}"#).unwrap();
        let d = evaluate(&p, &PolicyRequest { fuel: 5000, memory_mib: 1, parallel: 1 });
        assert!(matches!(d, PolicyDecision::Deny { .. }));
    }
}
