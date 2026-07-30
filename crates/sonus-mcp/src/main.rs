//! sonus-mcp binary — MCP-over-stdio for Suno music generation.
//! Logs → stderr. stdout is JSON-RPC only (the Imaginarium B2 lesson).

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    sonus_mcp::run().await
}
