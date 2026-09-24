# Audio definition qualification, 2026-09-23

This increment builds on `7f3117bd2332da9c2536585d9aff6886c5df6db5` and adds
[authored definition reads](../AUDIO_DEFINITIONS.md). The selector chooses a Node
output or the actual Repeat default, including when no existing play uses it.
The result uses an explicitly scoped local-zero point grid. Core 15/database 21
and retained-context schema 1 remain unchanged.
The base commit's [CI run](https://github.com/micthiesen/deadpan/actions/runs/35953030109)
passed.

## Behavior exercised

Six plan tests exercise all-overridden defaults, missing/wrong-kind selectors,
foreign plan identity, local-zero NTSC point-ceil allocation, definition scope
through nested Preserve, distinct Node and RepeatDefault selectors, query limits
and compact billion-play traversal with relative stable occurrence identities.
Both ordinary and retained-context compilation keep the true default-child index.

Six audio integration tests use actual qualified WAV decoding and canonical DSP.
They distinguish unheard default audio from every audible override, compare
3,204 NTSC definition points with 3,203 final root samples, and verify nested
Preserve/RoomTone against independent sampling/stretch recipes. Explicit silence
remains distinct from absent Source audio. A double-Preserve fixture checks an
output silent interval that has no input-grid point. Cache admission counts
exercise separate project, enclosing-definition and direct-definition scopes.
Frozen contexts recheck media admission, including hot-cache access. Invalid
ranges, foreign handles, cancellation and preparation limits fail.
A retained definition also renders after its live source and stage are deleted,
then rejects revoked admission on a cached historical read.

The headless integration test registers qualified managed media, overrides every
Repeat play with silence, and reads the unheard default's actual original PCM
through both selector forms. It checks structured errors and unchanged history.
After deleting the current Repeat, the authenticated historical context still
provides its old default. The executable's `--revision` path also returns that
old PCM and revision identity, rejects a missing revision, and leaves the cursor
unchanged. This regression failed with `InvalidInput` before revision selection
was implemented. Existing missing/changed linked-original tests also
exercise definition reads and reject the altered bytes.

## Repository gate

The final gate passed on macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1, with
the pinned compatible LGPL FFmpeg prefix: formatting, locked workspace/all-target
Clippy with warnings denied, locked workspace tests/build, CLI doctor and native
Metal startup/shutdown. The suite reported **1,097 passed, zero failed, zero
ignored**, including 13 new tests. All 354 source and fixture hashes stayed
unchanged through the gate; no source changes followed it.

[Evidence](../../tools/audio-qualification/evidence/2026-09-23-definition/summary.json)
retains the counts, [command results](../../tools/audio-qualification/evidence/2026-09-23-definition/report.json),
[source hashes](../../tools/audio-qualification/evidence/2026-09-23-definition/source-hashes.json),
six compressed logs and the gate script. Changed Markdown links, eight saved
design image/prompt records and five unchanged original specification files
passed verification. An earlier gate was deliberately interrupted to add the
reviewed revision selector; its partial run is not final evidence.

## Review

Two fresh reviewers covered the complete change and the definition/occurrence
clock, policy and cache boundaries. One capability gap was applied: the executable
now accepts an exact historical revision, with a regression that invokes the
command after deleting the current Repeat. Follow-up review confirmed that the
requested revision supplies source admission and that inspection remains
read-only. No findings remain unapplied.

## Remaining work

These are recipe reads, not authored Repeat birth or Hold-insertion edits.
Binding tables, committed-context dependency closure, lexical Repeat arguments,
live ownership/phase transforms, policy replacement, shared preparation across
contexts and atomic insertion remain open. The new canonical definition grid
must not replace surviving occurrences' old root-clock continuity.

No GUI behavior changed. The existing saved single-Original ImageGen boards and
native aesthetics/keyboard reviews remain the visual evidence. Native smoke
checks only startup/shutdown. No DP requirement or delivery gate is complete.
