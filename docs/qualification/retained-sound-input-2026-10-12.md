# Retained sound input, 2026-10-12

Independent event effects need the sound's complete input before saved moves
and cuts. Filtering an already routed array would advance effect history through
new gaps and discard input removed from current delivery.

`StageAudio::read_retained_sound_input` now reads that input through a checked
`AudioRoutedRoot`. Its returned allocation and sample labels describe the original
recipe, independently of the route's output allocation and live project duration.
The reader handles raw root sources, projected inputs and complete structural
occurrences. Source resampling, nested Preserve preparation and captured input
policy retain their existing semantics. Current consuming Hold gates, route gaps,
event edges and gain are outside this boundary.

Ordinary routed sound reads use the same extracted preflight, dependency
admission and input-query implementation before copying samples to their current
labels. The new entrypoint admits the complete hidden processing recipe and live
qualified dependencies, rejects foreign plans and out-of-allocation reads, and
retains the existing 256-frame inspection limit and shared preparation budget.
Failures release transient reservations and cannot publish partial PCM.

This is an implemented preparation boundary for specification §17.4. It does not
authorize additional output time or complete independent event continuous sends.
Creative input envelopes, returns before delivery routing, native authoring and
their preview/export qualification remain agent work. DP-09, DP-16 and the full
goal remain open. No project format, DSP algorithm, UI or audio verification
tolerance changed.

## Verification

All 692 audio and playback unit, integration and documentation tests pass with
no failures or ignored tests. Four new tests cover the retained-input entrypoint;
existing silent-Hold, admission and revocation tests also exercise it. The full
suite includes existing trim/copy/move, sound-clock, owner-return and playback
regressions through the refactored delivery path.
Strict workspace/all-target Clippy with `ui-harness` passes.

The decoded source witness uses both 48 kHz stereo and 44.1 kHz mono at a
30,000/1,001 project rate. A trim shortens the root from six frames (9,610 samples)
to five (8,008 samples). Reads of removed early history and sample 8,100 still
match an independent exact-phase resampling oracle with the original seven-sample
offset. Two nested Preserve stages and repeated occurrences match independently
prepared complete PCM on cold, shuffled reads before two inserted route gaps.
A fully deleted projected route remains silent while its retained input stays
readable.

Other checks cover current silent-Hold separation, foreign plan identity, invalid
ranges, cancellation, hidden depth/stage/input/residency limits, source revocation
after cache preparation and recovery. The first compile exposed a private helper
left in the old module; moving it with its callers fixed that error. The failure
log is retained.

The primary agent implemented and reviewed the change alone, including all
source/test diffs and the shared routed-read path. No independent reviewer,
live GUI, physical device or listening evidence is claimed.

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-retained-sound-input)
retains the source patch, source/executable hashes, exact commands, compressed
logs and initial compile failure. `python3 check-evidence.py --source` verifies
current source; omit `--source` for historical evidence. Cargo validation ran
serially on Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84),
Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix. No source edits were made
during a validation sequence.
