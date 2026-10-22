# Explicit authored node selectors

Status: implemented and locally verified within the scope below. No requirement
or gate is newly complete. Runtime results remain limited to the recorded stage.

## Public old-code control

Base: bfb8b811d51c0f19448f8c23a5b5a1184e72636d. The new standalone public
integration test builds Root [Lead 3f, Group [Empty A, Source A 8f, Hold 2f,
Source B 8f, Empty B], Tail 3f] at 30000/1001. The requested invocation remains
at Root/cursor 23/selected Tail with an unrelated Lead Visual range.

Existing public APIs resolve Source B's actual Group slot and [13,21) range,
capture its complete subtree, asset, owned mark and literal part mapping, and
round-trip the closed payload. A separately valid Group SelectedBeat Yank
matches that capture. Existing Root SelectedBeat and Visual selectors choose
Tail and Lead, demonstrating that neither is the requested target.

Public DeleteRipple and semantic SelectedBeat Cut preserve exact inverse
patches, register aliases and Compound replay. Whole-child Cut requests zero
split identities. The empty siblings retain distinct authored IDs and payloads
despite zero duration; deleting either preserves the other, total duration and
an exact inverse. The original document and requested invocation stay unchanged.

Only the final positive strict SemanticProgram admission of
`{"type":"node","node":"source-b"}` fails with unknown variant `node`, in
0.009553s. The exact executed binary SHA-256 is
`850278378412d250f7d993c3067f39c76c6fe79d12ef6ecdccea7ae647a50340`.
All 1,971 installed Rust files and six supplemental inputs retain their
pre-run hashes. A direct Git-blob audit proves all 1,970 base Rust files
unchanged; only the standalone witness is new. This uses synthetic structural
metadata and proves no decoding, audio, UI or export behavior.

## Intended scope under qualification

A whole authored node is addressed by exact NodeId independently of cursor,
selected beat or time geometry. Resolution stays inside the captured ordinary
Sequence through ordinary Sequence ancestry. Capture and mutation use the
actual immediate parent; continuation retains the navigation scope. Empty
Sequences retain structural identity. Root, the scope itself, absent IDs,
historical audio-only owners, out-of-scope targets and paths through Repeat,
Retime or partition owners refuse without fallback.

Finished selection, recorded consuming operators and dot-repeat retain the
node ID rather than cached frames. Native selection requires Your edit and
Linked role. Selection-only execution must write neither authored history nor
registers. Existing per-operator limits remain authoritative. Native p/P uses
whole-child semantic replacement; the existing Place slice preview does not
yet represent this selection and refuses with p/P guidance.

## Review and checks so far

Independent general review reports no material findings. A native review's
initial empty-node paste finding was withdrawn after tracing the early semantic
replacement route; valid Node selections do not reach the ordinary time-range
fallback. Test review identified an ambiguous role refusal because the original
fixture lacked audio. The revised control gives the Source valid linked audio,
executes both role consumers successfully on ordinary time selections, then
checks the exact whole-identity refusal before any allocation. Follow-up review
reports no findings in that revision.

The first all-target check found one new test constructor supplying a program
instead of its required Arc. That fixture-only correction is retained. The
second affected-package all-target check with the UI harness enabled passes,
with unchanged source and input hashes. After the role-control improvement, all ten focused core tests
pass in 0.020s. The Store identity/no-write case and durable CLI case also pass.
Their exact binaries and frozen inputs are retained separately.

The first app command selected the library target, which contains none of the
native tests. Nextest returned a no-tests error; it is not a passing run. The
correct executable target then passes all 40 focused app cases. Generated
command documentation and the routing baseline retain their intended rewrite
failures. The latter needs a second generation because its completion filter
uses the previously compiled-in verb usage. The final diff changes only the
select usage and its completion text; shortcut paths are unchanged.

## Debug native workspace

The first replay passes 42 of 43 reported checks and stops at a no-write
assertion. The app correctly refuses Place slice with p/P guidance and retains
the Node selection. The test incorrectly compared its current post-Undo
document with an earlier revision identity. Three no-write assertions now
capture the exact current snapshot immediately before the attempted operation;
the Place slice check also compares the register bank. Independent follow-up
review finds no weakening of the checks.

The second replay passes all 84 workflow checks across 176 frames, plus the
Kestrel audit. It covers unrelated cursor/beat/Visual state, semantic recording,
named calls, dot-repeat, missing identity refusal, nested whole-child paste,
empty-node copy/replacement/cut, empty Repeat refusal and delayed command
context. Both 960x640 captures were inspected: label, exact ID, scope, range,
empty-node status, separate clocks, focus and keyboard hints remain readable.
The report retains its intermediate screenshot-budget warning, 24 second-retry
frames and six distinct-cause consecutive-retry runs. The ignored painted retry
and repeated identical retry guards pass. Source, inputs and executed binary
hashes remain unchanged during each run.

## Broader regression

The eight-thread broad run executes 4,549 cases: 4,545 pass and four fail
their existing 20-second import wait while reporting Retaining. The failures
are the two moved-linked-Original backup cases, the full-Original new-project
baseline case and unavailable-linked-Original insertion invalidation. Each
passes alone with the exact retained normal-app executable, in 0.14 to 0.36s.
The broad run remains failed; isolated success does not establish its cause.

The Retaining status spans both preparation on the import worker and retention
on the project writer. The cleared user busy flag and last published storage
status cannot identify a blocked thread. These tests fail before their
editing/relink steps and before Node resolution.
No timeout or runtime limit has been weakened. Source, supplemental inputs,
155 test executable identities and five helper identities remain unchanged
throughout the broad run.

A bounded diagnostic runs the same four exact cases concurrently in three
batches while the full UI suite runs. All 12 invocations pass in 0.510 to
1.152s with the retained executable hash unchanged. None reaches the three-second
stack-sampling threshold, so no samples are taken in that diagnostic.

The first full UI suite and its sampled repeat each pass 1,267 of 1,270 cases,
retaining the two backup failures and new-project failure. Manual samples of
the two backup processes locate the import thread in Foundation bookmark
creation, through CFBundleGetMainBundle and directory enumeration. The project
writer is parked waiting for work. The automatic sampler reaches its
eight-second deadline; its missing files are not successful samples.

A controlled location contrast then runs the same four cases concurrently
with each normal/UI executable, first at its original Cargo deps path and then
from a byte-identical compact copy. Both original-location batches fail all
four cases in 20.17 to 20.35s. Both compact-location batches pass all four in
0.29 to 0.56s. All eight original-location process samples locate the import
thread in bookmark creation, CFBundle main-bundle discovery and directory
enumeration. The deps directory reaches the bounded count of 100,000 entries;
each compact directory has one. The exact opened directory pathname is not
independently traced, but unchanged executable hashes and the controlled
location contrast establish the execution-location effect. The NativeNew
sample in this diagnostic identifies its Linked iteration specifically.

Qualification therefore uses a standard Nextest archive extracted into a
small scratch tree, with every selected case and test executable hash compared
to the original run. Existing 20-second waits and eight-thread concurrency
stay unchanged. The original failed runs remain evidence. No production
bookmark behavior is changed by this test-location correction.

Nextest reports all 1,270 relocated UI cases passing in 70.048s and all 4,549
relocated broad cases passing in 245.005s. The corresponding driver durations,
including post-run identity guards, are 70.655s and 249.997s. The broad run includes 1,224 normal-app
cases across its executable and headless targets. The selected case sets and
all 155 broad test executable hashes match the original inventory; all five
helper hashes also match. The final strict driver requires those helper
equalities, closing a minor evidence-hardening review finding. Both runs
preserve eight-thread concurrency and unchanged sources, inputs and binaries.
The original UI archive setup attempts fail before running tests because the
extraction directory was absent, /tmp canonicalizes to /private/tmp, and a
relative helper path was treated as absolute. Those logs remain retained;
none is counted as a passing run.

## Strict checks and release native workspace

The affected-package strict check, UI-feature all-target Clippy with warnings
denied, workspace format check and whitespace check pass. All 1,977 Rust files
and six supplemental inputs retain their frozen hashes.

Both release native modes pass 84 workflow checks plus Kestrel. The visual
replay has 211 frames and performance has 391; asynchronous settling changes
frame counts. The inspected 960x640 release captures retain the nested target's
label, exact ID, scope and 20f range independently of the selected enclosing
group. The empty-node capture explicitly shows 0f, [20,20), distinct identity,
correct retained picture, separate clocks and p/P replacement controls.

Visual mode retains the intermediate screenshot-budget warning. Each release
mode retains 24 second-retry frames and five distinct-cause consecutive-retry
runs; the hard ignored-paint and repeated-identical-retry checks pass. In the
quiet performance run, input-to-state p99 is 1.992ms across 79 samples, UI-frame
CPU p99 is 1.833ms across 391 frames, commit p99 is 83.432ms across nine samples,
and input-to-picture p99 is 92.519ms across 21 samples. These are local replay
measurements, not complete workflow or hardware-tier qualification. Executed
release app/helper identities, Kestrel source, power state and the absence of
competing Cargo/rustc/Deadpan processes are retained.

## Release media and portable owner fixture

The real-media recipe uses the qualified cfr-bframes Original at 30000/1001.
It starts with Original [12,42), splits ordinary children, inserts a 2f
background Hold and groups the target children. From an unrelated Root cursor
and selected Hold, one public named program grows the Hold to 3f, copies and
cuts the exact nested 4f child, then repeats the following exact 8f child twice.
The literal final picture oracle is Original [12,18), Background 3f,
Original [22,30) twice, then Original [30,42): 37f total. The staged capture
retains the actual Group parent, source asset, owned mark and part mapping.
Dry-run, durable closed registers, complete history replay and public Undo/Redo
remain part of the fixture.

The release test verifies all 37 emitted pictures and one complete 59,259-sample
canonical audio window. Offset is verified zero; flags, uncovered samples and
low-SNR blocks are zero. Minimum block SNR is 39.574159dB. Thresholds and the
verifier are unchanged. This compares canonical preview audio with encoded
audio; it does not supply an independent PCM oracle, listening or full-mastering
qualification.

The exact MP4 identity and deterministic nine-file project archive are retained.
The archive is safely extracted and every file rehashed; the retained release
CLI validates the extracted package and passes doctor. All release source,
supplemental-input, app/helper and media-test executable hashes remain unchanged
through their recorded runs. No earlier checkpoint's result substitutes for
these checks.

## Evidence audit

An independent audit recomputes the retained inventory, verifies all mapped
Rust/input archives and the owner fixture, and reconciles the baseline,
focused/broad/UI, strict, native and media receipts. It confirms the retained
original-location failures and the distinct Nextest/driver durations. No
unsafe archive member, executable, foreign lockfile or future-selector source
is included. The sanitized audit and inventory are retained with the evidence.

## Remaining scope

Discontiguous and transcript-search-result selectors remain open, as do wider
temporal occurrence families, partial sound owners and complete workflow,
mastering and renderer qualification. A whole Node selection grants no
occurrence-interior or role-only authority. Native Place slice continues to
refuse this selection explicitly; p/P supports whole-child replacement.
