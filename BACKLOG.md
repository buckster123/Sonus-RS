# Sonus-RS — Backlog (slice ledger)

The studio-arc discipline: one slice = one PR off fresh main; ✅ only after
field verification. Contract: `docs/hermes-parity.md`.

- [x] **S0 — bootstrap** (2026-07-30): workspace scaffold, CLAUDE.md, parity
      contract extracted from hermes-sonus-v2, CI (fmt/clippy/test/build),
      compiling skeleton with config + type stubs.
- [ ] **S1 — the suno client** (`sonus-core`): config/env resolution, Bearer
      client, `POST /generate` + `GET /generate/record-info` + `GET
      /generate/credit`, typed task lifecycle (submitted → pending → complete
      → files), captured-JSON fixture tests, resumable-timeout semantics.
- [ ] **S2 — download + library**: track download into `SUNO_DOWNLOAD_DIR`,
      honest filenames, dedupe, disk-space guard.
- [ ] **S3 — sonus-mcp**: stdio JSON-RPC server with the core-loop tools
      (`generate_song`, `check_status`, `check_status_until_done`,
      `download_track`, `extend_track`, `generate_lyrics`, `check_credits`) —
      names/args per parity doc; extended tools return honest not-yet errors.
      Smoke over real stdio (the Imaginarium #8 pattern).
- [ ] **S4 — live-fire integration**: one counted run against sunoapi.org with
      André's credits (`check_credits` first); capture real response JSON into
      the fixture set.
- [ ] **S5 — ApexOS integration** (ApexOS-RS PR): `sonus_provision()` in
      install.sh, `/etc/sonus/env` seeding, plugins.toml stanza swap
      python→rust, docs (imaginarium-pattern).
- [ ] **S6 — cutover + field**: apex-3 runs Sonus-RS, APEX composes through it
      (same tool names — zero soul churn), python venv retired. Field finale:
      compose → SCORE → the Cutting Room renders a scored cut end-to-end on
      an all-Rust pipeline.

## Post-v1 parking

Extended tools (album/stems/voice/wav/video/sfx) as demand pulls; upload_audio
(`SUNO_FILE_API_BASE`); MCP audio hand-off to Imaginarium (frame-cap aware);
Nano field test on a Zero 2W (the whole point — measure RSS and boast).
