//! sonus-core — the Suno (sunoapi.org) client the MCP surface stands on.
//!
//! Modules (BACKLOG S1–S2):
//!   config  — env resolution (S0)
//!   error   — honest error surface + the documented code table (S1)
//!   types   — wire types + pure parsers: lifecycle, tracks, credits (S1)
//!   client  — Bearer HTTP: generate / record-info / credit + poll loop (S1)
//!   library — downloads into SUNO_DOWNLOAD_DIR, honest names, dedupe (S2)
//!
//! Invariants (CLAUDE.md has the long form):
//!   - a paid task is never stranded: poll timeouts stay resumable
//!     (PollOutcome::TimedOut carries the last snapshot; the id stays valid)
//!   - no call auto-spends: check_credits is free, generation is explicit
//!   - the key is read from env/file, never logged, never serialized

pub mod client;
pub mod config;
pub mod error;
pub mod types;

pub use client::{PollOutcome, SunoClient};
pub use config::Config;
pub use error::SonusError;
pub use types::{Credits, GenerateParams, RecordInfo, TaskStatus, Track};
