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
