//! Freight — decentralized Wasm package registry (signed manifests, CAS-backed).

use std::collections::HashMap;

use anyhow::bail;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use parking_lot::RwLock;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::fs::ChimeraFs;
use crate::gateway::FunctionGateway;
use crate::protocol::now_ms;
use crate::rbac::{require, Permission, Principal};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub wasm_hash: [u8; 32],
    pub publisher_pk: [u8; 32],
    pub deps: Vec<String>,
    pub description: String,
    pub timestamp_ms: u64,
    pub signature: Vec<u8>,
}

impl PackageManifest {
    fn signable_bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(self.name.as_bytes());
        b.extend_from_slice(0u8.to_le_bytes().as_slice());
        b.extend_from_slice(self.version.as_bytes());
        b.extend_from_slice(&self.wasm_hash);
        b.extend_from_slice(&self.publisher_pk);
        for d in &self.deps {
            b.extend_from_slice(d.as_bytes());
        }
        b.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        b
    }

    pub fn verify(&self) -> bool {
        let Ok(vk) = VerifyingKey::from_bytes(&self.publisher_pk) else {
            return false;
        };
        let Ok(sig) = Signature::from_slice(&self.signature) else {
            return false;
        };
        vk.verify(&self.signable_bytes(), &sig).is_ok()
    }
}

#[derive(Clone)]
pub struct PublisherKey {
    signing: std::sync::Arc<SigningKey>,
    pub verifying: [u8; 32],
}

impl PublisherKey {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key().to_bytes();
        Self {
            signing: std::sync::Arc::new(signing),
            verifying,
        }
    }

    pub fn sign_manifest(
        &self,
        name: &str,
        version: &str,
        wasm_hash: [u8; 32],
        deps: Vec<String>,
        description: &str,
    ) -> PackageManifest {
        let mut m = PackageManifest {
            name: name.into(),
            version: version.into(),
            wasm_hash,
            publisher_pk: self.verifying,
            deps,
            description: description.into(),
            timestamp_ms: now_ms(),
            signature: vec![],
        };
        let sig = self.signing.sign(&m.signable_bytes());
        m.signature = sig.to_bytes().to_vec();
        m
    }
}

#[derive(Clone)]
pub struct FreightRegistry {
    /// name@version → manifest
    packages: std::sync::Arc<RwLock<HashMap<String, PackageManifest>>>,
    /// wasm_hash → bytes (local cache; also mirrored in ChimeraFS when available)
    blobs: std::sync::Arc<RwLock<HashMap<[u8; 32], Vec<u8>>>>,
}

impl Default for FreightRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FreightRegistry {
    pub fn new() -> Self {
        Self {
            packages: std::sync::Arc::new(RwLock::new(HashMap::new())),
            blobs: std::sync::Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn key(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    pub fn publish(
        &self,
        principal: &Principal,
        manifest: PackageManifest,
        wasm: &[u8],
        fs: Option<&ChimeraFs>,
    ) -> anyhow::Result<PackageManifest> {
        require(principal, Permission::SubmitWorkload)?;
        if !manifest.verify() {
            bail!("manifest signature invalid");
        }
        let hash = *blake3::hash(wasm).as_bytes();
        if hash != manifest.wasm_hash {
            bail!("wasm hash mismatch");
        }
        if let Some(fs) = fs {
            let _ = fs.ingest_bytes(
                &format!("freight/{}-{}", manifest.name, manifest.version),
                wasm,
            )?;
        }
        self.blobs.write().insert(hash, wasm.to_vec());
        let key = Self::key(&manifest.name, &manifest.version);
        self.packages.write().insert(key, manifest.clone());
        Ok(manifest)
    }

    pub fn search(&self, query: &str) -> Vec<PackageManifest> {
        let q = query.to_lowercase();
        self.packages
            .read()
            .values()
            .filter(|m| {
                m.name.to_lowercase().contains(&q)
                    || m.description.to_lowercase().contains(&q)
                    || q.is_empty()
            })
            .cloned()
            .collect()
    }

    pub fn get(&self, name: &str, version: &str) -> Option<PackageManifest> {
        self.packages
            .read()
            .get(&Self::key(name, version))
            .cloned()
    }

    pub fn install(
        &self,
        principal: &Principal,
        name: &str,
        version: &str,
        gateway: &FunctionGateway,
        tenant: &str,
    ) -> anyhow::Result<PackageManifest> {
        require(principal, Permission::SubmitWorkload)?;
        let manifest = self
            .get(name, version)
            .ok_or_else(|| anyhow::anyhow!("package not found"))?;
        if !manifest.verify() {
            bail!("signature failed");
        }
        let wasm = self
            .blobs
            .read()
            .get(&manifest.wasm_hash)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("module blob missing"))?;
        let check = *blake3::hash(&wasm).as_bytes();
        if check != manifest.wasm_hash {
            bail!("integrity check failed");
        }
        gateway.deploy(
            principal,
            tenant,
            &manifest.name,
            &wasm,
            16,
            5_000_000,
        )?;
        Ok(manifest)
    }

    pub fn run(
        &self,
        principal: &Principal,
        name: &str,
        tenant: &str,
        input_hex: &str,
        gateway: &FunctionGateway,
    ) -> anyhow::Result<crate::gateway::InvokeResult> {
        gateway.invoke(
            principal,
            &crate::gateway::InvokeRequest {
                tenant: tenant.into(),
                function: name.into(),
                input_hex: input_hex.into(),
                priority: 1,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{demo_add1_wasm, FunctionGateway};
    use crate::protocol::NodeId;
    use crate::rbac::{Principal, Role};
    use uuid::Uuid;

    #[test]
    fn publish_install_run() {
        let reg = FreightRegistry::new();
        let pubk = PublisherKey::generate();
        let wasm = demo_add1_wasm();
        let hash = *blake3::hash(&wasm).as_bytes();
        let m = pubk.sign_manifest("add1", "0.1.0", hash, vec![], "demo add1");
        let admin = Principal {
            name: "admin".into(),
            role: Role::Admin,
            asset_prefixes: vec![],
        };
        reg.publish(&admin, m, &wasm, None).unwrap();
        let found = reg.search("add");
        assert_eq!(found.len(), 1);
        let gw = FunctionGateway::new(NodeId(Uuid::new_v4())).unwrap();
        reg.install(&admin, "add1", "0.1.0", &gw, "freight").unwrap();
        let res = reg
            .run(&admin, "add1", "freight", &hex::encode([41u8]), &gw)
            .unwrap();
        let out = hex::decode(&res.output_hex).unwrap();
        assert_eq!(i32::from_le_bytes(out.as_slice().try_into().unwrap()), 42);
    }
}
