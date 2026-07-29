//! sonus-core — the Suno (sunoapi.org) client the MCP surface stands on.
//!
//! Module plan (BACKLOG S1–S2):
//!   config  — env resolution (S0 stub below, grow in S1)
//!   client  — Bearer HTTP client: generate / record-info / credit  (S1)
//!   types   — task lifecycle: Submitted → Pending → Complete{tracks} (S1)
//!   library — downloads into SUNO_DOWNLOAD_DIR, honest names, dedupe (S2)
//!
//! Invariants (CLAUDE.md has the long form):
//!   - a paid task is never stranded: poll timeouts stay resumable
//!   - no call auto-spends: check_credits is free, generation is explicit
//!   - the key is read from env/file, never logged, never serialized

pub mod config;

pub use config::Config;
