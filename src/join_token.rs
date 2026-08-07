//! Signed, expiring mesh join tokens.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::protocol::now_ms;
use crate::rbac::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinTokenClaims {
    pub token_id: String,
    pub mesh_id: String,
    pub role: Role,
    pub issued_ms: u64,
    pub expires_ms: u64,
    pub node_name_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinToken {
    pub claims: JoinTokenClaims,
    pub signature: Vec<u8>,
    pub public_key: [u8; 32],
}

#[derive(Clone)]
pub struct TokenIssuer {
    signing: SigningKey,
    verifying: VerifyingKey,
    mesh_id: String,
}

impl TokenIssuer {
    pub fn new(mesh_id: impl Into<String>) -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        Self {
            signing,
            verifying,
            mesh_id: mesh_id.into(),
        }
    }

    pub fn from_seed(mesh_id: impl Into<String>, seed: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        Self {
            signing,
            verifying,
            mesh_id: mesh_id.into(),
        }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.verifying.to_bytes()
    }

    pub fn issue(&self, role: Role, ttl_secs: u64, node_hint: Option<String>) -> JoinToken {
        let now = now_ms();
        let claims = JoinTokenClaims {
            token_id: uuid::Uuid::new_v4().to_string(),
            mesh_id: self.mesh_id.clone(),
            role,
            issued_ms: now,
            expires_ms: now + ttl_secs * 1000,
            node_name_hint: node_hint,
        };
        let body = serde_json::to_vec(&claims).unwrap_or_default();
        let sig = self.signing.sign(&body);
        JoinToken {
            claims,
            signature: sig.to_bytes().to_vec(),
            public_key: self.verifying.to_bytes(),
        }
    }

    pub fn encode(&self, token: &JoinToken) -> String {
        let bytes = serde_json::to_vec(token).unwrap_or_default();
        hex::encode(bytes)
    }

    pub fn decode(raw: &str) -> anyhow::Result<JoinToken> {
        let bytes = hex::decode(raw.trim())?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn verify(token: &JoinToken) -> anyhow::Result<()> {
        if now_ms() > token.claims.expires_ms {
            anyhow::bail!("join token expired");
        }
        let body = serde_json::to_vec(&token.claims)?;
        let vk = VerifyingKey::from_bytes(&token.public_key)?;
        let sig = Signature::from_slice(&token.signature)?;
        vk.verify(&body, &sig)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify() {
        let issuer = TokenIssuer::new("mesh-a");
        let tok = issuer.issue(Role::Operator, 3600, Some("beta".into()));
        TokenIssuer::verify(&tok).unwrap();
        let enc = issuer.encode(&tok);
        let dec = TokenIssuer::decode(&enc).unwrap();
        assert_eq!(dec.claims.mesh_id, "mesh-a");
    }
}
