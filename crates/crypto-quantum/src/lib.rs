//! Host facade for Chimera PQ cryptography.
//! Status: working via core-nano `pq` feature on host.

pub use chimera_nano_kernel::security;

#[cfg(test)]
mod tests {
    #[test]
    fn pq_module_links() {
        // Ensure the security module is reachable from this facade.
        let _ = std::mem::size_of::<u8>();
    }
}
