# Fixture provenance

Reconstructed 2026-07-30 from field evidence, not guesses:

- **Shapes/keys**: hermes-sonus-v2's parsing code + its hand-written test
  mocks (`tests/test_suno.py`, `tests/test_mcp_layer.py`) — two fixtures here
  are those mocks verbatim (`*_hermes_mock.json`, `*_tracks_no_status.json`).
- **Values**: a real captured V5 run (2026-04-25, `~/.hermes/sonus/music/
  tasks.json`): 32-hex dashless taskId, dashed-UUID track ids, float
  durations (168.6/172.72), `tempfile.aiquickdraw.com` relay-CDN URLs,
  exactly 2 variants sharing a title.
- **Statuses**: sunoapi.org's documented set. The four failure statuses are
  the ones hermes never handled (its weakness — we terminate on them).

S4 (live-fire) replaces/augments these with real captured response JSON —
that's the slice's explicit deliverable.

## The S4 live capture (2026-07-30) — `record_info_success_live.json`

Verbatim `GET /generate/record-info` for task
`bb69b305b057b6182f2292496372801c` — **APEX's first real compose through
Sonus-RS** ("Same Voice, New Bones", apex-3, model V5/chirp-crow, the
counted run). The reference truth the parsers are tested against.

What it proved beyond the reconstruction: `data.param` echoes our exact
request body (camelCase + the localhost callBackUrl, accepted in
production); `status` arrives UPPERCASE; `errorCode`/`errorMessage` are
present-but-null on success; `sourceAudioUrl` is now the real Suno CDN
(`cdn1.suno.ai`) while `audioUrl` is the relay — both unauthenticated.
The reconstructed fixtures stay: they cover shapes (hermes mocks, failure
statuses) the happy path can't.
