# Saved beat-sound input, 2026-10-11

The beat-sound preparer now consumes full `AudioSoundOwnerContext` history,
including saved move/copy journals. Historical owner contexts are resolved
through the existing checked scope alias; their complete source and nested
Preserve inputs remain independent of the Original. Routed envelopes query the
full current Hold-policy domain, including negative and beyond-project samples.

`StageAudio::read_beat_sound_owner_input` exposes that attachment input after
current gates, event edges and event gain. Its sum stays f64. It excludes
Original audio, root SoundEvents and all owner treatments. The underlying
preparer retains individual occurrences for the pending shared treatment graph.
Every source still requires current-revision admission, including warm reads.
Construction work, static preflight, deadline and PCM residency remain shared.

## Preserved routing boundary

Expanding processing history can move its rounded start without changing the
boundary that a saved move used. At 30000/1001 fps, a copied crop starts at
sample 6406, while its expanded owner starts at 4805. Using that expanded start
to transport the old history would translate PCM by 6407 samples.

An occurrence now retains its ordinary delivery start separately. An identity
route rounds this anchor on each chronological grid, while its complete input
allocation retains the hidden PCM. Routed envelopes use the same translation.
The route constructor validates a complete chain of identity Keeps; a Window
or edited map cannot opt into different seam rounding. Occurrence capture checks
the exact anchor, and projected/raw root captures reject substitute anchors.
No stored format or ordinary delivery route changes.

## Evidence

The final plan/audio suites pass 963 tests with none ignored. Formatting,
strict workspace Clippy with all targets and `ui-harness`, and whitespace checks
pass. Twelve focused owner-history checks cover:

- Decoded stereo WAV input through cold, partial and batch reads, nested
  Preserve, and selections inside the enclosing processor.
- Two attachment contributions with event gain retained in f64, while a muted
  owner remains outside this raw operand.
- Saved move/return/copy clocks at fractional frame rate, with a current-bus
  interior reference and complete-history comparisons before, at and after
  the crop boundaries. Negative input and history beyond delivery are exercised.
- Current silent-Hold gates and automatic transitions after a saved move,
  compared with uncropped decoded PCM.
- Foreign plans, cancellation, overflow, residency rejection before media,
  recovery after failure and source revocation after a warm read.
- A direct routing control: an unanchored route maps sample 2000 to old sample
  -1204, while the checked delivery anchor retains old sample -1203. Incorrect
  capture anchors and nonidentity route substitutions reject.

The first copied-crop diagnostic failed with a one-sample shift. Its log and
executed-binary/source hashes are retained. An initial fade oracle omitted the
automatic silent-Hold transition; its corrected expectation includes the
existing 96-sample fade, without changing production fade semantics. Initial
compile and unqualified-fixture failures are also retained separately.

The primary agent implemented and reviewed this change alone on an Apple M5
Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and the
qualified FFmpeg 8.0.3 prefix. Source, fixture and executed-test hashes, commands,
logs and review are in [the evidence directory](../../tools/audio-qualification/evidence/2026-10-11-beat-sound-owner-input/).
Its `check-evidence.py --source` verifies retained files and current sources.

## Remaining work

This is independent input preparation. Shared recursive group EQ, ordered
treated children, root events and final per-voice silence/tail permissions still
need integration. The f64 inspection sum cannot substitute for individual
contributions across shared nonlinear stages. Complete final-delivery and
neutral-EQ edge parity in that graph, along with native commands, persistent
filtered caching and performance qualification. The mixed-EQ guards remain.

No GUI, device, listening, encoded-export or interactive-latency result is
claimed for this increment. DP-09/DP-16 and the full product goal remain open.
