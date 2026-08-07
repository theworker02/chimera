//! Intent-based compilation into Wasm task plans.

use uuid::Uuid;

use crate::protocol::{IntentSpec, JobId, TaskId, TaskSlice, TaskState};

pub struct IntentCompiler {
    wasm_hash: [u8; 32],
}

impl IntentCompiler {
    pub fn new(wasm_hash: [u8; 32]) -> Self {
        Self { wasm_hash }
    }

    pub fn parse(declaration: &str) -> IntentSpec {
        let mut intent = IntentSpec {
            id: Uuid::new_v4(),
            name: "intent".into(),
            declaration: declaration.into(),
            latency_budget_ms: None,
            privacy_local_only: false,
            prefer_gpu: false,
            max_peers: 8,
            slice_hint: 4,
            elements_per_slice: 4096,
        };
        for token in declaration.split_whitespace() {
            if let Some(v) = token.strip_prefix("latency<") {
                let v = v.trim_end_matches("ms");
                intent.latency_budget_ms = v.parse().ok();
            } else if token == "privacy=local" {
                intent.privacy_local_only = true;
            } else if token.starts_with("render=") {
                intent.prefer_gpu = true;
                if token.contains("hd") || token.contains("4k") {
                    intent.elements_per_slice = 8192;
                }
            } else if let Some(v) = token.strip_prefix("slices=") {
                intent.slice_hint = v.parse().unwrap_or(4);
            } else if let Some(v) = token.strip_prefix("peers=") {
                intent.max_peers = v.parse().unwrap_or(8);
            } else if let Some(v) = token.strip_prefix("name=") {
                intent.name = v.into();
            }
        }
        intent
    }

    pub fn compile(&self, intent: &IntentSpec) -> IntentPlan {
        let mut slices = intent.slice_hint.max(1);
        if let Some(budget) = intent.latency_budget_ms {
            if budget < 100 {
                slices = slices.min(2);
            } else if budget > 1000 {
                slices = slices.max(8);
            }
        }
        if intent.privacy_local_only {
            slices = slices.min(4);
        }
        let job = JobId::new();
        let mut tasks = Vec::with_capacity(slices as usize);
        for i in 0..slices {
            tasks.push(TaskSlice {
                id: TaskId::new(),
                job_id: job,
                index: i,
                total: slices,
                seed: intent.id.as_u128() as u64 ^ (i as u64),
                element_count: intent.elements_per_slice,
                wasm_hash: self.wasm_hash,
                data_deps: vec![],
                state: TaskState::Pending,
                assigned_to: None,
                checkpoint_offset: 0,
                fuel_used: 0,
                intent_id: Some(intent.id),
            });
        }
        IntentPlan {
            intent: intent.clone(),
            job_id: job,
            tasks,
            mem_pages: (intent.elements_per_slice as u64 / 1024).max(4),
            prefetch_blocks: vec![],
            local_only: intent.privacy_local_only,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntentPlan {
    pub intent: IntentSpec,
    pub job_id: JobId,
    pub tasks: Vec<TaskSlice>,
    pub mem_pages: u64,
    pub prefetch_blocks: Vec<[u8; 32]>,
    pub local_only: bool,
}
