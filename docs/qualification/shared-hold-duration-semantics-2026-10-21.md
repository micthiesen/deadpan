# All plays Hold duration semantics, 2026-10-21

This slice exposes existing Hold length intent with All plays explicit at every
enclosing Repeat, including a positively proven dormant default. It does not
complete DP-06, DP-07, DP-24 or any delivery gate. Mixed Default/Play choices,
selected several-play timing, Retime ancestry on Repeat duration paths and the
remaining sound-owner timing routes remain open.

## Authority and behavior

`SemanticSharedHoldDurationContext` retains the inspected Repeat root, literal
`ScopedNodeTarget` and optional concrete `InstancePath`. It is invocation state;
saved `SetHoldDuration { length }` programs contain no node or play identities.
It is mutually exclusive with Original, Visual, single-play framing and the
ordinary fixed-Retime duration contexts. Only duration setters and named calls
are admitted. Ordinary `SetHoldDuration` performs each staged change with a
parameter revision and no cloned node or mark identities.

The query validates the document and literal Default branches, excludes Retime
and non-Sequence/non-Repeat ancestry, and preserves the selected stable
representative. A visible Hold continues at that representative's new start.
No presentation is accepted only when complete body-override coverage at an
enclosing Repeat proves the literal definition dormant. Gap overrides do not
establish dormancy. Dormant changes preserve rendered duration and the cursor.
The existing reducer retains physical old-origin audio bindings, marks, Tail
clamping, accepted-provider shortening and fallback behavior.

The native inspector reads its cached ancestry and projection for availability.
Capture and commit validate the full authority and actual endpoint pictures.
Enter, Change duration and command entry share one captured path; recording,
named calls and direct-edit dot preserve that path. Named Run and Undo retain
their previous dot policy. Scope navigation permanently revokes queued
continuation ownership, including navigation away and back. Duration authority
does not authorize Camera or broaden generic semantic framing.

## Valid old-code witnesses

The base is `8814cb424a0391301fb1f43767ab17f76fff8461`. Only two new native test
witnesses and their registration were present before implementation. Both pass
real Original preparation, document/target validation, all-Default admission
controls, strict generic framing refusal and raw setter positive controls before
requiring the unchanged public `State::hold_parameter` API to return
`hold-duration 2f`. Both fail there with `None`: exit 100, 0.295s, 1,204 filtered.
The executed app test binary SHA-256 is
`2ba73a3dae23440eb9565c8458d6997ec75bf2b07bc866f7360e96fc49f1ebaf`.

The live nested fixture has root length 45, two live uses of the shared 2-frame
Hold, an independent inner 4-frame Hold and independent outer 7-frame Hold.
Changing the shared length to 4 yields 49 frames and moves the captured second
default representative from 19 to 21. The dormant fixture overrides all three
outer bodies: output stays 39 frames, every picture stays identical after
revision normalization, and presentation remains absent. Both use an actual
qualified Original Freeze and qualified RoomTone, rather than blank Source
placeholders.

## Verification

On Mac17,7 with M5 Max and 128 GiB memory, macOS 26.5.2 (25F84), Rust/Cargo
1.97.1 and the qualified FFmpeg prefix, all 14 focused tests pass in 10.686s
(3,331 filtered). They include ten core cases, three native/service cases and
the real-media semantic preview/export case. All 712 broader tests pass in
69.750s (4,039 filtered), and all 193 UI-feature tests pass in 15.167s
(1,060 filtered). The generated command reference and strict changed-package
and all-target UI-feature lint pass.

Three bounded static reviews report no actionable findings. The first runtime
run exposed a test fixture identity collision: Repeat allocation `outer`
conflicted with the common planner helper's final revision. Seven core cases
failed before reaching their intended behavior; native and real-media cases
passed. The test-only correction namespaces Repeat allocations and strengthens
negative assertions with specific codes/messages and exact parameter-allocation
counts. The original source and failed log are retained. The reviewed correction
does not change production behavior. The command-reference regeneration's
deliberate rewrite failure is retained separately from its passing recheck.

The first release replay reached 111 scoped checks, then caught the new capture
branch selecting duration authority for an All plays Source. The edit still
refused, but lost the existing This play framing guidance. The corrected branch
requires an actual Hold; other node types retain the original framing guard.
All 147 subsequent macro/scoped/Camera tests pass in 10.324s (1,106 filtered),
with strict lint and a reviewed type distinction. The first executed binary,
source hashes and failed report are retained separately from the final run.

Subsequent replay failures exposed three harness assumptions: the last-picture
assertion needed to wait for asynchronous presentation, Redo needed the complete
immediately preceding document rather than an earlier equal-length document
with different retained timing history, and the refusal command is
`hold-silence`. The recording checkpoint also exposed misleading This play
instruction text, corrected to teach duration setters and named duration calls
only. The fifth run passed all 1,007 checks reached before exhausting the
6,000-frame trace. The expanded scoped scenario now has a bounded 7,000-frame
trace; other budgets, deadlines and assertions are unchanged. Every failed run
retains its executed binary identity, source snapshot and report.

Both final release replays pass all 1,069 scoped checks plus the Kestrel routing
audit: 6,742 visual frames and 6,674 quiet performance frames. The executed app
SHA-256 is `0811a373a12433e2c20abb06efb5b9c45a0c2899f7004c53e1626357e07bf47c`.
The parent inspected all four new 960×640 checkpoints. Scope labels, duration
entry, controls, separate clocks, focus and footer remain readable. The black
dot-target picture is its authored Background provider. The dormant inspector
explicitly reports no active picture and retains the preceding image.

The quiet run has input-to-state p99 1.619ms, input-to-commit p99 24.872ms
(maximum 99.774ms), and request-to-picture completion p99 4.016ms (maximum
9.888ms). These are replay measurements on this Mac, with no GPU readback in
the timing run; physical display latency is not measured. Reports retain the
960×640 RoomTone control scrolling requirement, 281 second-layout retries and
49/50 runs of independently caused consecutive retrying frames. The visual
intermediate screenshot allowance is exhausted, while named checkpoints and
semantic frames continue.

The retained release preview/export test passes in 10.246s (48 filtered).
The real 30000/1001 A/V fixture changes a shared three-frame Hold to five or
one frame across four nested uses. All 68/52 output pictures and all
108,909/83,283 presented audio samples verify in contiguous windows at measured
offset zero, with no uncovered samples or audio flags. Minimum block SNR is
36.81/38.43dB. The independent decoded-PCM oracle retains each old physical
origin (20, 29, 38 and 47 project frames plus 1,554 samples) within 1e-6.
The semantic planner's compound command is committed through the headless
command path; this does not claim that CLI macro requests expose the native
shared invocation context.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-21-shared-hold-duration-semantics)
includes failed and final reports, bounded source snapshots, exact executed
binary hashes, media/package identities, test logs, review notes, scripts and
the four inspected captures. `inventory.json` records each retained file's
size and SHA-256, rehashed after collection. Raw executables and temporary
project/media packages remain private scratch data.

## Limits

Native replay exercises scripted production routing, accessibility/layout state
and Metal submission on this Mac. It does not establish physical keyboard or
IME behavior, VoiceOver speech, acoustic quality or another hardware tier.
The native RoomTone provider witness alone does not prove an acoustically
nonzero interval. Real decoded Source PCM and complete encoded-output checks
are separate witnesses. Owner-only checks remain in
[Requirements](../REQUIREMENTS.md#to-verify-owner).
