# The hermes-sonus parity contract

> Extracted 2026-07-30 from `~/Projects/hermes-sonus-v2` (the live Python
> plugin). This is THE drop-in contract: same tool names, same argument
> shapes, same upstream. Verify against the source before implementing a
> tool — line refs below point into `hermes_sonus/`.

## Upstream (sunoapi.org)

- Base: `https://api.sunoapi.org/api/v1` (`SUNO_API_BASE` / `SUNO_BASE_URL`
  override). Auth: `Authorization: Bearer $SUNO_API_KEY` + JSON bodies.
- Rate ceiling per their docs: **20 requests / 10 s** (`mcp/batch_generate.py:75`).

| Verb | Endpoint | Purpose |
|---|---|---|
| POST | `/generate` | submit a generation (returns task id) |
| GET  | `/generate/record-info?taskId=` | poll a task (the lifecycle read) |
| POST | `/generate/extend` | extend an existing track |
| POST | `/generate/upload-cover` | cover an uploaded audio |
| POST | (lyrics endpoint — see `suno.py:321 submit_lyrics`) | lyrics only |
| GET  | `/generate/credit` | remaining credits (FREE — check before spending) |
| POST | `/voice/generate` | voice clone ops |
| POST | `/vocal-removal/generate` | stem separation |

(Plus wav-conversion / music-video / sound-fx endpoints behind the tail tools —
read `music/suno.py` + `mcp/server.py` per tool when implementing.)

## The MCP tool surface (16 tools, FastMCP names — keep them EXACTLY)

Core compose loop (v1 scope, S1–S3):

| Tool | Notes |
|---|---|
| `generate_song` | prompt/style/lyrics/model knobs → task id (`server.py:106`) |
| `check_status` | one poll (`:203`) |
| `check_status_until_done` | poll loop w/ timeout (`:234`) — leave paid tasks resumable on timeout |
| `download_track` | task → files into `SUNO_DOWNLOAD_DIR` (`:290`) |
| `extend_track` | (`:444`) |
| `generate_lyrics` | (`:505`) |
| `check_credits` | (`:614`) free — the spend gate |

Extended surface (post-v1 unless demand pulls them forward — an honest
"not implemented yet in Sonus-RS" error is the correct v1 behavior):

| Tool | Notes |
|---|---|
| `generate_album` | batch of generate_song (`:340`; rate-limit aware) |
| `separate_stems` | (`:640`) |
| `replace_section` | (`:663`) |
| `boost_style` | (`:688`) |
| `convert_to_wav` | (`:~712`) |
| `create_music_video` | (`:~715`) |
| `generate_sounds` | SFX, model V5 (`:~718`) |
| `clone_voice_validate` / `clone_voice_create` | (`:535`/`:567`) |

## Env contract

| Var | Meaning | v1? |
|---|---|---|
| `SUNO_API_KEY` | the money key — `/etc/sonus/env` ONLY on nodes | ✔ |
| `SUNO_API_BASE` / `SUNO_BASE_URL` | upstream override | ✔ |
| `SUNO_DOWNLOAD_DIR` | where tracks land (agentd: `workspace/sonus`) | ✔ |
| `SUNO_CALLBACK_*` (4 vars) | legacy callback-server flow | ✘ — poll-only v1 |
| `SUNO_FILE_API_BASE` | upload transport | with `upload_audio` support |

## Known Python-side weaknesses (fix, don't port)

- Polling that gives up can strand paid tasks — keep task ids resumable
  (the Imaginarium B1 lesson).
- The venv footprint is the whole reason this port exists — keep deps lean
  (reqwest + serde + tokio; think before every `cargo add`).
- Playback/EEG/dashboard concerns from hermes do NOT come along; ApexOS-RS
  owns playback already (`/api/sonus/*`).
