<div align="center">

<img src="assets/banner.jpg" alt="Sonus-RS" width="100%">

<h1>Sonus-RS</h1>

<p><strong>Pure-Rust music generation for agents.</strong><br>
A lean Suno client + MCP server — any old Pi becomes a composer.</p>

<p>
<img alt="license" src="https://img.shields.io/badge/license-MIT-blue">
<img alt="rust" src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white">
<img alt="ci" src="https://img.shields.io/github/actions/workflow/status/buckster123/Sonus-RS/ci.yml?label=ci">
<img alt="status" src="https://img.shields.io/badge/status-v0.1%20%C2%B7%20core%20loop%20complete-brightgreen">
</p>

</div>

---

> [!NOTE]
> **Bring your own key.** Sonus-RS talks to [sunoapi.org](https://sunoapi.org)
> with your `SUNO_API_KEY`. Generation spends real credits — `check_credits`
> is free and the tools tell the agent to call it first. On ApexOS nodes the
> key lives in **one** root-owned file (`/etc/sonus/env`) that the plugin
> reads itself; the host daemon never sees it.

## The three senses

[**Occipital-RS**](https://github.com/buckster123/Occipital-RS) = web ·
[**Imaginarium-RS**](https://github.com/buckster123/Imaginarium-RS) = vision ·
**Sonus-RS** = sound. Three standalone Rust siblings of
[ApexOS-RS](https://github.com/buckster123/ApexOS-RS), same integration
pattern: clone, build, register as an MCP plugin, and the resident agent
gains a sense.

## Why

The predecessor ([hermes-sonus](https://github.com/buckster123/hermes-sonus))
proved the loop — an agent that composes its own music — but it's a Python
venv, and a venv is a heavy passenger on a Pi Zero. Sonus-RS is the drop-in
Rust replacement: **the same 16 MCP tool names**, so agent souls, procedures
and habits carry over with zero churn, in a single self-contained binary with
single-digit-MB RSS and no local ML.

- **Agents compose conversationally** — `generate_song` → poll → tracks land
  on disk where the player already looks.
- **Honest by construction** — a paid task is never stranded (poll timeouts
  return the task id, resumable for days), failures carry upstream's reason,
  unimplemented tools say so instead of faking success.
- **Nano-first** — an HTTP client, a poller and a downloader. That's the
  whole footprint.

## The compose loop

```
check_credits            free — THE spend gate
   └─ generate_song      styles / lyrics / model / sliders → task_id  (2 variants)
        └─ check_status_until_done    resumable timeout, backoff 5s → 30s
             └─ download_track        → SUNO_DOWNLOAD_DIR, honest names, dedupe
```

Plus `check_status` (one poll), `extend_track`, `generate_lyrics`. Nine
extended tools (`generate_album`, `separate_stems`, `convert_to_wav`, …)
answer an honest *"not implemented yet in Sonus-RS (post-v1)"*.

## Quickstart

**On an ApexOS-RS node** (the intended home):

```sh
apexos-update --sonus            # clone + build + register (opt-in add-on)
echo 'SUNO_API_KEY=…' | sudo tee -a /etc/sonus/env
apexos-update                    # key present → agent tools activate
```

**Standalone, with any MCP client** (Claude Code, etc.):

```sh
cargo build --release -p sonus-mcp
SUNO_API_KEY=… SUNO_DOWNLOAD_DIR=~/Music/sonus ./target/release/sonus-mcp
```

```json
{ "mcpServers": { "sonus": { "command": "/path/to/sonus-mcp",
    "env": { "SUNO_API_KEY": "…", "SUNO_DOWNLOAD_DIR": "~/Music/sonus" } } } }
```

The server speaks newline-delimited JSON-RPC over stdio (MCP `2024-11-05`),
logs to stderr only, and stays up keyless — every money tool then answers
with the exact configuration fix instead of dying.

## Configuration

| Var | Default | Meaning |
|---|---|---|
| `SUNO_API_KEY` | — | the money key (BYOK) |
| `SUNO_DOWNLOAD_DIR` | `~/.local/share/sonus` | where tracks land |
| `SUNO_API_BASE` | `https://api.sunoapi.org/api/v1` | upstream override |
| `SONUS_ENV_FILE` | `/etc/sonus/env` | node env file, self-loaded |

Resolution: process env first, then the env file (empty vars don't shadow —
"drop the key in later" just works).

## Design notes

- **Contract-first.** `docs/hermes-parity.md` pins the entire wire contract —
  every field, status, and endpoint, extracted from the live Python plugin
  and a captured real generation — plus the deliberate divergences (the big
  one: upstream's four failure statuses are *terminal* here; the Python polls
  failed tasks until timeout).
- **Keyless downloads.** Generated audio lives on an unauthenticated relay
  CDN; the download client carries no credentials by design — your API key
  never travels to a third-party host (the test suite asserts it).
- **Atomic library.** Streams to `.part`, renames on completion, dedupes by
  deterministic name, guards disk space before writing. A partial download
  never masquerades as a finished track.
- **Tested against truth.** 63+ tests: fixture parsers built from real
  captured responses, socket-level client e2e, and two stdio smokes that
  drive the real binary over real JSON-RPC — the exact posture the host
  daemon uses.

```
crates/sonus-core/   suno client · lifecycle types · library (downloads)
crates/sonus-mcp/    the stdio MCP server (16 hermes-parity tools)
docs/hermes-parity.md  THE contract
BACKLOG.md           slice ledger (S0 → S6)
```

## Kin

Part of the [ApexOS](https://github.com/buckster123/ApexOS-RS) organism —
an agent OS for spare devices, where the resident composes its own film
scores: generate here, score in the
[Cutting Room](https://github.com/buckster123/ApexOS-RS/blob/main/docs/imagine-studio.md),
render via [Imaginarium-RS](https://github.com/buckster123/Imaginarium-RS).

## License

MIT. Banner generated by
[Imaginarium-RS](https://github.com/buckster123/Imaginarium-RS)
(job `01KYRAHMNF6AFATD88EH0G92VN`).
