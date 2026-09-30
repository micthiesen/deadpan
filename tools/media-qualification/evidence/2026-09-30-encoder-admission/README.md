# Automatic encoder admission evidence

See the [qualification report](../../../../docs/qualification/encoder-admission-2026-09-30.md)
for scope, final results, corrected failures and remaining work.

- [Summary](summary.json): checks, counts and unrun checks.
- [Independent review](review.md): source findings and verified corrections.
- [Native files](native-files.tar.gz) and [inventory](native-files.json): all five
  successful synthetic probe movies and the four final AVFoundation PCM decodes. Every
  member was rehashed after archiving.
- [Native cases](native-cases-summary.json): final choices, picture counts,
  measured pixel errors and independently decoded exact audio-event positions.
- [Manifest](manifest.json): byte lengths and SHA-256 for every other file here.

The qualified product source inventory is `f8b70d4ff7a0673e537a4f1f8ea8f9781333749cdbbca02aed195638d64baa9b`. Per-command JSON
journals retain arguments, base commit, diff hash, source inventory, duration and
exit status. The canonical runner pins `DEADPAN_FFMPEG_PREFIX` to
`/tmp/deadpan-ui-ffmpeg/prefix`. Saved scripts use this run's scratch paths;
replace those paths when reproducing on another machine.

The final workspace run passed 2,384 tests in 170
result groups, with zero failed or ignored tests. The final native matrix passed
190 frames and 24 independently
decoded exact stereo event positions. All final cases used the 1024-packet cap.
The initial `ntsc.json` run predates that cap and is retained separately.

The initial compile failure and deliberately interrupted Clippy log remain here.
The 14x16, 16x16 and 64x64 probes failed at the decoder geometry guard with
confirmed cleanup. Their reports remain here; no failed movie was admitted.
See [geometry review](geometry-review.md). These rasters remain unqualified.
No helper binaries are committed. The reports retain their hashes; the host
record includes development library hashes as reproduction evidence, not a
product runtime fingerprint. These results do not establish durable automatic
Render, public controls or full-product acceptance.
