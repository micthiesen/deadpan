# Encoder runtime binding evidence

See the [qualification report](../../../../docs/qualification/encoder-runtime-2026-09-30.md)
for the contract, native results, retained failures and proof limits.

- [Summary](summary.json): workspace checks and remaining work.
- [Review](review.md): independent source review and bounded follow-ups.
- [Bound project](bound-project-final.json): actual probes, exact runtime and
  controls, committed input, candidate and complete-file verification.
- [Independent readers](independent-full-file.json): all 128 pictures,
  384 complete planes and 205,005 audio sample frames through each of three readers.
- [Initial comparison failure](independent-initial-failure.json) and
  [five-second fixture edits](five-second-oracles/edits.json): the original
  four-second capacity, explicit scratch-only extension and unchanged tolerances.
- [Native files](native-files.tar.gz) and [inventory](native-files.json): exact
  movies, references, decoded bytes and a consistent synthetic project backup.
  Every archive member was rehashed. No helper/library binaries are committed.
- [Database check](database-final.json): all cells in 19 tables unchanged.
- [Geometry diagnostics](geometry-summary.json): three unsupported tiny rasters.
- [Host](host.json): platform and exact runtime backing-file matches.

The qualified source inventory is
`46892abe2157dbb43b41cf84889b25cde5eb840f029293fea063216776cea20f`.
Per-command JSON journals record
arguments, base commit, diff/source hashes, elapsed time and exit status. Scripts
retain the scratch paths used by this run; replace paths when reproducing. The
canonical runner pins `DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-ui-ffmpeg/prefix`.
Saved scripts and source snapshots have a `.txt` suffix. Remove that suffix in
scratch space before reproducing, preserving the directory layout, including
the five-second oracles and their original snapshots.

The full workspace run failed only the 21 tests sharing an outdated encoder
protocol fixture. Correcting that one Python fixture passed all 21 on the focused
rerun; production sources and other passing targets were preserved. Final coverage
is 2,397 passing tests in 171 groups, with no remaining failure or ignored test.
The original exit-101 log is retained.
Native smoke covers startup/shutdown.
These results do not establish public Render, durable automatic decisions or
full-product acceptance. [Manifest](manifest.json) hashes every other file here.
