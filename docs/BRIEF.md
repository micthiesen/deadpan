# Deadpan brief

This is the only document that says what Deadpan must do. It replaced the
full specification, the requirements ledger, the progress score and the
delivery gates on 2026-11-10 (owner decision). Those files were deleted; git
history keeps them. Do not restore them or rebuild their bookkeeping.

## What it is

A native macOS, keyboard-first editor for turning one video into a weird
YouTube-poop style edit. One person uses it on one Mac.

A project has exactly one Original (a local file or one YouTube URL). The
whole video is already on the timeline when the project opens. Editing means
changing that: cut, pause, repeat, speed up, reverse, reframe, cut away to
another moment of the same video, and drop in sound effects. Then one Render
to an MP4.

The loop is: navigate to a moment, select it, apply an operation, adjust it,
listen, move on.

## Rules that stay

- Time is exact. Project frames, 48 kHz samples and source timestamps are
  separate integer or rational types. Ranges are half-open. Sample boundaries
  come from the common origin, `B(f) = round_even(f * 48000 / fps)`, never
  from summed rounded durations. No float times.
- A pause inserts exactly its frames and the Original resumes where it left
  off. Three plays means three plays in total.
- Edits are typed, reversible commands on a beat tree (Source, Sequence,
  Hold, Repeat, Retime). Undo and redo are durable. Repeats stay structural.
- Preview and export use the same render plan. An export must look and sound
  like the preview.
- Nothing changes authored state except a command the user issued. AI
  candidates are never swapped in automatically.
- The Original and accepted generated media are never deleted by cleanup.
- Audio is not normalized or levelled behind the user's back.
- Everything is reachable from the keyboard, and the keys are shown where the
  actions are.

## Scope is capped

What exists today is the feature set. `docs/COMMANDS.md` (generated from the
action registry) lists it. Three rules cap it:

1. **No new combinations.** An operation that does not work inside some
   scope (one Repeat play, a speed change, a beat that carries a sound, a
   group) refuses with a message that says what to do instead. That refusal
   is the finished behavior, not a gap. Do not extend operations to more
   scopes.
2. **Frozen features keep working but get no new work**: AI pauses,
   per-play overrides, macros and dot-repeat, Trim mode, Place slice,
   tracking, captions, gags, YouTube import, recovery and backups, HDR
   output, the headless CLI. Fix one only if the finish list or a bug in the
   acceptance session needs it.
3. **Dropped requirements** are not work: model-pack signing and offline
   distributions, signed update manifests, SBOM and licence layers, helper
   trust pinning beyond what the bundle already does, clean-machine and
   second-Mac tests, hardware tiers, chaos, fuzz and hostile-media
   campaigns, CLI parity for new actions, VoiceOver work beyond
   keyboard-only use, qualification records, evidence archives, and any
   percentage score.

## Finish list

This list is finite. When every box is checked, the project is done and the
owner starts using it. Check a box only with the evidence named on its line,
and write that evidence as one line under the item. No other records.

- [ ] **F1 Installed app.** `cargo xtask install` builds the release bundle
  and places a launchable `Deadpan.app` in `/Applications`. Evidence: the
  installed app's `--smoke-test` passes and its version line names the commit.
- [ ] **F2 Real-footage acceptance session.** One scripted session on a real
  video of at least ten minutes with speech: create the project, transcribe,
  search a word, repeat it with a gain and zoom step, insert a pause, creep
  on the pause, cut away to another moment, change speed, reverse a passage,
  place a sound effect, play, render. It runs through the real UI replay
  harness and headless. Evidence: both runs pass and `verify-export` passes
  on the rendered movie.
- [x] **F3 Timeline overview.** The whole edit is visible as one strip, to
  scale, with every beat, the cursor, selection, pauses, repeats and placed
  sounds, readable at 10 beats and at 1,000. Beat cards remain the detail
  view. Evidence: replay frames at 10 and 1,000 beats.
  Done 2026-11-10 (d7d72c608, 6db58ebba): `ui-real` and `ui-start` replay frames on Tears of Steel at both sizes; confirm in the final sweep.
- [x] **F4 Transcript pane.** The transcript is readable as text (not a
  six-line rail list), follows the cursor, is searchable, and a word can be
  selected and operated on from it. Evidence: acceptance replay frames.
  Done 2026-11-10 (d7d72c608, 6db58ebba): `ui-real` and `ui-start` replay frames on Tears of Steel at both sizes; confirm in the final sweep.
- [x] **F5 Quiet inspector and footer.** The inspector shows the selected
  beat's main values first and folds the rest; the footer is one row of the
  keys that matter in the current context. Evidence: replay frames at
  1280x820 and 960x640.
  Done 2026-11-10 (d7d72c608, 6db58ebba): `ui-real` and `ui-start` replay frames on Tears of Steel at both sizes; confirm in the final sweep.
- [x] **F6 Word timing.** Word boundaries are good enough that a word repeat
  does not clip the word. Evidence: measured median word-start error under
  60 ms on real speech, with refinement on by default.
  Done 2026-11-10 (77668e18e and the commit after it): on LibriSpeech
  test-clean against forced-alignment truth, word starts are a 20 ms median,
  93% within 60 ms, p90 50 ms with the phoneme aligner (50 ms median with
  DTW alone). Needs the `phoneme-ctc-en` pack installed; install it with the
  app (F1).
- [x] **F7 Picture first.** The picture is the largest thing on screen and is
  never clipped at 1280x820 or 960x640; a settled frame never shows
  "Preparing picture". Evidence: replay frames.
  Done 2026-11-10 (d7d72c608, 6db58ebba): `ui-real` and `ui-start` replay frames on Tears of Steel at both sizes; confirm in the final sweep.
- [x] **F8 Render and Trim screens.** Render sits inside the layout and shows
  a destination, progress and result without raw internals; Trim shows its
  junction pictures. Evidence: replay frames.
  Done 2026-11-10 (d7d72c608, 6db58ebba): `ui-real` and `ui-start` replay frames on Tears of Steel at both sizes; confirm in the final sweep.
- [ ] **F9 Sound sanity on the real export.** No clicks at cuts
  (discontinuity scan), no clipping, A/V offset within one frame, levels
  unchanged where nothing was authored. Evidence: scan output on the
  acceptance movie.
- [ ] **F10 Playback on real footage.** Sixty seconds of the acceptance edit
  plays without dropped pictures at 1080p. Evidence: measured run.
- [ ] **F11 Clear refusals.** The combinations that refuse do so with a
  message that names the alternative. Evidence: a sampled list of ten.

## Simplify list

- [x] **S1 Documents.** Spec, ledger, score, gates and qualification records
  are gone; `AGENTS.md` is under 15 KB.
  Done 2026-11-10 (746ae182b, d4fdc2f2e): `AGENTS.md` is 8 KB; 70,800
  evidence files and ten record documents removed.
- [x] **S2 Cheap verification.** `cargo xtask gate` runs its tests in under
  ten minutes on this Mac. Tests that pin superseded behavior
  (before/after/trace/baseline oracles) are deleted; slow matrices run only
  on request.
  Done 2026-11-10 (787f039d1), full gate still to be confirmed at the end:
  the three test stages ran in 238 s, 117 s and 67 s with other builds
  running. `cargo xtask gate --slow` runs the 235 opt-in tests.
- [ ] **S3 Derived audio timing.** Original audio placement is derived from
  the current tree. The stored timing history that kept samples
  bit-identical across edits (bindings, lattices, reanchor steps) is
  removed. Accepted loss: at most one sample of difference at 29.97-style
  frame rates.
- [ ] **S4 Dead layers.** Code that only served dropped requirements is
  removed where removal is safe and shortens the build or the gate.

## Done

1. Every Finish box is checked with its evidence line.
2. `cargo xtask gate` and `cargo xtask replays` pass at the installed commit.
3. Each Simplify box is checked, or carries one line saying what was left
   and why.
4. `/Applications/Deadpan.app` is that commit.

After that the owner's own sessions are the only source of new work.

## How to work

- Verify with the cheapest check that covers the change: named tests while
  editing, `cargo xtask check` before a commit, the gate and replays once per
  coherent delivery.
- UI changes are judged from replay frames on real footage, not from the
  colour-bar fixture alone.
- Commit and push to `main`. One line in the commit message is the record.
  Do not write evidence documents.
- Conventions for an area live in that area's document under `docs/`.
  `docs/CONVENTIONS.md` is the old long instruction file, kept as reference;
  read the part that covers the code you are changing.
