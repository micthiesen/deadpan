# Sound allowance evidence, 2026-09-27

See the [qualification](../../../../docs/qualification/sound-allowances-2026-09-27.md)
for scope, corrections, verification and acceptance limits.

- Command JSON records exact arguments, source manifests, base commit, start,
  duration and terminal exit. Logs retain failed development attempts as well
  as the successful checks. No quiet process was restarted or abandoned.
- Source manifests hash tracked and untracked build sources plus every file
  under `crates/` and `native/`, including migration fixtures. Documentation
  and evidence edits do not change that source identity.
- Compressed JSON reports preserve all replay checks, samples, frames and
  metadata losslessly. `summary.json` retains report identities, results,
  selected timing summaries and capture mappings.
- `manifest.json` hashes retained files except itself. Hashes identify bytes;
  they are not signatures or release acceptance.

The retained PNGs show the pause permission before grant at both window sizes,
the selected sound after grant, and a routed sound after rejected movement.
The summary maps each to its exact report, frame and capture filename. Other
report image paths refer to scratch outputs. Screenshot-quota warnings preserve
semantic frames and named checkpoints; performance replay does no readback.

Replay uses the production app, real project/media services and Metal with
private fixture projects. Playback delivery in UI scenarios is simulated.
Separate decoded-audio tests exercise the actual limited authored bus. Physical
keyboard/IME delivery, VoiceOver, display appearance, listening, full-size
performance and encoded export remain separate acceptance work.
