# Footer evidence, 2026-09-27

See the [qualification](../../../../docs/qualification/footer-layout-2026-09-27.md)
for behavior, review, executed checks and remaining limits.

- `*.json.gz` retain original replay reports losslessly. `summary.json` records
  their original byte counts and SHA-256 digests, actual build metadata, checks,
  findings, exclusions, timing summaries and observed layout-pass counts.
- Logs include the failing shortcut regression, the async test race, lost text
  on immediate Escape, and their focused diagnostics and corrected results.
  `*-command.json` records exact serial commands, terminal exits and elapsed time
  where the runner recorded them. Earlier direct Cargo commands are in their logs.
- `source-identity.json` records final changed Rust source, lockfile and toolchain
  hashes against the base commit. Reports separately identify their own binary
  and checkout. The qualification distinguishes earlier and final binaries.
- `manifest.json` hashes retained files other than itself. These are byte
  identities, not signatures or claims of release acceptance.

| Capture | State |
| --- | --- |
| `queue-first-paint.png` | Eight explicit wraps in one batch, with the pending notice and correctly positioned footer. |
| `command-minimum.png` | Exact speed entry and its explanation at 960×640. |
| `original-hints-minimum.png` | First paint after pointer dismissal into Original; complete command/help hints. |
| `resize-text-escape.png` | Resize, final text and Escape in one native batch, with closed command mode on that frame. |
| `camera-entry.png` | Camera controls on mode entry. |
| `camera-cancellation.png` | First frame after cancelling the Camera draft. |

`summary.json` maps each retained image to its exact source report, step and
filename. The original reports' other screenshot paths refer to scratch.
Performance runs have no screenshot readback. Replays use actual store/media/GPU
paths, with explicitly controlled picker, delivery and playback boundaries; they
do not qualify physical keyboard delivery, IME, accessibility or device audio.
