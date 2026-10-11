# Proxy encoder investigation and retry regression

See the [qualification](../../../../docs/qualification/proxy-order-2026-10-11.md)
for results and limits. This evidence does not claim the encoder service is
fixed. Production code, packet checks and the one-retry limit are unchanged.

- `sequential`, `concurrent-320x180`, `actual-cfr` and `capture-first-order`
  contain all 1,800 probe summaries, exact diagnostic sources and compiler /
  executable identities. The diagnostic sources replace packet admission and
  muxing with tracing and independent decoding; they never make valid proxies.
- All files from the three timed-out trials are retained, including empty and
  partially buffered files. `actual-cfr/trial-323/trace.jsonl` records the
  adjacent PTS swap. Its empty packet file prevents a claim about those two
  pictures' identities. The successful sequential sample includes decoded
  binary picture IDs.
- `environment.json` records the host and qualified libraries.
  `original-decode.json` identifies the separate development FFmpeg 9.0.1
  executable used to prepare diagnostic RGBA input. Probe encoders and their
  independent decoder use the qualified 8.0.3 libraries.
- `make-retry-fixture.json`, its log and `scripts/make_retry_fixture.py`
  retain the real production worker request, reply, hashes and regeneration
  command. The resulting media and reply live in the canonical
  [test fixtures](../../../../native/deadpan-media-worker/tests/fixtures/proxy/README.md).
- `checks.json`, `followup-checks.json` and `checks/` retain formatting,
  strict Clippy, all seventeen proxy media tests, both lingering-output
  follow-ups and twenty consecutive lifecycle test passes. No full workspace,
  sanitizer or bundle check was repeated for this test/fixture-only change.
- `source-identity.json` identifies the base and final test/fixture bytes.
  `compressed-originals.json` records decompressed sizes and SHA-256 hashes;
  `SHA256SUMS` covers every retained file except itself.

Scripts are retained records, not a portable test runner. Copy them to a fresh
scratch directory and adjust their explicit paths before rerunning. Full
successful captures, RGBA input and diagnostic binaries remain under
`/tmp/deadpan-proxy-order-20261011`; their identities are retained here.
