//! sonus-mcp — the stdio MCP server (BACKLOG S3).
//!
//! Drop-in for the Python hermes-sonus plugin: SAME tool names + argument
//! shapes (docs/hermes-parity.md). Until S3 lands this stub only proves the
//! workspace links and reminds the runner what it will become.
//!
//! S3 requirements (learned the hard way in the siblings):
//!   - tracing to STDERR only (stdout is the JSON-RPC stream — the
//!     Imaginarium B2 lesson)
//!   - serial loop is fine v1; long polls belong in check_status_until_done
//!     with resumable-timeout semantics
//!   - extended tools answer with an honest not-yet error, never fake success

fn main() {
    let cfg = sonus_core::Config::from_env();
    eprintln!(
        "sonus-mcp {} — scaffold (S3 pending). base={} key={} downloads={}",
        env!("CARGO_PKG_VERSION"),
        cfg.api_base,
        if cfg.api_key.is_some() {
            "present"
        } else {
            "ABSENT"
        },
        cfg.download_dir.display(),
    );
    std::process::exit(2); // honest: not a server yet
}
