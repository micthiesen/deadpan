# Workspace layout evidence, 2026-09-27

See the [qualification](../../../../docs/qualification/workspace-layout-2026-09-27.md)
for findings, build identities, exact scope and limitations.

- `summary.json` retains each run's metadata, check outcomes, findings, skipped
  checks, timing summaries and screenshot provenance. Full assertion values and
  individual timing samples remain in the compressed reports.
- `*.json.gz` are losslessly compressed original reports, including failed
  development runs. Decompress with `gzip -dc <file>`; their uncompressed SHA-256
  and byte counts are in the summary. They retain per-frame input, semantic
  state, accessibility, paint and timing evidence. Their original screenshot
  paths refer to scratch; only the selected images below are retained here.
- `validation*.json`, `focused.json` and `*-command.json` record actual commands,
  exit status and elapsed time. Logs retain failures as well as successful
  checks. Commands ran serially with the qualified FFmpeg prefix.
- `app-code.patch` is the final app diff against the base commit recorded in
  `source-identity.json`. That file also hashes changed source, Cargo.lock and
  the toolchain. This is separate from report-time dirty checkout metadata.
- `manifest.json` hashes every retained file except itself. These hashes record
  identity, not an independent signature or claim of product qualification.

## Inspected captures

| File | Source run | State |
| --- | --- | --- |
| `baseline-workspace.png` | visual-01 | Before layout changes, initial edited Original |
| `workspace.png` | workspace-05 | Revised default workspace, Camera and pause visible |
| `workspace-minimum.png` | workspace-05 | 960×640, selected seventh beat and frame navigation |
| `original-minimum.png` | workspace-05 | 960×640, Original browsing and moment controls |
| `camera.png` | visual-03 | Camera interaction checkpoint |
| `original-moment.png` | visual-03 | Selected half-open Original moment |
| `retime-entry.png` | retime-05 | Exact duration and pitch hint before commit |
| `error-first-resize.png` | visual-03 | Long wrapped output error on first 960×640 resize paint |
| `sound-paused.png` | sound-playback-04 | Paused sound, sample clock and pinned Resume/Loop |
| `sound-first-resize.png` | sound-playback-04 | Same paused sound on first 960×640 resize paint |

The failed full `visual-03` run is not labeled successful: its sound scenario
failed, then passed after a production fix in `sound-playback-04`. The final
workspace and speed checks have their own executable identity. Composite
coverage does not mean all fourteen scenarios ran again in one final build.
Playback delivery is simulated. Real imports, commands, decoding and Metal paint
do not establish device audio, VoiceOver, IME, physical input or display latency.
