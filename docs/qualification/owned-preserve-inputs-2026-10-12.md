# Owned Preserve input qualification, 2026-10-12

Core 55/store 83 persists complete current-tree Preserve operands through
`DerivedRetimeClock::preserve_input`. The
[contract](../OWNED_PRESERVE_INPUTS.md) describes the wire format, ownership,
bounds, compiler and admission limits. This increment prepares intrinsic PCM;
retained timeline output placement and atomic insertion remain open.

The primary implemented and self-reviewed alone, from base `4dd5affd`, with
Rust 1.97.1 and locked dependencies on the Apple M5 Max Mac. No live window,
browser, physical audio output or listening was used.

## Cases

Core validation covers contiguous positive partitions, unknown and duplicate
fields, per-input and aggregate run bounds, the ancestor-work bound, missing or
escaping providers, input/source extents, nonunity Preserve ownership, explicit
nested intrinsic references and exact Repeat default/override/gap branches.
Payload-only clock patches retain before/after values through serialization,
reopen and inverse application, and reject a stale payload.

The SQLite case creates and reopens an owned-input package, saves and restores
a named take, reopens again, undoes, reopens, redoes and reads the immutable
snapshots. Its first version incorrectly called `store.commit` with
RestoreSnapshot; the store correctly refused it with
`Takes("use the revision-bound named take entrypoint to restore")`. The corrected
test uses the public named-take interface. Core tests separately exercise
payload-only changes, since ordinary editing still refuses derived clocks.

Real PCM comes from the retained `pcm-stereo-48000.wav` fixture through the
qualified FFmpeg decoder. The nested input case joins current Source spans
of 1,024 and 2,048 samples, excluding a 768-frame inserted Hold at 48,000 fps.
The original inner recipe prepares 3,072 input samples into 2,048 output samples;
the outer prepares those 2,048 into 1,024. Their current allocations are 2,560
and 1,280, respectively. Every output sample matches independently specified
canonical stretch recipes across 127- and 256-sample reads. Changing the second
Source selection changes the result and still matches the independently
specified current-source oracle.

At 30000/1001 fps, a saved nested input with fractional effective extents matches
the original pre-insertion definition PCM exactly. Intrinsic output uses 3,204
PointCeil samples for two frames; it does not substitute the 3,203-sample root
RoundEven count. Both input runs share the original sample phase.

Two Repeat plays carry distinct stable paths through the inner projection and
its Source leaves. Their outer canonical processing matches the separately
prepared repeated input. An additional exact silence interval contains no
input-grid point but owns output sample 2,049; it remains explicitly suppressed
and zero after processing while neighboring output remains nonzero.

Reconstructing the same owned projection 80 times in one request reuses one
prepared block while rechecking source admission. A new plan with changed Source
selection and the same project/revision/stage descriptor produces a different
block. Foreign-plan handles are refused before preparation.

Self-review found fallback paths that needed explicit refusal: the ordinary
child-input handle exposed by a projected stage, and definition/flattened audio
walks when the effective picture slope becomes unity. Those paths now fail
instead of preparing or exposing the changed child as retained intrinsic output.
The regression case also checks structural root output and frozen-context refusal.
Independent occurrence voices also refuse projected hosts and ancestors, including
when all effective picture slopes are unity. Their retained intrinsic rates can
still be nonunity. The first broad regression build was interrupted before any
assertions ran to close this final path; final checks use the corrected source.

## Results and limits

All 2,941 core/plan/audio/playback/store tests pass, with none failed or ignored.
The earlier focused Preserve run passed 358 tests; the final independent-voice
guard also passed its focused test before the complete regression. Strict
all-target Clippy passes for core, plan, audio, playback, store, CLI and app with
warnings denied. Workspace formatting passes. Final verification sources stayed
unchanged throughout that run; the evidence identifies all 143 test executables.

The [evidence](../../tools/media-qualification/evidence/2026-10-12-owned-preserve-inputs/metadata.json)
retains command logs, the initial store-test setup failure, final source and
binary identities, fixture hash and hardware/toolchain identity. This is PCM
before effects, not a mastered or encoded export. Retained output clocks,
aggregate placement, effects/metadata lifecycle, native authoring and complete
Retime insertion remain required. No full gate, live-native, physical-audio,
performance or encoded-media qualification was run here. No requirement or gate
is newly complete.
