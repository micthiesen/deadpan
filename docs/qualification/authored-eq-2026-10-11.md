# Stored and recursive Original EQ, 2026-10-11

EQ is now a typed, explicitly ordered `AudioTreatments` stage. Its recipe survives
gain, saturation, balance and owner-prefix changes. Semantic `set_audio` commands
commit or remove it through the existing revision-aware compound transaction,
replay and undo path. The inspector reports its bands and cuts; native EQ
authoring controls remain open.

## Shared processing

The authored bus chooses each outermost remaining EQ stage, evaluates its input
prefix recursively, filters it and applies its suffix. Complete time/pitch and
creative edges remain before treatments. Child and group intermediates remain
f64 until the final bus conversion. Captured domain queries resolve hidden child
owners; retained input keeps its consumer map. Explicit silence and exhaustion
gate output, while mute is evaluated on its declared side of EQ.

One preparation request owns its filtered tiles and dependencies. A cache node
retains parent context, owner definition/occurrence/gap/origin, affine clock and
stage boundary. A whole Repeat's advancing play is evaluated per query; it does
not allocate another EQ owner. Nodes, work, prepared frames, depth and resident
PCM remain bounded. Compiling coefficients spends fixed work; each finite tile
spends input/output frames rather than impersonating a canonical stretch stage.
No filtered tile cache persists across requests.

Effect-tail input uses the same authored processing, excluding owners shared
with the tail. Shared owners apply once to tail output. Tail recursion remains
guarded, and the EQ graph is scoped separately to each invocation.

## Evidence

The final-source core, plan and audio run passed 2,004 tests with none ignored.
Workspace Clippy with all targets, `ui-harness` and `-D warnings`, formatting
and whitespace checks passed. The release export below is a separate check.

Decoded-media regressions cover explicit stage order against the previously
qualified owner processor, independently assembled nested f64 processing,
non-tile-aligned crops with hidden history, single-sample/block parity, resumed
Preserve, silent Holds, PCM limits and cleanup. A 70-play test failed with
`Limit("EQ owner count")` before the clock-identity fix and passes afterwards.
A nonzero 300 ms delay tail compares against the effect processor fed with
EQ output; a shared parent EQ is applied exactly once.

Core tests check strict wire admission, stage-order uniqueness, recipe retention
through other edits, semantic replay and inverse restoration. Frozen context
schema 10 authenticates EQ and rejects it under older version declarations.
Independent sound events currently trigger a specific unsupported-combination
error before any source provider call. This temporary restriction is tested
rather than hidden.

The release CLI fixture creates a real project and commits +6 dB and -6 dB
middle-band EQ to two 1 kHz tone Holds using semantic commands. Independent
preview checks expect the corresponding -4 dBFS and -16 dBFS peaks. The public
Render and verify-export paths produce and check H.264/AAC with unchanged
picture provenance and exact output timing. The retained report records all
54 pictures, 82 audio blocks and two timing windows; both measured offsets are
zero. Existing encoder priming metadata and verification tolerances are unchanged.

The primary agent implemented and reviewed this change alone. This Mac is an
Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1,
with the retained qualified FFmpeg 8.0.3 prefix. Source and executed-binary hashes,
commands, logs, export report, emitted movie, project and review are retained in
[the evidence directory](../../tools/audio-qualification/evidence/2026-10-11-authored-eq/).
`check-evidence.py --source` verifies retained files, passing results and current
source hashes. Diagnostic failures have separate logs.

## Remaining scope

This is recursive Original processing. EQ with independent sound events still
rejects explicitly; full shared group processing, independently timed automation
and per-voice send/tail permission remain required. Native authoring, full nested
binding coverage, persistent filtered tiles and interactive latency remain open.
No device/listening or native EQ workflow qualification is claimed. The earlier
native FIR numerical and sanitizer evidence still applies; this increment does
not change that adapter or its coefficients. DP-09/DP-16 and the full development
goal remain incomplete.
