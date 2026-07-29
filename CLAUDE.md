# Sonus-RS — Agent & Developer Guide

> Pure-Rust music-generation node for the ApexOS colony: a Suno API
> (sunoapi.org) client + MCP plugin that **drop-in replaces** the Python
> `hermes-sonus` plugin. Rust means every node composes — a Pi Zero 2W can't
> carry a Python venv, but it can carry this.
>
> The three senses, completed: **Occipital** = web · **Imaginarium** = vision ·
> **Sonus** = sound. All Rust siblings of ApexOS-RS, same integration pattern.

**Bootstrapped 2026-07-30** by the instance that shipped the ApexOS-RS Imagine
Studio arc (Cerebro agent `FORGE` — `session_recall(query="sonus-rs bootstrap
handoff", agent_id="FORGE")` for the full thread). The scaffold compiles; the
work starts at slice S1 in `BACKLOG.md`.

---

## Vision & shape

- **MCP-first**: `sonus-mcp` speaks the **same tool names** as hermes-sonus
  (`generate_song`, `check_status`, `download_track`, …) so APEX souls,
  procedures, and habits need **zero churn** on cutover. The full parity
  contract — every tool, endpoint, env var — is `docs/hermes-parity.md`.
- **Nano-first**: no local ML, no venv — an HTTP client + poller + downloader.
  Target RSS: single-digit MB. Any old Pi becomes a composer.
- **The player stays put**: downloads land in agentd's `workspace/sonus` dir;
  ApexOS-RS's `/api/sonus/*` routes, the Sonus app, and the Imagine SCORE
  picker (studio arc A6) all keep working **unchanged**.
- Post-v1 maybes live at the end of `BACKLOG.md`. EEG (hermes heritage) is
  explicitly **out of scope** — that half stays wherever hermes goes.

## Locked decisions

- **Language**: Rust, one Cargo workspace (`sonus-core` lib · `sonus-mcp` bin)
- **Upstream**: sunoapi.org (`https://api.sunoapi.org/api/v1`), auth
  `Authorization: Bearer $SUNO_API_KEY`. Poll-based lifecycle (submit →
  `GET /generate/record-info` until complete → download); **no callback
  server** in v1 (hermes' callback envs are legacy — the parity doc says so).
- **Key isolation**: `SUNO_API_KEY` lives ONLY in `/etc/sonus/env` on deployed
  nodes — the Imaginarium invariant (`/etc/imaginarium/env`) applied verbatim.
  agentd/UI never see the key; the MCP child reads it from its own env file.
- **Tool-name parity** with hermes-sonus is a contract, not a preference.
  Behavior parity for the compose loop (S1–S3); exotic tools may return an
  honest "not yet" — never a fake success.
- **Provisioning**: via ApexOS-RS `install.sh` (the occipital/imaginarium
  pattern): clone/build sibling, install binary, seed env, swap the
  `plugins.toml` stanza from `sonus-mcp` (python) to this binary.
  INSTALLED ≠ ACTIVE — activation requires the key.
- **CI from commit 0**: fmt `--check` + clippy `-D warnings` + test + build.
  The baseline is rustfmt-clean — `cargo fmt --all` freely (unlike ApexOS-RS,
  whose dirty baseline forbids it).

## The playbook (generalized from ApexOS-RS — read once, then live it)

1. **Contract first.** Before code: pin the wire contract in a doc
   (`docs/hermes-parity.md` here; openapi in Imaginarium). Code follows docs,
   PRs update both together. "Docs travel with code."
2. **Slices, not marathons.** One branch = one reviewable slice off
   freshly-fetched `origin/main`. Ship via PR; **André reviews and merges —
   never self-merge, never commit to main, never force-push or amend pushed
   commits.** After merge, branch fresh (check whether the repo squash-merges
   or merge-commits before reusing a branch).
3. **Honesty invariants.** A job is never stuck pending — every failure path
   flips it failed with the real reason. Degrades are stated, not masked
   ("no key configured" beats a timeout). Never silently clamp what you can
   honestly reject.
4. **Pure-fn test discipline.** The unit-test surface is pure functions:
   request builders, response parsers, state mappers. Side-effectful e2e tests
   gate on tool availability and skip loudly. Upstream calls get a mock-shape
   fixture test (real captured JSON) so parsers are tested against truth.
5. **Field truth beats green CI.** A slice is done when it's merged, deployed
   (`apexos-update`), and verified on a live node — screenshots, real jobs.
   The ledger row gets its ✅ only then.
6. **Secrets hygiene.** Never print keys/tokens (lengths and heads only);
   never write a secret into a repo, a transcript, or a non-0600 file.
   Minted/seeded secrets go to root-owned env files.
7. **Cerebro is the thread.** `session_recall` at start, `session_save` at
   milestones (agent_id `FORGE`). Deep windows checkpoint per-slice.
8. **Spend is gated.** Suno credits cost money: default flows must never
   auto-fire paid generations. Tests mock upstream; live-fire integration
   runs are explicit, counted, and use André's sunoapi.org credits
   (`check_credits` first — it's free).
9. **Cost the failure, not the happy path.** Poll timeouts must leave paid
   jobs recoverable (resumable task ids), mirroring Imaginarium's B1 lesson:
   a paid render that outlives a poll window is pending, not failed.

## Repo map

```
crates/sonus-core/   # suno client, config, types, download/library logic
crates/sonus-mcp/    # the MCP stdio server (FastMCP-parity tool surface)
docs/hermes-parity.md  # THE contract: tools × endpoints × env (extracted 2026-07-30)
BACKLOG.md           # slice ledger S0–S6 + post-v1 parking
```

## Dev commands

```bash
cargo test --workspace          # all tests (mocked upstream)
cargo fmt --all                 # clean baseline — always safe here
cargo clippy --workspace -- -D warnings
SUNO_API_KEY=… cargo run -p sonus-mcp   # stdio MCP (manual JSON-RPC smoke)
```

## Integration points (ApexOS-RS side — for the S5 slice)

- `config/plugins.toml`: the commented `sonus` stanza currently points at the
  python `sonus-mcp`; S5 swaps it to this binary (same plugin id, same tools).
- `deploy/` + `install.sh`: add `sonus_provision()` mirroring
  `imaginarium_provision()` (clone sibling → build → install → seed
  `/etc/sonus/env` → INSTALLED≠ACTIVE gate on `SUNO_API_KEY`).
- Downloads: honor `SUNO_DOWNLOAD_DIR` (agentd sets it to
  `/var/lib/agentd/workspace/sonus`); default `~/.local/share/sonus` off-node.

## Cerebro agent

All calls use agent `FORGE`. Session START:
`session_recall(query="sonus-rs", agent_id="FORGE")`. Session END + milestones:
`session_save(...)`. The bootstrap handoff note is saved under
"sonus-rs bootstrap handoff".
