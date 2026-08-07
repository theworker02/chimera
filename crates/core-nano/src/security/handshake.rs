//! Hybrid PQ handshake: ML-KEM-768 + ML-DSA-65 (pure Rust).

use alloc::boxed::Box;
use alloc::vec::Vec;

use ::kem::{Decapsulate, Encapsulate};
use ml_dsa::{
    EncodedSignature, EncodedVerifyingKey, KeyGen, MlDsa65, Signature as DsaSignature, SigningKey,
    VerifyingKey,
};
use ml_kem::array::Array as KemArray;
use ml_kem::{Ciphertext, Encoded, EncodedSizeUser, KemCore, MlKem768, SharedKey};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};

type Ek = <MlKem768 as KemCore>::EncapsulationKey;
type Dk = <MlKem768 as KemCore>::DecapsulationKey;
type Ct = Ciphertext<MlKem768>;
type Ss = SharedKey<MlKem768>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PqHandshakeError {
    Kem,
    Sign,
    Verify,
    Transcript,
    Puzzle,
}

#[derive(Clone)]
pub struct PqKeyPair {
    kem_dk: Box<Dk>,
    kem_ek: Box<Ek>,
    dsa_sk: Box<SigningKey<MlDsa65>>,
    dsa_vk: Box<VerifyingKey<MlDsa65>>,
}

impl PqKeyPair {
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let (dk, ek) = MlKem768::generate(rng);
        let kp = MlDsa65::key_gen(rng);
        Self {
            kem_dk: Box::new(dk),
            kem_ek: Box::new(ek),
            dsa_sk: Box::new(kp.signing_key().clone()),
            dsa_vk: Box::new(kp.verifying_key().clone()),
        }
    }

    pub fn kem_ek_bytes(&self) -> Vec<u8> {
        self.kem_ek.as_bytes().to_vec()
    }

    pub fn dsa_vk_bytes(&self) -> Vec<u8> {
        self.dsa_vk.encode().to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HybridHello {
    pub kem_ek: Vec<u8>,
    pub dsa_vk: Vec<u8>,
    pub nonce: [u8; 32],
    pub puzzle_challenge: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HybridReply {
    pub kem_ciphertext: Vec<u8>,
    pub dsa_vk: Vec<u8>,
    pub nonce: [u8; 32],
    pub puzzle_response: u32,
    pub signature: Vec<u8>,
    pub transcript_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridSession {
    pub shared_secret: [u8; 32],
    pub peer_dsa_vk: Vec<u8>,
    pub transcript_hash: [u8; 32],
}

fn transcript(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha3_256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub fn solve_puzzle(challenge: &[u8; 32], bits: u32) -> u32 {
    let bits = bits.min(16);
    for x in 0..u32::MAX {
        if verify_puzzle(challenge, x, bits) {
            return x;
        }
    }
    0
}

pub fn verify_puzzle(challenge: &[u8; 32], response: u32, bits: u32) -> bool {
    let bits = bits.min(16);
    let mut h = Sha3_256::new();
    h.update(challenge);
    h.update(&response.to_le_bytes());
    let out = h.finalize();
    let full = (bits / 8) as usize;
    let rem = bits % 8;
    for i in 0..full {
        if out[i] != 0 {
            return false;
        }
    }
    if rem > 0 && (out[full] >> (8 - rem)) != 0 {
        return false;
    }
    true
}

pub fn hybrid_handshake_start<R: RngCore + CryptoRng>(
    local: &PqKeyPair,
    rng: &mut R,
) -> HybridHello {
    let mut nonce = [0u8; 32];
    rng.fill_bytes(&mut nonce);
    let mut puzzle_challenge = [0u8; 32];
    rng.fill_bytes(&mut puzzle_challenge);
    HybridHello {
        kem_ek: local.kem_ek_bytes(),
        dsa_vk: local.dsa_vk_bytes(),
        nonce,
        puzzle_challenge,
    }
}

pub fn hybrid_handshake_finish<R: RngCore + CryptoRng>(
    local: &PqKeyPair,
    hello: &HybridHello,
    puzzle_bits: u32,
    rng: &mut R,
) -> Result<(HybridReply, HybridSession), PqHandshakeError> {
    let puzzle_response = solve_puzzle(&hello.puzzle_challenge, puzzle_bits);
    let ek = parse_ek(&hello.kem_ek)?;
    let (ct, ss): (Ct, Ss) = ek.encapsulate(rng).map_err(|_| PqHandshakeError::Kem)?;
    let ct_bytes = ct.to_vec();

    let mut nonce = [0u8; 32];
    rng.fill_bytes(&mut nonce);

    let th = transcript(&[
        b"CNK-PQ-v1",
        &hello.kem_ek,
        &hello.dsa_vk,
        &hello.nonce,
        &ct_bytes,
        &local.dsa_vk_bytes(),
        &nonce,
        &puzzle_response.to_le_bytes(),
    ]);

    let sig = local
        .dsa_sk
        .sign_deterministic(&th, &[])
        .map_err(|_| PqHandshakeError::Sign)?;
    let signature = sig.encode().to_vec();
    let shared_secret = mix_secret(&ss, &th);

    Ok((
        HybridReply {
            kem_ciphertext: ct_bytes,
            dsa_vk: local.dsa_vk_bytes(),
            nonce,
            puzzle_response,
            signature,
            transcript_hash: th,
        },
        HybridSession {
            shared_secret,
            peer_dsa_vk: hello.dsa_vk.clone(),
            transcript_hash: th,
        },
    ))
}

pub fn hybrid_handshake_accept(
    local: &PqKeyPair,
    hello: &HybridHello,
    reply: &HybridReply,
    puzzle_bits: u32,
) -> Result<HybridSession, PqHandshakeError> {
    if !verify_puzzle(&hello.puzzle_challenge, reply.puzzle_response, puzzle_bits) {
        return Err(PqHandshakeError::Puzzle);
    }
    let ct = parse_ct(&reply.kem_ciphertext)?;
    let ss: Ss = local
        .kem_dk
        .decapsulate(&ct)
        .map_err(|_| PqHandshakeError::Kem)?;

    let th = transcript(&[
        b"CNK-PQ-v1",
        &hello.kem_ek,
        &hello.dsa_vk,
        &hello.nonce,
        &reply.kem_ciphertext,
        &reply.dsa_vk,
        &reply.nonce,
        &reply.puzzle_response.to_le_bytes(),
    ]);
    if th != reply.transcript_hash {
        return Err(PqHandshakeError::Transcript);
    }

    let vk = parse_vk(&reply.dsa_vk)?;
    let sig = parse_sig(&reply.signature)?;
    if !vk.verify_with_context(&th, &[], &sig) {
        return Err(PqHandshakeError::Verify);
    }

    Ok(HybridSession {
        shared_secret: mix_secret(&ss, &th),
        peer_dsa_vk: reply.dsa_vk.clone(),
        transcript_hash: th,
    })
}

fn mix_secret(ss: &Ss, th: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha3_256::new();
    h.update(ss.as_slice());
    h.update(th);
    h.finalize().into()
}

fn parse_ek(bytes: &[u8]) -> Result<Ek, PqHandshakeError> {
    let enc: Encoded<Ek> = KemArray::try_from(bytes).map_err(|_| PqHandshakeError::Kem)?;
    Ok(Ek::from_bytes(&enc))
}

fn parse_ct(bytes: &[u8]) -> Result<Ct, PqHandshakeError> {
    KemArray::try_from(bytes).map_err(|_| PqHandshakeError::Kem)
}

fn parse_vk(bytes: &[u8]) -> Result<VerifyingKey<MlDsa65>, PqHandshakeError> {
    let enc = EncodedVerifyingKey::<MlDsa65>::try_from(bytes).map_err(|_| PqHandshakeError::Verify)?;
    Ok(VerifyingKey::<MlDsa65>::decode(&enc))
}

fn parse_sig(bytes: &[u8]) -> Result<DsaSignature<MlDsa65>, PqHandshakeError> {
    let enc = EncodedSignature::<MlDsa65>::try_from(bytes).map_err(|_| PqHandshakeError::Verify)?;
    DsaSignature::<MlDsa65>::decode(&enc).ok_or(PqHandshakeError::Verify)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    struct StepRng(u64);
    impl RngCore for StepRng {
        fn next_u32(&mut self) -> u32 {
            self.next_u64() as u32
        }
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            self.0
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for chunk in dest.chunks_mut(8) {
                let v = self.next_u64().to_le_bytes();
                chunk.copy_from_slice(&v[..chunk.len()]);
            }
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }
    impl CryptoRng for StepRng {}

    #[test]
    fn pq_handshake_agrees() {
        let mut rng = StepRng(0xDEAD_BEEF);
        let alice = PqKeyPair::generate(&mut rng);
        let bob = PqKeyPair::generate(&mut rng);
        let hello = hybrid_handshake_start(&alice, &mut rng);
        let (reply, bob_sess) = hybrid_handshake_finish(&bob, &hello, 8, &mut rng).unwrap();
        let alice_sess = hybrid_handshake_accept(&alice, &hello, &reply, 8).unwrap();
        assert_eq!(alice_sess.shared_secret, bob_sess.shared_secret);
    }
}
