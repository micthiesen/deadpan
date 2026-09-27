# Scoped sound mix preparation

This increment implements a borrowed preparation boundary for overlapping sound
voices. It does not implement persisted sound events, native sound placement,
creative voice treatments or the final master. No DP requirement or delivery gate
is promoted. [Sound events](../SOUND_EVENTS.md) records the complete required
ownership, structural editing and integration contract.

`AudioSignalMix` retains ordered tapes on one exact PointCeil clock, independently
applies each voice's source policy and scoped gates, and intersects known-silent
ranges across voices. An absent or exhausted voice cannot erase another voice.
Opaque processed output remains opaque so source endpoints do not truncate its
decay. Mixed `AudioStageProjection` input enters one canonical Preserve while its
output policy remains independent. Summation uses f64 and one finite f32
conversion without clipping or normalization. The result is explicitly labeled
as preparation before creative effects and mastering.

Real PCM tests decode the existing 44.1 kHz mono and 48 kHz stereo fixtures. Their
reference chooses exact source phase and physical support independently of the
plan, uses the shared qualified resampler, and performs a separate scalar sum.
A 37-sample onset and fractional-frame gates exercise arbitrary sample placement.
Shuffled and cold reads compare exact PCM, including peaks above unity. Canonical
DSP references distinguish mixing before one Preserve from separately stretching
voices, and retain the mixed stage's decay. Nested reuse rechecks source admission;
fully gated histories still reject unsupported processing before source I/O.

The first focused PCM run passed two cases and failed four. The independent
reference had used the selected mono endpoint of 4,410 samples despite its
37-sample onset within a 4,800-sample host. The physical endpoint is
`ceil((4800 - 37) * 147 / 160) = 4377`. Correcting that expectation preserved
exact assertions and left one actual code failure: the mix omitted explicit
`OutsideSourcePlacement` silence from its known-silent ranges. The fix includes
raw silence leaves without changing post-Preserve policy or flattening stages.

Independent reviews covered general correctness, exact timing/scope/silence,
and resource/dependency accounting. The resource review found two additional
gaps: simultaneous mix, voice-output and per-span buffers need four stereo f32
frame equivalents, and the shared 1,024-asset cap needed static preflight before
provider access. Both were corrected. Tests cover three-frame rejection and
four-frame admission, over-limit ordinary/Bound/projected histories before any
source access, same-renderer recovery, and exact-cap admission with duplicate and
fully gated voices. Dynamic source fingerprint checks remain separate.

A subsequent specification check corrected an overstatement in the proposed
integration contract. Section 10.2 applies time/pitch processing per voice before
the group bus; the existing sequential Original's continuous history does not
require an external sound to share its nonlinear processor. Default attached
voices need separate continuous processing and scoped output gates before
mixing. The new mixed-input Preserve remains a useful explicit aggregate
operation, not the default authored sound policy. A scalar bus mask cannot
selectively remove the Original's decay while preserving an allowed effect.
The code has no authored event integration, so this correction changes the
documented integration rule without changing stored projects or current playback.

The initial plan subset passed 49 tests. The broader plan/audio run passed 430
with none failed or ignored before the resource corrections. All nine final
focused PCM/resource cases then passed. The final gate passed formatting, strict
workspace Clippy, the workspace build, CLI doctor, strict harness-feature Clippy
and all 232 app/harness tests. Source hashes were unchanged through the gate.

The workspace test command passed 1,715 tests and failed four, with none ignored.
The existing Unix-socket admission test failed at socket creation with
`Operation not permitted` in this sandbox. Three playback tests timed out while
waiting for a fake device to start. Repeating the identical playback binary with
default test concurrency passed 13 and failed those three plus the injected
worker-panic test. Each of these four cases then passed alone without source
changes. The PCM cases took 12.58, 21.99 and 12.91 seconds overall; the panic case
took 0.14 seconds. These are complete test durations, not measured wait durations.
The same unchanged binary then passed all 17 playback tests with
`--test-threads=1` in 104.78 seconds, with none failed or ignored.

Review found no lost notification in the mutex/condition-variable handoffs and
no call from these playback requests into the new mix reader. The test helper
waits ten seconds for device start without reporting an earlier preparation
failure, while production preparation permits up to sixty seconds per batch.
The failures are concurrency-sensitive test timeouts with an unresolved precise
cause. No timeout assertion was weakened, and diagnostic retries do not replace
the failed default workspace gate.

Core schema 28 and database schema 34 are unchanged. Validation uses Rust 1.97.1
on macOS 26.5.2, arm64, with the pinned FFmpeg prefix. This increment changes no
native controls, window lifecycle or playback device path, so no live GUI,
listening or performance run is claimed. The existing Metal replay limitation and
unverified painted interactions remain open. The contributed UI harness and all
ImageGen boards and exact prompts are preserved.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-27-scoped-mix/README.md)
keeps failure logs, source identities, review corrections and final checks.
Git metadata is read-only in this session, so no commit or push is claimed.
The full-project goal remains active.
