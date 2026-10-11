# Current AAC packet budget in the video duration proof

See the [qualification](../../../../docs/qualification/mux-preroll-2026-10-11.md)
for the gate failure, reconstructed regression fixture and verification.

`reordered-current-aac.mp4` combines the retained real HEVC stream with freshly
encoded current AAC. Every packet byte and clock is matched to its source in
`fixture-identity.json.gz`; only the retained 47-tick video duration defect is
reintroduced after diagnostic stream copy. This is synthetic owned content.

`checks/` records the failing and passing regressions and other checks.
`scripts/` records their local commands, including unsuccessful attempts to
capture a fresh variable-order failure. `source-identity.json` binds the final
reviewed code. The original full gate log is retained with the
[network evidence](../2026-10-11-youtube-network/README.md).
`SHA256SUMS` covers every file except itself; deterministic compressed logs
retain their original hashes and lengths in `compression.json`.
