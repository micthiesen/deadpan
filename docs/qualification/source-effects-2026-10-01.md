# Source effect clocks, 2026-10-01

Physical Source growth now has explicit preservation operations for camera paths
and audio treatments. The implementation base is
`3df3724d91811457c2ac1772688308047453f50a`. See
[the contract](../SOURCE_EFFECT_CLOCKS.md) and
[retained evidence](../../tools/media-qualification/evidence/2026-10-01-source-effects/README.md).

## Behavior and coverage

Retained framing keeps the original positive duration and maps current local
coordinates through an exact offset. New context holds endpoint poses. Existing
OwnerOutput recipes still follow their current duration. Core tests cover all
curve shapes, fractional positions, composition, tail growth, invalid clocks,
overflow, serialization and reversible patches. Supported historical readers
reject the new clock vocabulary, including explicit defaults and escaped keys.

The indexed-picture witness grows a Source from 10 to 15 frames, retaining its
body behind a unity Partition. It compares exact source PTS, selected frame
identities, camera poses and live ancestor clocks, then splits the selected view
and verifies both pieces. This is pure plan evidence, not decoded/GPU output.
The Camera unit test verifies path adjustment, unchanged entry and explicit reset.

Gain translation moves envelope ranges, every segment endpoint and half-open
mute ranges by the prefix. Core arithmetic tests use independent expected values
for Step, Linear, Smoothstep and cubic curves. Composition, new factors and
checked overflow retain exact input state. The PCM witness combines translated
keys with fractional sample phase, an independent sample offset, retained/resumed
audio, ancestor gain and a separate root sound. It compares shuffled read blocks
and includes a missing-translation negative control and exact inverse restoration.

## Verification

Independent review found no actionable issues in the framing and gain drafts.
A second review found no blocking issue in the durable effect regression or
core 38/database 47 integration. The storage test checks exact pose/gain/mute
values, ordinary effect commits, serialized transactions, close/reopen, both
Undo/Redo steps and immutable historical reads.

All 3,046 workspace unit/integration tests and both compile-fail documentation
tests pass, with none failed or ignored. The locked Rust 1.97.1 run completed in
1643.02 seconds, including 17m 51s of compilation. Focused core, picture-plan,
decoded-PCM, storage and Camera checks also pass, as does workspace formatting.
The Camera check additionally ran with `ui-harness` enabled. Strict locked
workspace/all-target Clippy, including `deadpan-app/ui-harness`, passes with
warnings denied in 780.16 seconds. Root owned all execution.

The final workspace run, strict Clippy, formatting and focused storage/Camera checks share
source manifest `fb43d12fb6353e69e3f6fb2a1cdae2d02c8124e570a3f9c13514c191267a5051`.
The host was an Apple M5 Max with 128 GiB memory on macOS 26.5.2 (25F84).
The retained host report confirms no Deadpan process remained after verification.

The debug app link reports an `__eh_frame` size warning because the unwind section
exceeds the compact unwind table's limit. This is retained in the command log;
these checks make no exception-performance or release-binary claim.

## Limits

No native app was opened. This does not qualify native Trim, physical input,
GPU/display output, device playback, export or release packaging. Atomic
In/Out/Slip/Roll, exact linked editorial windows, media-handle admission and
mark/sound transforms remain required. No requirement or gate is completed.
