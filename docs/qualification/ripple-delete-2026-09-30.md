# Retained audio through ripple deletion, 2026-09-30

## Behavior

Native `dd`/`:delete` and CLI `delete` resolve to one `DeleteRipple { node, timing }`
transaction. The command captures suffix sample entries before removing a complete
ordinary Sequence child. It preserves previous sampling lattices/resumes, nested
Sequence siblings, compact Repeat/gap entries and full Preserve preparation.
Empty and terminal deletions create no unused clock. The root sound bus receives
one deletion transform; removed Hold allowances and marks follow their policies.

Core 34/database 43 stay unchanged. The old core `Delete` reduction is retained
for exact history replay. CLI normalization occurs before both preview and commit,
and stores the new command. Mixed histories reopen and validate without migration.
Range and role-only deletion, temporal Repeat/Retime occurrence edits, full
mastering, listening, performance and release qualification remain required.

## Reproduced defect

The fixture contains a one-frame silent prefix and four frames of genuine
44.1 kHz mono PCM at 30000/1001 project fps. At 48 kHz, the old suffix is
`[1602,8008)` and the new one is `[0,6406)`, both 6,406 samples. Its old entry has
a 2/5-mix-sample phase, equivalent to 147/400 of one source sample.

Executing historical `Command::Delete` changed 6,405 of 6,406 samples beyond
2e-6, with maximum absolute difference 0.779741. The first sample changed from
−0.8758008 to −0.75776315. The original entry agrees with an independent scalar
recipe; whole-suffix comparisons retain the actual source endpoint behavior.
The corrected command preserves every retained PCM sample exactly, including
a cold read of the last 256 samples and reversed irregular reads.

The initial test scaffold exceeded the 256-sample read bound, then used an
independent recipe beyond its correct terminal support. Both failed attempts
are retained and excluded from the defect claim. The final witness uses bounded
reads, an independent first-128-sample entry oracle and the full original suffix.
The retained baseline report identifies the old command and fixture, not a
whole-tree pre-edit source snapshot.

## Verification

The locked workspace passes 2,661 tests, with none failed or ignored. The
optional UI-feature app run passes 375 tests. Workspace formatting and strict
Clippy across every workspace target with the UI harness feature pass.
The complete core run passes 564 tests. The first composite-audio run passes
25 tests, including seven deletion cases: original Source phase, historical
replay semantics, nested and outer siblings, room-tone looping, repeated deletes
after insertion, Repeat plays/gaps and full Preserve crop history.
An eighth deletion test independently constructs 6,407 Source input points and
8,008 Preserve output points at the authored 4/5 rate. It verifies the original
fractional output entry and exact preservation of all 8,008 moved samples.
All 26 composite-audio tests pass together in the final workspace run.

Independent core review and independent native/store/CLI review found no
actionable issues. The latter checked capture before allocation, shared revision
identity, durable receipts before refresh, exact mixed-history replay, stale
rejection and normalization before preview/dispatch.

No keys, focus behavior or layout changed. Native project-service tests exercise
the command owner; no live window, acoustic or VoiceOver check is claimed.
The user's separately positioned Deadpan window was untouched.

## Evidence

[Retained reports](../../tools/media-qualification/evidence/2026-09-30-ripple-delete/)
include commands, source hashes, failure logs, baseline source and review.
The actual fixture is `pcm-mono-44100.wav`, SHA-256
`b95b3debd2c83cf2590fddfaa98276c23c3b383b720fd96ac52c2c00c668318b`.
Checks use Rust 1.97.1 and the pinned LGPL FFmpeg prefix on Apple M5 Max,
128 GiB, macOS 26.5.2 (25F84).
