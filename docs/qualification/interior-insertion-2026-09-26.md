# Interior pause insertion qualification, 2026-09-26

Core 25/database 31 add one atomic pause inside a root Source or ordinary
Background/Freeze Hold, including supported transparent fragments, when a
composite suffix follows. The native `,h`/`:hold` service uses the same command.
The edit keeps the Original and Repeat identities and shifts later content by
exactly the authored Hold duration. This is an increment toward the full splice
contract, not a completed product requirement or release gate.

## Timing and history boundary

The command captures sampling lattices before its internal Split, then captures
current placements afterward under a second checked timing ordinal. The copied
right-hand node retains its old sampling identity. The existing composite
reducer applies chronological entry steps and stops at the first nonunity
Preserve output. Previously admitted inputs keep their prior reducers.

At 30000/1001 fps, a Source cut at frame 1 resumes old sample 1602 at new sample
3203 after a one-frame pause. The following Repeat resumes old sample 3203 at
new sample 4805. A later pause inside an already resumed fragment uses its
current clock, including the independently checked old sample 3204 case.

Frozen core 24 permits its old composite root seams but rejects these new
interiors during database-30 replay. Database 29 and earlier keep their stricter
physical-suffix gate. The authentic old-binary fixture contains 21 revisions,
10 edits and one pending redo, including gap branches and abandoned history.
Migration checks every snapshot and both transaction directions, preserving the
backup before promotion.

## Environment and evidence

macOS 26.5.2, build 25F84, arm64; Rust 1.97.1; pinned developer FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Cargo jobs were serialized in the shared checkout.
Git HEAD was `c03a5edde5f28d27074745eb15711cb28b1f2e50`; this increment builds on
the preceding uncommitted repaint-wait checkpoint. Git metadata is read-only in
this session, so no commit or push was possible.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-26-interior-insertion/README.md)
contains source hashes, the incremental patch, raw logs, review results and
public-command records. It identifies this increment independently of the larger
accumulated Git diff.

| Check | Result |
| --- | --- |
| Core pause command | 19 passed, including exact mark transforms, billion-play compactness, identity/ordinal exhaustion and exact inverse patches. |
| Decoded PCM | 7 passed: direct/framed/fragment Source cuts, current-clock reinsertion, RoomTone Holds/gaps, and nested FollowSpeed/Preserve against independent sample and canonical DSP oracles. |
| Store migration | 88 passed, including authentic DB30 replay, pending redo, forged Source/Hold interior histories and strict unknown-field refusal without promotion. |
| Actual old CLI | Core24/DB30 refused both new interior cases with `InvalidCommand`; database contents remained unchanged. |
| Picture plan | 29 passed, including exact VFR Source/Hold/fragment interiors, Repeat gaps and occurrence identities. |
| Native app service | 166 unit and 2 integration tests passed, including actual VFR import, captured framing, suffix pictures, reopen and durable undo/redo. |
| Optional app/harness feature | Strict all-target lint passed; 194 unit and 2 integration tests passed. |
| Corrected audio-binding suite | All 36 passed; strict core all-target lint and formatting also passed. |
| Required formatting, lint, build and doctor | Passed; doctor reports core 25/database 31. |
| Exact workspace test rerun | 896 passed, 1 failed, 0 ignored. Stopped at the sandbox-denied Unix socket test in `deadpan-jobs/tests/artifact.rs:200`. Later workspace targets/doctests were not reached by this invocation. |
| Public app headless path | 42 invocations, 10 never-reused revisions and 5 history entries. Reopened each command; two interior pauses and undo/redo retained exact authored intent. Repeat-interior refusal left history unchanged. |
| Visual editing replay | Failed before app construction: Metal `No adapter found` at `egui_kittest/src/wgpu.rs:77`. Zero editing checks, captures or timing samples. Kestrel audit passed 3,472 routing cases and 62 reservations with no conflicts. |
| Additional complete store/plan suites | 375 passed, 0 failed, 0 ignored across 32 test/doctest results, including the full migration suite. |

The first exact workspace test invocation passed 603 tests and failed one stale
assertion in `audio_bindings/gaps.rs`, then stopped. It expected the old seam-only
diagnostic for a Repeat interior. The test now asserts `InvalidCommand` and the
specific unsupported Source/Hold-boundary diagnostic; its positive seam case is
unchanged. A follow-up independent review found no weakened assertion or masked
Split failure. No production source changed after the first gate. The failed
run and the correction's separate source manifest are retained.

The remaining workspace failure is `UnixListener::bind` returning OS error 1,
`PermissionDenied`/`Operation not permitted`, before its socket-admission check.
It is the same sandbox boundary recorded by previous increments. It was neither
ignored nor reported as a passing test.

The visual replay and durable app run used binary SHA-256
`547b18a2a44d92ab54958f86fbbaeb8c943861024eb9a6a6bb6b2815113680e0`.
The visual failure precedes the changed scenario. Release timing was not repeated
after that missing-adapter failure; the existing performance failures remain
open. Native startup smoke was not repeated because startup and lifecycle code
did not change. No new aesthetic, physical-display or latency claim is made.

The old-binary refusal probe first failed a script assertion expecting exit 2;
the CLI correctly returned exit 1. The corrected probe and original attempt
are retained separately. This was not a project mutation or application failure.

Two independent read-only reviews covered general correctness/migration safety
and exact timing/media semantics. Neither reported a concrete finding. The
parent inspected the reducer and adapter diffs and the actual refusal evidence.

## Remaining work

Interior insertion in a Repeat, Sequence, authored Retime or generated Hold,
fractional nested cuts, occurrence isolation and Original-moment payloads remain
required. The native keyboard replay cannot establish physical presentation,
VoiceOver, IME, listening quality, mastered playback or export. Existing UI and
latency findings retain their prior status. No requirement or gate changes status.
