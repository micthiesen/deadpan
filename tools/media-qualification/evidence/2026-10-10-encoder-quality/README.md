# Encoder quality evidence

See [the qualification](../../../../docs/qualification/encoder-quality-2026-10-10.md).
This directory retains measurements and the exact experimental source, not an
alternate production encoder. `manifest.json` hashes the retained files.

- `quality-full-matrix.json` reproduces the concurrent scene-cut failure with
  the original encoder and compares a QP-only candidate.
- `quality-bounded-matrix.json` adds the three-times-target data-rate ceiling.
- `quality-all-frames.json` summarizes complete raw-I420 comparisons; compressed
  per-frame records also retain independent decoded H.264 block-QP observations.
- `quality-packet-rates.json` measures every contiguous 24-picture window in
  these 24 fps, no-B-frame movies. Compressed packet tables retain the inputs.
- `vt-quality-properties.log` and the software regression log establish the
  ignored software QP property. That candidate is not qualified.
- `quality-ignored-software/` reproduces that ignored request with the exact
  grain/edge input and retains independent source, executable and movie hashes.
- `quality-followup-checks.json`, `quality-builds.json` and the compressed logs
  preserve failed gate runs, corrected fixture checks, the isolated stdio
  follow-up, native sanitizers and release/package commands. The sanitizer
  report records its actual test binaries and the limited instrumentation scope.
- `quality-source-identity.json` hashes the changed production/test files before
  the verified build. Fixture producer provenance is also checked in beside
  the small real MP4s under `crates/deadpan-cli/tests/encoded_verification`.
- `quality-public.json` and `quality-public-packet-rates.json` bind both
  concurrent release/packaged exports to their exact commands, binaries,
  movies, complete picture comparisons and bounded one-second rates.
  The two `quality-public-*` directories retain full publication and verifier
  reports plus compressed frame/packet observations.
- Source and command snapshots use `.txt`; their absolute paths record the
  executed experiment. `baseline-encoder.c.txt` and `baseline-encoder.h.txt`
  are from `fdef263d`. Prototype binaries used that ABI-2 header, not the later
  production ABI-3 header.

The complete picture reference comes from the shared renderer, using the
project/revision and hash recorded in the qualification and extraction report.
The full-clip probes are deliberately fixed to that 3,506-picture, 1920x1080,
24 fps fixture. Their exit status means measurement completed; the reports and
unchanged 32 dB threshold establish the measured outcome. The retained scripts
must not be pointed at another clip without updating that contract.

The reusable, asset-free production regression is:

```sh
cargo test --locked -p deadpan-encode --test sdr_quality -- --nocapture
```

Set `DEADPAN_FFMPEG_PREFIX` to the qualified prefix as described in
[development](../../../../docs/DEVELOPMENT.md). The test compares real decoded
grain/edge pixels from two concurrent hardware sessions. It also requires the
software mode either to meet that quality check or to explicitly reject its
unsupported quality controls before encoding. Silent fallback is not accepted.
