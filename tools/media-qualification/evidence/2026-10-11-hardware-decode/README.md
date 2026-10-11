# Optional VideoToolbox decode evidence, 2026-10-11

See [qualification](../../../../docs/qualification/hardware-decode-2026-10-11.md).
`manifest.json` records SHA-256 of every retained file and its uncompressed
original. `SHA256SUMS` also covers the manifest and this README. Large logs
and JSON are gzip-compressed deterministically. Source/test fixture pins and
executed binary hashes are retained separately from performance claims.

The final comparison binary and all four generated movies remain at
`/tmp/deadpan-hardware-decode-20261011`; `final/identity.json`, generated
fixture recipes and their hashes identify them. The final reports retain
every canonical pixel hash/PTS/duration and all 800 random target/anchor
comparisons. Reproduce with `compare_decode` and the recorded pinned prefix.
The generator's Homebrew FFmpeg is not the decoder library.

`initial/` includes the early prototype, its full-range normalization mismatch,
corrected native runs, failed initial example build, formatting correction and
all earlier timings. `probes/v2` and `probes/v3` preserve the corrected public
FFmpeg comparisons and software-layout observations. `sanitizers/` instruments
the native adapter, not FFmpeg or Apple frameworks. One agent implemented and
reviewed this change; the review notes record the callback-ownership correction.

No app default enables hardware decoding. Correctness passed; long-GOP
interactive latency failed. These reports do not close DP-16 or DP-24.
