# Native sound placement evidence, 2026-09-27

See the [qualification](../../../../docs/qualification/native-sound-placement-2026-09-27.md)
for implementation, review corrections, final results and limits.

- `*-command.json` records exact commands, start times, terminal exits, durations,
  base commit and source manifests. Matching logs retain compile errors, failed
  development replays and successful checks.
- `source-*.json` hashes tracked and untracked Rust, native, Python and build
  sources at each invocation. Each command names its manifest. Documentation and
  evidence-only edits do not change that source identity.
- `*.json.gz` retain complete original replay reports losslessly. `summary.json`
  records their uncompressed identities, build metadata, checks, warnings,
  exclusions and timing summaries. Full samples remain in the compressed reports.
  Older passing reports cover their earlier
  source manifests, not the final compact layout.
- `manifest.json` hashes every retained file except itself. These are byte
  identities, not signatures or release acceptance.

The six PNGs are selected from the final complete visual replay. `summary.json`
maps each to its exact report, frame, source filename and verified state:

| Capture | State |
| --- | --- |
| `sound-workspace.png` | Separate catalog, placed-event selection and inspector at 1280×820. |
| `minimum-sound-workspace.png` | Stopped sound workspace and visible keys at 960×640. |
| `minimum-preparing.png` | Preparing transport after pointer Play at 960×640. |
| `minimum-playing.png` | Simulated Playing delivery with the retained sound inspector. |
| `minimum-sample-command.png` | Native command entry and exact sample hint at 960×640. |
| `routed-sound.png` | Existing timeline cuts retained after an explicitly rejected move. |

Other screenshot paths in reports refer to scratch files. The screenshot quota
warning means intermediate images reached the bounded allowance; semantic frames
and named checkpoint captures continue. Performance runs perform no screenshot
readback. Placement replay qualifies real media, uses durable store commands and
renders through Metal; its playback delivery is simulated and it does not start
audio output. Physical keyboard delivery, native IME, VoiceOver, display appearance,
listening and full-size performance remain separate work.
