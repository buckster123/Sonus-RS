# Sonus-RS — Backlog (slice ledger)

The studio-arc discipline: one slice = one PR off fresh main; ✅ only after
field verification. Contract: `docs/hermes-parity.md`.

## v0.1 — the drop-in (2026-07-30, one window S0→S5)

- [x] **S0 — bootstrap** (2026-07-30): workspace scaffold, CLAUDE.md, parity
      contract extracted from hermes-sonus-v2, CI (fmt/clippy/test/build),
      compiling skeleton with config + type stubs.
- [x] **S1 — the suno client** (PR #1; live-fire stamp rides S4)
      (`sonus-core`): config/env resolution, Bearer client, `POST /generate`
      + `GET /generate/record-info` + `GET /generate/credit`, typed task
      lifecycle, fixture tests, resumable-timeout semantics.
- [x] **S2 — download + library** (PR #2): downloads into
      `SUNO_DOWNLOAD_DIR`, hermes filenames, .part→rename atomicity, dedupe,
      statvfs disk guard, keyless CDN client.
- [x] **S3 — sonus-mcp** (PR #3): stdio JSON-RPC server, 16 hermes-parity
      tools (7 core + 9 honest not-yets), agentd-posture stdio smokes.
- [ ] **S4 — live-fire integration**: APEX's first real compose on apex-3 is
      the counted run (`check_credits` first); capture the real record-info
      JSON into the fixture set (curl with the node key, task_id from APEX).
- [x] **S5 — ApexOS integration** (PR #4 + ApexOS-RS #302/#303): env-file
      fallback, `sonus_provision()`, `/etc/sonus/env`, stanza swap, policy
      seeds, `apexos-update --sonus` flag path.
- [ ] **S6 — cutover + field**: apex-3 INSTALLED via `--sonus` (2026-07-30) ✓;
      remaining: APEX composes through it (zero soul churn), files visible in
      the 🎵 app + SCORE picker, python venv retired. Field finale: compose →
      SCORE → the Cutting Room renders a scored cut on an all-Rust pipeline.

## v0.2 — the standalone arc (queued 2026-07-30, behind the S4/S6 stamp)

Sonus stands on its own feet: an AI music app for ANY system — MCP for
agents, CLI for humans and scripts, HTTP + studio for browsers. The sibling
lego-brick surface set; field truth first, then new surface area.

- [ ] **S7 — the CLI**: thin `sonus` binary over sonus-core — `credits`,
      `gen -s "styles" [-l lyrics] [--model V5] [--wait] [--download]`,
      `status <task_id>`, `ls` (library). Env-file aware like the MCP;
      human-first output; no spend without an explicit `gen`.
- [ ] **S8 — `sonus serve` + the studio**: axum daemon (loopback default,
      LAN bearer token opt-in — the imaginarium shape) over sonus-core:
      `POST /v1/generate`, `GET /v1/tasks/{id}`, `GET /v1/credits`,
      `GET /v1/library` + `/v1/stream/{file}`. Embedded zero-install studio
      (ONE file, vanilla JS — see the frontend rule): credits chip, compose
      form (styles/lyrics/sliders/model/instrumental), polling job cards,
      library list with an `<audio>` player. Serve-mode nodes may hold the
      key daemon-side; an MCP proxy mode follows only if demand pulls.

### Frontend rule (decided 2026-07-30)

**No build step → vanilla JS. Build step → TypeScript.** Embedded
single-file studio pages stay vanilla (a tsc/node_modules toolchain inside
a one-binary Rust repo contradicts the values that put Rust here). The day
a UI is big enough to want a framework, it's big enough to deserve TS —
Vite + Vue + TS, with wire types DERIVED from the Rust API structs via
`ts-rs` (one source of truth, typed end to end).

Doors parked, not closed: an opt-in TS "composing workstation" studio for
power nodes that can carry the build stack; the full-meme Rust/WASM
frontend (Leptos/Yew) only if something ever needs a real-time canvas.

## Post-v1 parking

Extended tools (album/stems/voice/wav/video/sfx) as demand pulls; upload_audio
(`SUNO_FILE_API_BASE`); MCP audio hand-off to Imaginarium (frame-cap aware);
Nano field test on a Zero 2W (the whole point — measure RSS and boast).
