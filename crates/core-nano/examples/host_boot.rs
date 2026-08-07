//! Host-simulated CNK boot demo (Windows / desktop).

use chimera_nano_kernel::executor::{NanoExecutor, NanoTask};
use chimera_nano_kernel::hw::HwProfile;
use chimera_nano_kernel::memory::ImmutableRegion;
use chimera_nano_kernel::{boot, CNK_VERSION};

fn main() {
    println!("Chimera Nano-Kernel v{CNK_VERSION} — host boot");
    let profile = HwProfile::host();
    let report = boot(profile.clone());
    println!(
        "platform={} ram={}MiB interpreter={} — {}",
        report.platform,
        report.ram_bytes / (1024 * 1024),
        report.prefer_interpreter,
        report.notes
    );

    let mut exec = NanoExecutor::new(profile);
    let out = exec.run(NanoTask {
        id: 7,
        seed: 42,
        elements: 128,
        consensus_math: true,
    });
    println!(
        "task {} checksum={:016x} elements={} degraded={}",
        out.id, out.checksum, out.elements_done, out.degraded
    );

    let ids = NanoExecutor::recover_completed_ids(&exec.log).expect("replay");
    println!("replay recovered ids: {ids:?}");

    let mut region = ImmutableRegion::new(128, 32);
    region.write(0, b"sealed-payload").unwrap();
    region.seal();
    println!(
        "immutable region pages={} sealed={}",
        region.page_count(),
        region.is_sealed()
    );

    #[cfg(feature = "net")]
    {
        use chimera_nano_kernel::frame::{msg, MeshFrame};
        use chimera_nano_kernel::net::sim_loopback_frame;
        let f = MeshFrame::new(msg::HEARTBEAT, 1, b"cnk".to_vec());
        let round = sim_loopback_frame(&f).expect("sim net");
        println!("smoltcp sim frame ok type={}", round.header.msg_type);
    }

    #[cfg(feature = "pq")]
    {
        // Run PQ on a large stack thread — Dilithium keygen is stack-heavy in debug.
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .name("cnk-pq".into())
            .spawn(|| {
                use chimera_nano_kernel::security::{
                    hybrid_handshake_accept, hybrid_handshake_finish, hybrid_handshake_start,
                    PqKeyPair,
                };
                use rand_core::{CryptoRng, RngCore};
                struct Step(u64);
                impl RngCore for Step {
                    fn next_u32(&mut self) -> u32 {
                        self.next_u64() as u32
                    }
                    fn next_u64(&mut self) -> u64 {
                        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
                        self.0
                    }
                    fn fill_bytes(&mut self, dest: &mut [u8]) {
                        for c in dest.chunks_mut(8) {
                            let v = self.next_u64().to_le_bytes();
                            c.copy_from_slice(&v[..c.len()]);
                        }
                    }
                    fn try_fill_bytes(
                        &mut self,
                        dest: &mut [u8],
                    ) -> Result<(), rand_core::Error> {
                        self.fill_bytes(dest);
                        Ok(())
                    }
                }
                impl CryptoRng for Step {}
                let mut rng = Step(1);
                let alice = PqKeyPair::generate(&mut rng);
                let bob = PqKeyPair::generate(&mut rng);
                let hello = hybrid_handshake_start(&alice, &mut rng);
                let (reply, s_bob) =
                    hybrid_handshake_finish(&bob, &hello, 8, &mut rng).expect("finish");
                let s_alice =
                    hybrid_handshake_accept(&alice, &hello, &reply, 8).expect("accept");
                assert_eq!(s_alice.shared_secret, s_bob.shared_secret);
                println!(
                    "PQ handshake OK shared={:02x?}",
                    &s_alice.shared_secret[..8]
                );
            })
            .expect("spawn pq")
            .join()
            .expect("pq thread");
    }

    println!("CNK host boot complete.");
}
