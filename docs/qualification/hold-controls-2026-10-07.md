# AI Hold motion and guidance, 2026-10-07

Specification §12.5 controls now reach the native job, closed and open-project
CLI, stored request, worker prompt and provenance. Motion is Still, Subtle or
Moderate. Optional guidance is nonblank text, at most 512 UTF-8 bytes, without
control characters. It is model text only. Timing, pause audio and explicit
acceptance retain their existing contracts.

## Behavior

`:generate 2 motion=subtle text=Keep the hands still.` captures the selected
pause and revision when command entry opens. The inspector displays the
choices and opens an editable prefill. Bare Generate and Retry retain current
choices. Changed choices allocate a new request; earlier variants keep their
original provenance. Recorded choices survive failure, cancellation and
reopening. An unrecorded job retains its choices for the same Hold/revision
within that session, including a runtime setup failure.

The CLI uses `--motion` and `--instructions`; `--another` refuses control or
seed flags and retains the request's values. Live generation reports the
owner's captured controls, and `ai-variants` reports each request's controls.

The `deadpan-hold-2` prompt varies one motion sentence under a fixed camera,
identity/composition and no-speech/new-object/scene-change instruction. With
guidance, a final sentence gives these restrictions priority. This cannot
guarantee model obedience; generated pictures still require review. The
exact prompt goes to the pipeline and retained provenance. Actual Gemma
tokenization rejects more than 1,024 tokens, preserving upstream left padding
without truncation. No prompt enhancement model is used.

The adapter identity is `0.15.8+deadpan2`; schema-4 manifests declare supported
motion amounts and the instruction byte limit. Signed weight updates must
match the compiled capabilities. Model bytes and pack version remain
unchanged. Older accepted media still uses its retained masters.

## Verification

Apple M5 Max, macOS 26.5.2. Evidence is under
`/tmp/deadpan-resume-20261006`.

- `hold-controls-tests-2.log`: ten focused Rust tests passed in 1.330 seconds,
  including controls across failures/retries/reopen, distinct request identity
  after changing them, command-prefill round trips, bounds, and manifest and
  signed-update capability validation.
- `hold-controls-python.log`: 75 tests passed, covering strict optional wire
  fields, Unicode byte/control bounds, malformed guidance before model
  imports, motion prompt differences, conflicting guidance, literal text,
  exact padding and overlong-prompt refusal.
- `hold-prompt-tokenization-final.json`: twelve real pinned-tokenizer cases
  passed with all three motion amounts, no guidance, 512-byte ASCII,
  510-byte Unicode, and command-like literal text. The largest actual input
  was 244 of 1,024 tokens. Tokens and masks matched without loss.
- Independent review found and verified the fixes for unrecorded runtime
  failure retries and explicit fixed-prompt precedence. No findings remain.
- `gate-12.log` ran all 4,751 workspace tests: 4,750 passed and the sole
  assertion failure was the old command-usage snapshot. Regeneration changed
  only Generate usage, generated examples and refusal text. Its generator
  reads examples from the prior snapshot, so the intended usage change needed
  a second regeneration. Both generated diffs were reviewed. The repaired
  snapshot and reference passed in `gate-12-completion.log`, which also
  reran strict workspace/UI lint, all 1,030 UI-harness tests (232.829 seconds)
  and both doc tests.
  Ten workspace and two UI-harness qualifications remain explicitly skipped.
- Nextest reported output-pipe closure diagnostics on one store test, the
  first repaired routing-snapshot run, and one UI candidate test. Their
  assertions passed. Isolated reruns in `hold-controls-pipe-recheck.log` and
  `hold-controls-pipe-recheck-ui.log` passed without those diagnostics; these
  records do not claim the initial diagnostics were absent.
- `hold-controls-replays/summary.json`: generation, variants and comparison
  passed all 59 checks. They retained layout-retry diagnostics (3/9/4 frames
  needed a second retry; consecutive retrying frames had distinct causes).
  No failed layout check or other warning was reported. The final retry fix
  receives a separate generation replay below.
- `hold-controls-replays-final/summary.json`: the final generation replay
  passed all 17 checks in 102.2 seconds, including visible controls after an
  unavailable-runtime failure. It reported four second-retry frames and four
  runs of consecutive retrying frames with distinct causes, with no failed
  check. Binary SHA-256:
  `9a4f16fe96ff859a6efa9c52cf2d55af1d64ec81b2caa9d60c847d3699d4162a`.
  Its production-router audit passed 21,884,016 cases against 62 Kestrel
  reservations. `hold-controls-kestrel.json` separately confirms the current
  local `Shortcuts.swift` hash matches the audited fixture (`368c01df…`);
  these replays did not pass the live-source flag.
- `bundle-controls/Deadpan.app`: release build, 722.6 MiB, 74 Mach-O files,
  ad hoc signature. `bundle-controls-verify.log` passed every positive and
  negative check, including tampered worker rejection. App SHA-256:
  `d006af6e4a947e16e6ff9538b4ee43a88d3e111f97d6cc83197f17e689fa748a`.
  CLI SHA-256:
  `8beb090252ee2df695da96759cb31518ff040e0bf2bfbf7359a43daf326cdd65`.
- `controls-generation/summary.json`: the packaged real model reached Ready
  in 80.902 seconds with Subtle motion and `Keep the hands still.`. Both host
  and worker retained those controls, `deadpan-hold-2`, and the actual 75-token
  prompt. The fallback remained unchanged until explicit acceptance. The
  accepted candidate rendered a verified movie in 2.213 seconds. The same
  app rendered an older accepted artifact in 2.189 seconds without changing
  its authored document. Each process exited; no native window was opened.

The real run uses the `cfr-bframes.mp4` test fixture. It proves request delivery,
real model execution, acceptance and output validity; it does not establish
model obedience on the owner corpus. Automated motion/lighting/geometry
checks and the other tracked DP-12 requirements remain open.
