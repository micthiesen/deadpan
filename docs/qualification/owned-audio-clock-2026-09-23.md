# Owned audio clock qualification, 2026-09-23

This increment builds on `c52c788ed962148b7ede0c6630af503dcbdc3e97` and adds
[owned recipe evaluation](../OWNED_AUDIO_CLOCKS.md) in explicit signed root
placements. The selected current or historical plan supplies the raw recipe and
media contracts. Core 15, database 21 and retained-context schema 1 are unchanged.
The base commit's [CI run](https://github.com/micthiesen/deadpan/actions/runs/35955060189)
passed.

## Behavior exercised

Five new plan tests cover signed NTSC phase, exact support cropping, support-edge
provenance, noncoincident Hard exceptions, complete nested Preserve preparation,
definition scope, query budgets, strict JSON, invalid roots/ranges and overflow.
The complete plan suite reported 111 passing tests.

Five new audio integration tests decode the qualified WAV fixture and use the
canonical DSP. An independently calculated Source fixture places local support
`[1/3,5/3)` at root origin `-13/7` with scale `3/2`: root samples are
`[-2174,1030)`, admitted source taps are `[534,2670)`, the first sampling phase is
533.6 and each output step advances 2/3 source sample. All 3,204 output samples,
read in reverse block order, match the independent resampling oracle. This test
initially exposed missing envelope-edge ownership for explicit support; the fix
records automatic placement-support provenance without moving old Hard edges.

A real `SetRepeat` transaction replaces a silent gap with RoomTone beneath two
Preserve stages. Keeping the same root placement produces the changed raw sound
and policy, using the new revision's source identity. The inverse restores the
original document exactly. Additional fixtures preserve a silent Hold with no
input-grid point, verify placement-independent intrinsic cache reuse and revoked
source admission, and reject foreign handles, invalid ranges, cancellation and
preparation exhaustion. All 11 definition-audio tests passed.

The new CLI integration test uses qualified managed AAC from the MP4 fixture.
It reads from signed sample -1600, changes Source alignment through a committed
command, and verifies new PCM at the same placement. `--revision registered`
still returns the original PCM and identity. The executable rejects unavailable
definitions, nonphysical roots, missing revisions, invalid sample ranges,
malformed/nested-extra-field clock JSON, zero scale and files above 4 KiB.
Every inspection leaves history unchanged. Existing missing/corrupt linked-media
tests now also exercise placement reads. An initial WAV CLI fixture correctly
failed its unspecified layout admission; the final test uses qualified AAC
without weakening the host's layout rules.

## Repository gate and review

The full gate passed on macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1, using
the pinned compatible LGPL FFmpeg prefix: formatting, locked workspace/all-target
Clippy with warnings denied, locked workspace tests/build, CLI doctor and native
Metal initialization/shutdown. The suite reported **1,108 passed, zero failed,
zero ignored**, including 11 new tests. All 354 source and fixture hashes stayed
unchanged throughout the gate. No source edits followed it.

[Evidence](../../tools/audio-qualification/evidence/2026-09-23-owned-clock/summary.json)
retains counts, [command results](../../tools/audio-qualification/evidence/2026-09-23-owned-clock/report.json),
[source hashes](../../tools/audio-qualification/evidence/2026-09-23-owned-clock/source-hashes.json),
six compressed logs and the gate script. Changed Markdown links, eight saved
design image/prompt records and five unchanged original spec archives passed
verification.

Two independent reviewers covered the complete change and the exact clock,
support, policy and DSP boundaries. The only finding was an error-code wording
ambiguity in the new contract. It was corrected to distinguish decode-time
invalid geometry from unsupported definition kinds/support, and follow-up review
confirmed the correction. No findings remain open.

## Limits

This is a headless evaluation boundary. It does not persist placement bindings,
compose authored resume anchors, implement compact Repeat birth/survivor rules,
or insert a Hold. Those are still required. The preferred owned-tree design
avoids duplicating historical raw bodies; its complete lifecycle is not yet
implemented or qualified. Existing historical context inspection remains intact.

No GUI behavior changed. Saved single-Original design boards and earlier native
aesthetic/keyboard reviews remain the visual evidence; the smoke test qualifies
only startup/shutdown. Playback, listening, export and every open DP requirement
and delivery gate remain outside this increment's claims.
