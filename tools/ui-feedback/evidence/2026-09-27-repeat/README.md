# Repeat input evidence, 2026-09-27

See the [qualification](../../../../docs/qualification/repeat-input-2026-09-27.md)
for implementation, results, review findings and limits.

- `*.json.gz` are losslessly compressed original replay reports. Decompress
  with `gzip -dc <file>`. `summary.json` retains original byte counts/digests,
  build metadata, assertions, findings, exclusions and timing summaries.
  Original report screenshot paths refer to scratch; the selected images below
  are retained here.
- `*-command.json` records exact commands, terminal exits and elapsed time.
  Logs preserve failed compile/invalid-option attempts as well as passing checks.
  `visual-01.log` is the initial Cargo replay; its original invocation appears
  in the log. Commands ran serially using the qualified FFmpeg prefix.
- `source-identity.json` hashes final changed app sources, Cargo.lock and the
  toolchain against the recorded base commit. Reports separately identify the
  actual executable and checkout at their start. Intermediate binaries are not
  retained. Git contains the final source.
- `manifest.json` hashes all retained files except itself. These are identities,
  not independent signatures or claims of release qualification.

| Image | `rapid-input-03` source | State |
| --- | --- | --- |
| `queue-first-paint.png` | `rapid-input-010.png` | All eight wraps arrive in one batch; seven wait. Also retains the existing one-frame command-exit layout gap. |
| `queue-progress.png` | `rapid-input-011.png` | Next frame, queue progresses and the layout settles. Counts reflect painting before end-of-frame continuation dispatch. |
| `pointer-cancellation.png` | `rapid-input-060.png` | Original pointer navigation cancels two waiting wraps; submitted work may finish. |
| `overflow-minimum.png` | `rapid-input-068.png` | First 960×640 resize paint; Working, sixteen waiting, four rejected. |
| `overflow-default.png` | `rapid-input-069.png` | Same state at 1280×820. |
| `escape-cancellation.png` | `rapid-input-071.png` | Escape cancels sixteen waiting wraps and retains the explicit overflow result. |

The release replay includes the strengthened full-document undo assertion added
after `rapid-input-03` compiled. The visual captures establish appearance; release
runs have no screenshots/readback and establish separate performance intervals.
Tests use real store commits, decoding and Metal. Held UI delivery and injected
playback do not establish native input delivery, IME, VoiceOver or device audio.
