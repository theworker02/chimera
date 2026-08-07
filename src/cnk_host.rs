//! Phase 6 CNK integration — PQ envelope + nano-kernel hooks for the host node.

use std::sync::OnceLock;

use chimera_nano_kernel::hw::HwProfile;
use chimera_nano_kernel::security::{
    hybrid_handshake_accept, hybrid_handshake_finish, hybrid_handshake_start, HybridSession,
    PeerBook, RateLimiter, PqKeyPair,
};
use chimera_nano_kernel::{boot, CNK_VERSION};
use rand_core::{CryptoRng, RngCore};

struct HostRng;

impl RngCore for HostRng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        getrandom::getrandom(&mut b).expect("getrandom");
        u32::from_le_bytes(b)
    }

    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        getrandom::getrandom(&mut b).expect("getrandom");
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        getrandom::getrandom(dest).expect("getrandom");
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        getrandom::getrandom(dest)
            .map_err(|_| rand_core::Error::from(core::num::NonZeroU32::new(1).unwrap()))
    }
}

impl CryptoRng for HostRng {}

fn generate_pq() -> PqKeyPair {
    // Dilithium keygen is stack-heavy in debug builds.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let mut rng = HostRng;
            PqKeyPair::generate(&mut rng)
        })
        .expect("pq keygen thread")
        .join()
        .expect("pq keygen join")
}

pub struct CnkHost {
    pub profile: HwProfile,
    pq: OnceLock<PqKeyPair>,
    pub rate: RateLimiter,
    pub peers: PeerBook,
}

impl CnkHost {
    pub fn bootstrap() -> Self {
        let profile = HwProfile::host();
        let _ = boot(profile.clone());
        Self {
            profile,
            pq: OnceLock::new(),
            rate: RateLimiter::new(64, 1000),
            peers: PeerBook::new(),
        }
    }

    pub fn version() -> u16 {
        CNK_VERSION
    }

    pub fn pq(&self) -> &PqKeyPair {
        self.pq.get_or_init(generate_pq)
    }

    pub fn establish_session_with(&self, peer: &PqKeyPair) -> HybridSession {
        let local = self.pq();
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn({
                let hello_local = local.clone();
                let peer = peer.clone();
                move || {
                    let mut rng = HostRng;
                    let hello = hybrid_handshake_start(&hello_local, &mut rng);
                    let (reply, theirs) =
                        hybrid_handshake_finish(&peer, &hello, 8, &mut rng).expect("pq finish");
                    let ours =
                        hybrid_handshake_accept(&hello_local, &hello, &reply, 8).expect("pq accept");
                    debug_assert_eq!(ours.shared_secret, theirs.shared_secret);
                    ours
                }
            })
            .expect("pq session thread")
            .join()
            .expect("pq session join")
    }
}
