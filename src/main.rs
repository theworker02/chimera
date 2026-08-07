use clap::Parser;
use tracing_subscriber::EnvFilter;

use chimera::brand::print_banner;
use chimera::config::NodeConfig;
use chimera::node::ChimeraNode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Ensure ring crypto provider for rustls/quinn.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("chimera=info,warn")),
        )
        .with_target(false)
        .init();

    let config = NodeConfig::parse();
    if !config.use_tui() {
        print_banner();
    }

    ChimeraNode::new(config)
        .run()
        .await
        .map_err(|e| {
            eprintln!("chimera error: {e:#}");
            e
        })
}
