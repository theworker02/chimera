//! Brand identity helpers — ANSI banner, palette, watermark hooks.

pub const VOID_BLACK: &str = "#0A0A0C";
pub const ELECTRIC_CYAN: &str = "#00F0FF";
pub const WARNING_AMBER: &str = "#FFB800";

pub const ANSI_CYAN: &str = "\x1b[38;2;0;240;255m";
pub const ANSI_AMBER: &str = "\x1b[38;2;255;184;0m";
pub const ANSI_RESET: &str = "\x1b[0m";
pub const ANSI_DIM: &str = "\x1b[2m";

/// Cyan-on-dark ASCII lockup for CLI / TUI launch.
pub fn ascii_banner() -> String {
    format!(
        "{cyan}{banner}{reset}\n{dim}  decentralized compute · ChimeraFS · ChimeraMEM · agents{reset}\n",
        cyan = ANSI_CYAN,
        reset = ANSI_RESET,
        dim = ANSI_DIM,
        banner = r#"
   ██████╗██╗  ██╗██╗███╗   ███╗███████╗██████╗  █████╗
  ██╔════╝██║  ██║██║████╗ ████║██╔════╝██╔══██╗██╔══██╗
  ██║     ███████║██║██╔████╔██║█████╗  ██████╔╝███████║
  ██║     ██╔══██║██║██║╚██╔╝██║██╔══╝  ██╔══██╗██╔══██║
  ╚██████╗██║  ██║██║██║ ╚═╝ ██║███████╗██║  ██║██║  ██║
   ╚═════╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝"#
    )
}

pub fn print_banner() {
    println!("{}", ascii_banner());
}

/// Cryptographic visual signature for Wasm output / block metadata.
pub fn payload_watermark(content_hash: &[u8; 32], node_hint: &str) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"CHIMERA-WATERMARK-v1");
    h.update(content_hash);
    h.update(node_hint.as_bytes());
    h.update(ELECTRIC_CYAN.as_bytes());
    *h.finalize().as_bytes()
}

pub fn ratatui_cyan() -> ratatui::style::Color {
    ratatui::style::Color::Rgb(0, 240, 255)
}

pub fn ratatui_amber() -> ratatui::style::Color {
    ratatui::style::Color::Rgb(255, 184, 0)
}

pub fn ratatui_void() -> ratatui::style::Color {
    ratatui::style::Color::Rgb(10, 10, 12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::{verify_receipt, ReceiptSigner};
    use crate::fs::ChimeraFs;
    use crate::intent::IntentCompiler;
    use crate::protocol::{NodeId, TaskId};

    #[test]
    fn receipt_roundtrip() {
        let signer = ReceiptSigner::generate();
        let r = signer.sign_receipt(
            TaskId::new(),
            NodeId::new(),
            *blake3::hash(b"out").as_bytes(),
            42,
            *blake3::hash(b"in").as_bytes(),
        );
        assert!(verify_receipt(&r));
    }

    #[test]
    fn intent_compiles_slices() {
        let intent = IntentCompiler::parse("name=t latency<100ms privacy=local slices=3");
        assert!(intent.privacy_local_only);
        assert_eq!(intent.latency_budget_ms, Some(100));
        let plan = IntentCompiler::new([0u8; 32]).compile(&intent);
        assert!(!plan.tasks.is_empty());
    }

    #[test]
    fn cas_merkle_roundtrip() {
        let dir = std::env::temp_dir().join(format!("chimera-test-{}", uuid::Uuid::new_v4()));
        let fs = ChimeraFs::open(&dir, NodeId::new(), 4096, 32).unwrap();
        let asset = fs.ingest_bytes("x.bin", b"hello-chimera").unwrap();
        let bytes = fs.store.read_asset_bytes(&asset).unwrap();
        assert_eq!(bytes, b"hello-chimera");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn watermark_stable() {
        let h = *blake3::hash(b"x").as_bytes();
        let a = payload_watermark(&h, "n");
        let b = payload_watermark(&h, "n");
        assert_eq!(a, b);
    }
}
