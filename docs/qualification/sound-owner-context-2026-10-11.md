# Independent sound history, 2026-10-11

`AudioSoundOwnerContext` supplies checked full current input for a concrete
sound treatment owner. Exterior transparent crops do not discard the history
needed by a temporal filter. Sample positions remain signed and absolute;
current structural placement never adopts an Original's retained clock.

The first enclosing time/pitch processor retains its full allocation. Selections
inside it still apply. Scoped queries retain exact stable Repeat identities,
use indexed play lookup and exclude other owner occurrences. A billion-play
planner fixture resolves one selected occurrence in fewer than 1,000 work units;
this does not claim that its enormous Preserve input can be rendered.

The context keeps current Hold policy over its full support. Source recipe,
plan identity, scope, work, runs and sample bounds remain checked. The existing
audio reader admits sources again after a warm read. Ordinary project-delivery
queries keep their existing root bounds and behavior.

## Verification

All 958 plan/audio tests pass, including seven new focused tests. Formatting,
strict workspace Clippy with all targets and `ui-harness`, and whitespace checks
pass. The retained 48 kHz stereo WAV is decoded through the real media reader.
Checks cover hidden negative PCM, current silent Holds, nested Preserve,
selections inside Preserve, exact cold/split/batch parity and source revocation.
At 30000/1001 fps, planner checks retain fractional source phase and the sound's
exact endpoint independently of the enclosing rounded frame allocation.

The primary agent implemented and reviewed this change alone. Evidence records
the base revision, changed source and fixture hashes, executed test binaries,
commands and logs on this Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2
(25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix. The initial NTSC test
incorrectly expected a final sample past the declared sound endpoint; its
failure is retained and the corrected endpoint assertion passes without a
production-code change for that failure.

See [retained evidence](../../tools/audio-qualification/evidence/2026-10-11-sound-owner-context/).
Run its `check-evidence.py --source` to check retained files and current sources.

## Remaining integration

This is full input history for independent sounds. It does not enable mixed
group EQ, change the existing bus or alter any stored schema. The next graph
integration must preserve saved sound-clock journals, sum contributions before
shared nonlinear stages and apply per-voice final silence and tail permissions.
The explicit EQ-plus-independent-sound guards remain. No native workflow,
device, listening, export, latency or second-machine qualification is claimed
for this increment. DP-09/DP-16 and the full development goal remain incomplete.
