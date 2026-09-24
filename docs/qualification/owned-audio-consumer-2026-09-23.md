# Owned audio binding consumer qualification, 2026-09-23

This increment builds on `ef9a1a153f480572e7f990de08b086aad428789f` and connects
[owned timing bindings](../OWNED_AUDIO_BINDINGS.md) to normal render plans and
StageAudio. Core 16/database 22 remain unchanged. The base commit's
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35961538155) passed.

## Behavior exercised

Pure capture visits owned default and override subtrees with compact stable
Repeat arguments and birth rules. Existing bindings remain unchanged. Nonunity
Preserve captures its output in the enclosing clock, then resets its input
scope at the selected origin. Tests include a billion plays, all-overridden
defaults, reordered overrides, nested clocks, collision/no-op behavior and
atomic rejection of unsupported nonempty Repeat gaps or excessive path storage.

The normal audio reader resolves current physical recipes on retained root or
PointCeil lattices, bypassing only their own binding. Actual decoded WAV tests
exercise moved and resumed speech, real Split parity, stable Repeat survivors
and newly born defaults, changed Silence/RoomTone policy, selected point origins,
hidden negative support, empty operands and nested canonical Preserve DSP.
Current Edit crops exclude pre-trim source filter taps without replacing the
retained sampling phase. Source-only SequenceAudio rejects bindings before media
access rather than silently reading a different signal.

Creative fade tests retain the two-sample envelope after a one-frame clip moves
to a one-sample allocation at 32000 fps. A 64-sample resume retains old fade
progress and endpoint exhaustion. Fades remain after time/pitch mapping, with
96-output-sample maximum width. Pure tests cover the retained virtual origin
through later moves/rate changes, signed ties-to-even parity, selected-origin
PointCeil and canonical births, fractional progress, zero/one-sample fade sizes,
current Hard/crop owners and request partitioning. An exhaustive scalar test
compares integer progress against the established sample-centered fade engine.

Current and retained policy are queried on every consuming grid. A zero-point
SilentHold still suppresses newly created Preserve output. A separate actual-WAV
counterexample proves a zero-point Source with no audio retains ordinary DSP
decay after timing capture. Endpoint masks remain on the physical sampling grid;
they do not become explicit output silence across a later nonunity Preserve.
The latter fixture matches independent canonical DSP and shuffled read blocks.

Tests cover dense 256-play policy queries, work exhaustion before provider access,
sequential query admission, complete source dependencies through cache/halo reads
and relative-depth admission for a cache warmed at a shallower entry point.
All query paths use one read's deadline, cancellation, preparation and residency
controls. A full transfer carrier must still fit an i64 sample length; wider
carriers fail explicitly, even though offsets use checked i128 arithmetic.

## Review and repository gate

The general review covered pure capture and the complete rendering change.
Cross-review covered policy grids, preparation budgets, source provenance and
cache depth. Review exposed and fixed the dense-policy inventory cap and the
empty-point decay mute. Integration review also corrected immediate aggregate
work charging and the fade/crop regressions above. Final reviews reported no
additional findings.

The full gate passed on macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1, with
the pinned compatible LGPL FFmpeg prefix: formatting, locked workspace/all-target
Clippy with warnings denied, locked workspace tests/build, CLI doctor and native
Metal initialization/shutdown. The suite reported **1,172 passed, zero
failed, zero ignored**. All 368 source and fixture
hashes stayed unchanged throughout the gate and before commit.

[Evidence](../../tools/audio-qualification/evidence/2026-09-23-bound-consumer/summary.json)
retains counts, [command results](../../tools/audio-qualification/evidence/2026-09-23-bound-consumer/report.json),
[source hashes](../../tools/audio-qualification/evidence/2026-09-23-bound-consumer/source-hashes.json),
six compressed logs and the gate script. Changed Markdown links, eight saved
design image/prompt records and five unchanged original spec archives passed
verification.

## Remaining product work

No editor command yet captures these bindings or inserts an arbitrary pause.
Repeat gap ownership, full raw-recipe/movement/crop lifecycle and atomic Hold
authoring remain open. Context schema 1 still rejects bindings. These tests
establish an engine consumer, not a usable pause-insertion workflow.

No GUI behavior changed. Prior single-Original design targets and native
aesthetic/keyboard reviews remain the visual evidence; this smoke test covers
startup/shutdown only. Application playback, listening, export, full creative
operations and every open DP requirement and gate remain incomplete.
