# Native Hold lengths inside fixed Retime, 2026-10-21

The native inspector now resizes an ordinary Hold inside fixed Edit Retime
windows through Enter, Change duration and `:hold-duration`. Recording, named
calls and dot retain length intent. The captured runtime address identifies the
actual Hold independently of the cursor; it never enters the saved program.
This path requires no Repeat ancestry, no Visual selection and no derived pause
clock in a containing stage. General Retime macro authority and the separate
This-play timing restrictions remain unchanged.

## Exact visibility and atomic execution

`ordinary_hold_duration_visibility` validates the ordinary Sequence parent and
its direct inspector root, then ascends the compact parent index. Each Sequence
adds exact preceding extents. Each Retime clips the half-open input interval
before applying its exact affine map. Only Sequences and fixed, unclocked Edit
Retimes may connect the Hold to the inspected root; its outer ancestry stays
ordinary Sequence navigation. Work never expands Repeat plays.

Project picture centers determine visibility: the sampled interval is
`[ceil(In - 1/2), ceil(Out - 1/2))`, clamped to project bounds. A center tied at
In belongs to the Hold; one tied at Out does not. Hidden and unsampled Holds
refuse before allocation. Each staged setter rechecks the same authority and
visibility after the underlying `SetHoldDuration`; a resize that loses support
refuses the entire plan. Continuation uses the first sampled visible picture,
rather than the potentially cropped intrinsic start.

The exact native fixture has output 20, input[0,20), and input children Lead 2,
RoomTone 8 and Tail 10. Enter starts at cursor 5 and continues at2 after resizing.
The cropped fixture has output 10 and input[3,18): the Hold's intrinsic start
maps to -2/3, exact visible bounds are [0,14/3), and sampled frames are [0,5).
A grow to 10 continues at 0 while the stage output and mapping stay fixed. A
shrink to 5 would leave input length 17 below the retained crop end 18 and refuses
without writes.

Both native fixtures are private portable copies of an actually qualified
Original. Public project commands construct the edit and an independent output
sound with +37 sample offset. The checks preserve that sound's recipe, retained
clock, Original identity and independent cursor. The fixture is not evidence
that its particular short RoomTone selection is audibly nonzero. The separate
nonzero master/export qualification supplies the PCM evidence.

The core planner grants this context only `SetHoldDuration` and nested named
calls. Repeated physical captures alone do not seal a Retime; derived pause
clocks do. No-op lengths, forged or conflicting contexts, wrong allocations,
unsupported later instructions, Repeat/Partition ancestry and stale captures
refuse without authored history or register changes. The host additionally
proves that both sampled endpoint pictures present the captured Hold, before
planning and after execution, because structural geometry alone does not prove
the evaluated provider or cutaway.

## Native recording and drawing

The duration-only recording guard admits length setters, named calls, dot,
command entry, save and cancel. Gain, Camera, RoomTone, Silence and other
unsupported edits refuse before opening an unrecordable draft. Read-only help
and diagnostics remain available. Enter already used the inspector's parameter
route; its new recording replay is coverage, not evidence of an old Enter bug.

Drawing uses the immutable inspector's cached projection, parent map, clock
ownership and Hold recipe. It performs a bounded ancestor walk and never calls
the authoritative whole-document query or constructs counterpart targets.
Command capture and commit still perform complete validation. The work-bound
test exercises the actual parameter hint repeatedly beside a compact million-
play Repeat, with exactly three selected ancestors, and rejects multi-play and
clocked contexts.

## Baseline, tests and review

Base `ed33550d` runs two valid positive native tests against the unchanged
availability API. Both fail at the intended availability assertion after actual
Original qualification, document validation, selected Hold framing and target
validation. Generic framing authority still refuses as required. The old
executable SHA-256 is
`1e754093aa71b4c5a4774dfeeaf83b784cbacd5079b86eadb07cb277881e1886`.
Both failures complete in 0.334 seconds, exit 100; 1,193 unrelated tests are
filtered. The original fixture import failure and its binary are retained
separately; its cause was not established by the captured output and it is not
counted as a regression witness.

All 15 focused tests pass in 0.694 seconds: eight pure core cases, two native
availability cases, two real-package service cases, two recording guards and
the cached-hint work bound. They cover all pitch policies, fractional and nested
crops, picture-center ties, atomic refusals, counted nested calls, durable
Undo/Redo/reopen, unchanged registers and independent sound eligibility.

Fresh general, clock/work-bound and verification reviewers reported no findings.
A separate follow-up review found no issues in recording draft guards, cached
hints or actual-row delayed-navigation replay. Reviewers ran no Cargo or native
checks; root owns the execution evidence below.

Retained test-only corrections include E0603 for a private fixture import,
E0502 for a nested test-map borrow and E0433 for an unqualified duration type.
The first service expectations confused transaction-step rows with the one
history/revision row: two calls containing two setters each correctly produce
four transaction steps. The assertion now checks that actual contract. No
production admission or oracle was weakened.


## Broader checks

The normal selection passes all 698 tests in 68.755 seconds across core, app,
CLI, store and audio; 4,039 unrelated tests are deliberately filtered. This
includes existing real-media ordinary, fixed-window, processing-input and
one-play Hold-duration exports, output-owned sounds and the complete nonlinear
scoped audio chain. The app with UI support passes all 190 selected tests
in 15.025 seconds, with 1,060 filtered. The command-reference check passes after
its deliberate rewrite step; that step's explicit rewrite failure is retained.
`cargo xtask check --since ed33550d --no-test` and strict all-target UI-feature
Clippy both pass, without suppressions.

The first release visual run reaches 640 scoped checks before a replay-only
mistake: it invokes dot immediately after Undo, which deliberately clears the
remembered edit. The second attempt reaches 636 checks but also refuses dot:
named Macro Run does not establish new parameter dot intent. The final oracle
verifies the named call, undoes it, then uses a successful direct setter before
moving to Lead and invoking dot. It proves that repeatable parameter before
invocation and checks both Hold recipes, fixed output and sound clocks. Neither
Undo nor named Run policy is changed. Both failed reports and exact executed
source/binary identities are retained.

Reviewing the first run's capture also exposed a misleading
All plays explanation on a no-Repeat Retime child. The inspector now says that
changes apply to this beat inside the Retime, with a painted-text replay check.
A fresh follow-up review reports no findings; it ran no runtime checks.

A further root audit found an implementation regression before commit: Camera
used the newly broadened continuation flag as semantic framing authority.
An ordinary duration capture could therefore force a nonrecording Zoom preset
into the duration-only planner and refuse it. Continuation and explicit framing
authority are now separate checks. Ordinary nested Zoom keeps its direct scoped
commit path, while duration capture still owns its delayed result. The final
replay uses a real qualified frozen Original picture and checks committed Zoom,
unchanged Hold length, fixed window and output sounds, then Undo.

After separating continuation and framing authority, the focused app selection
passes all 144 macro, scoped and Camera tests in 9.971 seconds, with 1,106 filtered.
Formatting and strict all-target UI-feature Clippy pass again. The final native
fixture freezes an actually measured Original picture for the selected Hold;
Lead and Tail keep Background providers. The guarded recording and duration
oracles still preserve the exact provider recipe.

## Release media continuity

The current release CLI reruns the complete nonzero Original/RoomTone and
independent 44.1kHz event master from the
[fractional-owner qualification](fractional-owner-support-2026-10-21.md).
The test passes in 3.332 seconds. All 11 pictures and all 17,618 presented samples
pass in two contiguous signal windows at measured offset 0, with 38 compared
blocks, minimum block SNR 31.051dB and minimum luma PSNR 55.579dB. The complete
independent master's maximum scalar error remains 3.006e-8 before/after the
interior resize, including nonzero sample 8236 and genuine exhaustion 12812.
This is a backend continuity check; the new native fixture's controls are
qualified separately.

Executed release CLI SHA-256:
`52327cf91a73ef83d5b225732434334203e447e6cf38774a98173b9cc36d3d12`.
Executed release test SHA-256:
`3ce2c4cbd062587da10f43c1907acb7629236d60c01526d4023bb7207e608857`.
The emitted 22,491-byte movie has SHA-256
`87a4ad19c5a85f3b9ebb7aaa952b766619707c05ea8cc7eabd26ea9335a5df9d`.


## Final native results and retained scope

Both final release replays pass all 958 scoped checks plus the live-source
Kestrel audit of 21,885,504 production routing cases, with no assertion failures
or timed-out measurements. Native keyboard/pointer widgets, SQLite service,
qualified media and Metal execute the actual duration, Zoom, recording, call,
different-Hold dot, Undo/Redo, cropped continuation, invalid shrink and delayed
away/back cases. The delayed recording case uses actual row selection, which
abandons the recording; pending keyboard navigation retains its existing guard.

The quiet performance run starts with no Cargo, rustc or other Deadpan process
and AC power. Input-to-state p99 is 1.454ms, input-to-commit maximum 86.001ms,
request-to-picture maximum 8.897ms and input-to-picture maximum 94.903ms.
The raster is the bounded qualification fixture, not a large-media or
10,000-node performance claim. Final executed app SHA-256:
`599a16a57b5006f8c6ec21b7e4e02b1f83b062593056bdf8fa790bcb79edfed5`.

Root inspected final 960×640 captures 139, 141, 142, 146 and 147: the Enter parameter,
real frozen picture and Zoom, active recording, crop-visible continuation and
explicit rejected shrink keep the relevant control and footer visible. Warnings
remain explicit: the existing RoomTone control needs scrolling at minimum size,
258 frames settle with a second retry, and separate-input retry sequences remain
reported. There are no ignored-retry or repeated-without-input failures. The
visual run reaches its intermediate screenshot allowance while retaining named
checkpoints. Scripted picker paths, synthetic analysis words, physical input/IME,
VoiceOver speech, actual display presentation and acoustic device output remain
outside this replay; the existing owner verification list covers those checks.

Evidence is retained in
[2026-10-21-native-fixed-retime-hold](../../tools/media-qualification/evidence/2026-10-21-native-fixed-retime-hold).
Its inventory records 133 files, 7,798,992 bytes before the inventory itself, and
all retained bytes are independently rehashed. Raw media, project packages and
executables stay in private scratch. Commands, source identities, executed
binary identities, exact baseline/failure sources, full compressed reports and
review outcomes are retained. Environment: Mac17,7 M5 Max, 128GiB,
macOS 26.5.2(25F84), Rust 1.97.1, locked dependencies and the qualified FFmpeg 8.0.3
prefix. No requirement or gate is newly complete. DP-06, DP-07 and DP-24 remain
partial; broader temporal scopes, selectors, mastering and full-workflow
qualification remain required work.
