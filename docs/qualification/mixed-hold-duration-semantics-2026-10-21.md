# Mixed Repeat Hold duration semantics, 2026-10-21

Implemented and verified on this Mac. Completed checks and retained failures
are recorded below. No requirement or gate is closed.

## Authority and intended behavior

One Hold target retains at least one explicit All plays (Default) and one
explicit This play choice, together with an optional concrete presentation.
The selected Repeat remains a direct child of an ordinary Sequence scope.
Only duration setters and named duration calls use this authority. Generic
framing authority remains unchanged. Retime, Partition, several-play selections,
owned-gap ancestry and root or attached sound events retain explicit refusals.
The initial mixed family operates on Repeat bodies; existing all-Play owned-gap
behavior remains separate.

`SetScopedHoldDuration` must isolate only unowned Play branches through the
existing temporal isolation machinery, then execute ordinary `SetHoldDuration`
once on the mapped Hold. Default choices remain Default. Actual execution
proofs map the target and its presentation through every staged instruction.
An all-Play address reconstructed from a representative would change authoring
meaning and is not an acceptable implementation. Dormant authoring requires
positive compact liveness evidence and no invented visible play.

Existing raw duration behavior owns physical audio capture, mark transforms,
Tail clamping and accepted-provider shortening or fallback. Generation requests
and preparations must follow the same actual isolation proof. Extension beyond
accepted coverage prepares the mapped isolated Hold; shortening or extension
within coverage retains the accepted provider without a new preparation.

## Valid native old-code witnesses

The base is `bcbf8493cdf709a2321e616cb8b1b6b3edb3ddd0`, after the qualified
All-Default duration milestone. Only the two new tests and their registration
were installed. Both fail at the unchanged public `State::hold_parameter`
positive assertion: expected `Some("hold-duration 2f")`, actual `None`.
Exit 100, two failures, 0.352s, 1,207 filtered. The executed app test binary
SHA-256 is `c93dcf3a646945a7cabc9159a4aa32c7cb72624cbf07f70a75fa51998a03d638`.

Before that assertion, both tests prepare an actual qualified offset-bframes
Original, Freeze and RoomTone, validate the explicit mixed target and real
representative at Edit19, and retain strict generic framing refusal. Existing
scoped Rename preparation executes real isolation in an immutable copy, then
the label is restored and ordinary raw duration changes its mapped Hold.
This independent positive control proves exact node pools of nine/four,
45→46/47 frame totals, one/two affected uses, representative starts19/20 and
unchanged original and independently owned recipes. It does not establish the
new wrapper's atomic timing, mark or sound lifecycle.

The first selection incorrectly used `--lib` and ran zero tests. That setup
failure is retained separately; it is not a red behavioral witness. Selecting
the app binary tests produced the two intended failures above.

## Implementation and focused checks

The mixed authority is distinct from ordinary, all-Default and all-Play
contexts. `SetScopedHoldDuration` calculates exact temporal isolation pools,
uses the executed isolation proof to map both addresses, then invokes the
ordinary duration reducer once. A zero-copy invocation still requests its
explicit Isolation allocation. Saved duration instructions contain length
intent, without the captured Hold, play or presentation IDs.

Native capture validates the selected root, literal branches and presentation.
The committed receipt independently derives the full Compound proof, checks
the resulting semantic context and verifies both endpoint pictures. Inspector
painting reads cached ancestry hints; it does not validate the complete
document each frame. Navigation permanently revokes delayed continuation,
including an away/back roundtrip. Generic framing authority is unchanged.

All 15 focused tests pass in 23.217s, with 5,186 filtered. Eight core checks
cover both branch orders, sparse overrides, partial physical audio clocks,
marks, exact proof mapping, counted calls, compact million-play structure,
dormancy and atomic no-op/late/stale/forged failures. Four native checks cover
actual qualified Original, Freeze and RoomTone capture, invocation-relative
recording/calls, exact service history, stale navigation and remaining guards.
Store checks cover scoped generation requests through Undo/Redo/reopen and
accepted media through both branch orders, raw and Compound execution, and
shortening or extension. Extension beyond accepted coverage creates the mapped
preparation; shortening retains its accepted provider without a new birth.

The real-media focused check covers both branch orders with 3→5 and 3→1 Hold
lengths. It asserts the literal picture sequence and compares decoded Original
click windows against their old physical source origins, before verifying the
complete encoded output. Release measurements remain separate below.

## Retained setup and fixture corrections

The initial integrated check needed the public preparation export. Subsequent
test compilation corrected a borrowed revision clone, use of an already
validated isolation proof and the accepted artifact's sampling field. These
compile errors ran no behavioral tests and are not old-code witnesses.

The first executable focused run passed 13 of 15 tests. Its store fixture used
the Repeat orders' pre-insertion allocation IDs, although insertion assigns
the new revision to every inserted Repeat. The corrected helper reads each
stored first-play identity and asserts the normalized allocation. The fresh
focused run passes its request mapping, history and reopen assertions.

The real-media fixture expected five/four cloned nodes, omitting its two
Partition wrappers around the speech Sources. The retained typed tree proves
seven/six nodes. The corrected oracle checks that shape explicitly, then the
exact clone pools; target ancestry still contains no Retime or Partition.
All independent PCM, picture, scope and history assertions remain. A targeted
probe passed the corrected media case in 21.268s; the full focused rerun passes
it in 23.213s. Failed reports and the first runtime source/binary identities
are retained separately from final verification.

The first broad run passes all 3,431 tests in 142.694s, with three slow markers
and one Nextest captured-output leak marker. All 197 UI-feature tests pass in
15.090s, with two captured-output leak markers. The three marked tests use
in-memory model fixtures; isolated reruns and a process check are recorded
separately. The markers are not assertion failures and do not prove their cause.

Strict lint then reported the new inline context making `ProjectRequest::Macro`
688 bytes beside a 424-byte Render variant. Boxing only the optional mixed
context reduces request size without changing the public authority queries,
saved intent or runtime guards. An independent follow-up review finds no
behavior change. All 759 affected regressions pass in 70.648s; all 197
UI-feature checks pass in 15.255s. The former run has one further captured-output
marker. Three isolated native tests pass in 0.030s serial Nextest without
markers; the isolated audio case passes in 0.08s with captured output reaching
EOF before its 30s deadline. The executed audio binary is hashed. These
observations do not establish the cause of the concurrent-run markers.

UI-harness lint also rejects compiling the shared fixture twice as separate
modules. Both replays now import the one existing fixture module; only its two
helpers' visibility changes. Seed data, typed identities and assertions remain
unchanged. Strict changed-package checks pass, followed by all 197 UI-feature
tests in 15.112s without captured-output markers. The generated command
reference remains current, including recording guidance for mixed Hold lengths.

## Native release qualification

Both final `scoped-plays` replays pass all 1,222 scoped checks plus the Kestrel
audit: 7,631 visual frames and 7,740 quiet performance frames. The executed app
SHA-256 is `6db81e01a109f00ba0c620421e581e720fa1a6dd60cc83bdfe5e1c06ab7e9804`.
The frozen manifest covers all 54 changed Rust files. Metal runs on Apple M5
Max, Mac17,7, 128GiB, macOS26.5.2; Rust1.97.1 and the pinned FFmpeg prefix are
recorded in the environment receipt. Timing starts with AC attached and no
other Cargo, Rust compiler or Deadpan process. Thermal state and OS file cache
remain uncontrolled; physical display delivery is not measured.

| Quiet measurement | p99 | Maximum |
| --- | ---: | ---: |
| Input to state | 1.854ms | 2.453ms |
| Input to commit | 31.979ms | 82.202ms |
| Picture request to completed submission | 5.307ms | 10.496ms |

The parent inspected all six final 960×640 captures: both literal scope orders
at parameter entry and after isolation, recording/Call/direct dot, and dormant
authoring. Scope labels, separate clocks, action keys, duration controls and
footer remain readable. The dot checkpoint selects an authored Background
Hold, so its black picture is intentional. The dormant inspector explicitly
reports no active picture while retaining the previous image. The short
qualified RoomTone establishes provider use, not acoustic quality.

Retained diagnostics include the Room Tone action requiring inspector scrolling
before its full button is clickable at 960×640; the replay performs that step.
Both runs have 320 frames that settle only on a second layout retry. Visual
and performance report 53 and 51 runs of three or more consecutive retrying
frames, respectively, with distinct per-frame causes. Every painted frame
passes the settled-layout check, and no identical retry repeats for three
frames without new input. The visual trace exhausts only its intermediate
image allowance; named comparisons retain reserved capacity.

The first release replay reaches 885 checks, then its recording refusal witness
sends the nonexistent `silence` command. The parser rejects it before the
duration-only guard. Using canonical `hold-silence` preserves every immutable
state, draft, instruction and exact error assertion. Independent follow-up
review finds no other likely command, wait or history-oracle issue.

The second release replay passes all 1,145 checks it reaches, including the new
recording, Call, dot, delayed navigation and dormant cases. A later existing
checkpoint exceeds the 160-image quota. The scoped trace previously retained
158 images; its six new named comparisons need additional capacity. Only
`scoped-plays` receives a bounded 180-image allowance and 9,000-frame trace
capacity. Other scenarios retain their limits. No assertion, per-action wait
or timing limit changes. Both complete final replays pass with these bounds.
Failed reports, frozen source snapshots and executed binary hashes remain in
the evidence beside the final results.

## Retained release media

The release CLI test passes in 20.443s, with 49 filtered tests. Four real
`cfr-bframes.mp4` cases use 30000/1001 timing, both mixed branch orders and
3→5 or 3→1 Hold lengths. The exact literal layouts contain 64 or 56 pictures.
Every picture and all presented audio samples are checked, without uncovered
samples, verification flags or nonzero offsets. Eight contiguous signal
windows independently report verified zero offset.

| Mixed authoring | Hold change | Pictures | Presented samples | Minimum block SNR |
| --- | --- | ---: | ---: | ---: |
| Outer Play / inner Default | 3→5 | 64 | 102,502 | 29.924dB |
| Outer Play / inner Default | 3→1 | 56 | 89,690 | 32.589dB |
| Outer Default / inner Play | 3→5 | 64 | 102,502 | 29.924dB |
| Outer Default / inner Play | 3→1 | 56 | 89,690 | 32.589dB |

Total coverage is 240 pictures and 384,384 presented samples. Before encoding,
the independent PCM oracle compares the four surviving Original click windows
against their old physical origins at 20, 29, 38 and 47 frames plus 1,554 samples,
within 1e−6, with nonzero signal. Canonical tests separately exercise 44.1kHz
and partial retained bindings. Reference PCM is never event-aligned to conceal
encoder delay. Actual private output/package paths, file hashes, emitted-file
reports, decoded-window results and executed CLI/test/worker hashes are retained
in `encoded-result.json` and `encoded-identities.json`.

## Evidence and remaining scope

[Retained evidence](../../tools/media-qualification/evidence/2026-10-21-mixed-hold-duration-semantics/inventory.json)
contains the valid baseline, compile/setup failures, first executable test
failure, corrected focused and broad checks, independent review notes, failed
and final native reports, six inspected captures, source snapshots, executed
binary identities, environment and release media verification. The inventory
records sizes and SHA-256 for every retained file and is rehashed after collection.

Physical keyboards, IME, VoiceOver speech, listening and other hardware remain
owner-only verification under the project's existing local scope. Exact steps
are in [Requirements](../REQUIREMENTS.md#to-verify-owner). Several-play durations,
mixed gap timing, Repeat/Retime combinations and the remaining full-product
requirements remain open. DP-06, DP-07 and DP-24 stay partial.
