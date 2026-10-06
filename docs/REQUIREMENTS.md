# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the current implementation and measured evidence, not a reduced release scope.

The [adversarial suite](ADVERSARIAL.md) adds Gate G crash, chaos and
malicious-input testing on stable Rust: 33 seeded mutation targets cover source
container admission and codec records, isolated FFmpeg decoding and the real
conversion helper, every core JSON boundary with exact command inversion,
tampered packages, all framed worker protocols, the live-project socket,
yt-dlp metadata, model-pack archives and stored analysis evidence. They run
deterministically in the normal tests, and `cargo xtask chaos` runs
time-bounded and ASan/UBSan campaigns. The
[2026-10-05 record](qualification/adversarial-2026-10-05.md) ran 52.7 M cases
in 10 minutes and 38.0 M sanitized cases with no panic, abort, sanitizer report
or bound violation. Its one finding, register slot-row loss, rename or a
changed bank version validating undetected, is fixed by a register bank digest
(database schema 65) and pinned as a regression. A
10,000-beat, two-hour synthetic project passed its long-project stress budgets
in release. This advances DP-23 and Gate G; neither is complete.

[The bundled AI runtime and model pack manager](qualification/ai-runtime-2026-10-05.md)
put the private AI runtime inside `Deadpan.app` and install the 36.2 GB LTX
bridge pack through a verified, license-accepting manager with offline folder
and archive import. A scrubbed copy of the packaged app imported the pack
offline in 13.3 s and generated, accepted and rendered an AI pause, with
pixels identical to the development environment. The macOS 15 MLX build was
2.2 times slower, so AI pauses require macOS 26. This advances DP-12, DP-13,
DP-22 and DP-23; clean-machine tests remain open (Developer ID signing and
notarization are out of scope by owner decision, 2026-10-05).

[Copied sound clocks](OWNED_SOUND_VOICES.md#copying-retained-sound-clocks)
retain each attached sound's complete processing definition and chronological
placements across whole-beat/group copies, repeated pastes and recopies. Fresh
node and Repeat identities remain paired with their historical clocks. Capture
retains the source's final placement so paste cannot revive previously clipped
samples. Supported ordinary Sequence moves preserve those histories; current
Hold gates, edges and gain still apply. Register admission recaptures the named
immutable revision, while preview and commit recheck qualified source evidence.
This advances DP-04 and DP-09. Edits inside a surviving processing branch,
partial copies, allowances and native beat-sound placement remain open.
See the [qualification record](qualification/sound-slice-clocks-2026-10-03.md)
for review, retained evidence and limits. The full workspace passes 3,600 tests
with none failed or ignored. All 40 focused regressions, formatting and strict
workspace lint pass on the same unchanged source inventory.
No product requirement or release gate is complete.

[Occurrence sound preparation](OWNED_SOUND_VOICES.md) projects one independent
catalog recipe from a checked current beat occurrence through its enclosing
time maps. Each Preserve stage retains that voice's complete processing history;
current silent Holds gate the final root samples. Stable Repeat paths select
actual plays and overrides without expanding unrelated plays. A bounded window
query now finds all relevant current occurrences, including processed output
retained by an outer crop, and the PCM adapter sums their independent histories
under shared limits. Saved owner/local-ID recipes now use typed Set/Delete,
source admission, reversible history and whole-owner copies. The authored bus
applies per-occurrence processing, edges and gain before one common limiter.
This advances DP-04 and DP-09 groundwork. Complete temporal edit/partial-copy
transforms, allowances and native placement remain
required before `ib`/`ab` can include attached sounds. The
[single-occurrence record](qualification/owned-sound-voices-2026-10-03.md) and
[batch record](qualification/occurrence-batches-2026-10-03.md) retain review,
source inventories, checks and limits.
The [saved-sound record](qualification/saved-beat-sounds-2026-10-03.md)
tracks the persistence and authored-bus increment. The full workspace run
recorded 3,546 passes and four outdated format assertions. All four affected
targets pass after test-only corrections; final formatting and strict lint pass.
No requirement or gate is complete.

The borrowed [occurrence route](OWNED_SOUND_VOICES.md#retained-occurrence-sample-routing)
now transports a voice's old integral PCM labels through chronological edits,
retaining its complete nested Preserve history and intermediate clipping.
Current consuming gates, edges and gain remain outside this raw provider.
This provided preparation groundwork for the saved independent clocks above. The
[qualification record](qualification/routed-occurrence-pcm-2026-10-03.md)
records 756 passing planner/audio tests, formatting and strict workspace lint,
with fixture corrections and source inventories retained.

[Group objects](EDITED_SLICES.md#group-objects) add native `ig`/`ag` to
copy/cut/Repeat operators and typed Visual selection in ordinary Sequence
scopes. Registers, semantic recording and supported dot retain object intent;
Group and Original/Edited paste or Place replacement preserve exact structural
ownership. Scope receipts distinguish an inner edit from its surrounding
navigation. Empty contents stay explicit and can receive a paste. This advances
DP-05, DP-06 and DP-21. The
[qualification record](qualification/group-objects-2026-10-03.md) carries the
actual checks and limits. Beat objects `ib`/`ab` still need their owned temporal
attachment lifecycle; analysis/role/occurrence selectors and every full-product
requirement and release gate remain open or partial.

[Exact sibling selections](STRUCTURAL_SELECTIONS.md) add identity-based
capture, Group, Repeat and atomic deletion that retain empty children at both
endpoints. This provides the structural foundation for the group objects above
in DP-05, DP-06 and DP-21.
[Qualification](qualification/sibling-selections-2026-10-03.md) records
305 core, 90 store and 73 real-audio tests, 748 default and 784 optional app
tests, 846 rendered placement checks and strict lint at that checkpoint. The
later group-object implementation adds its keyboard, Visual and scope behavior;
beat-owned temporal attachments remain required.

[Named groups](GROUP_EDITING.md) adds exact beat/range grouping, neutral
Ungroup, captured names, recording and dot in ordinary Sequence scopes. Headless
Macro results report complete final mark changes. This advances DP-05, DP-06
and DP-21; framed/treated Ungroup, temporal occurrence editing, saved gag recipes
and full slice placement acceptance remain open.
[Qualification](qualification/group-editing-2026-10-03.md) records 747 default
and 783 optional app tests, 1,416 rendered checks, strict lint, exact PCM/GPU
comparisons and native register/macro/Undo verification. The full workspace run
had five new assertion failures; all three affected targets pass in full after
the documented test corrections. The native QA app exited cleanly.

Current [development formats](DEVELOPMENT_FORMATS.md) are database 58/core 46,
refusing earlier development packages before writes. Current recovery and
embedded audio-context codecs remain. Historical
evidence below describes its recorded revision, not current format admission.

[Scoped editing](SCOPED_EDITING.md) adds native navigation inside Repeat/Retime
contents and explicit All plays/This play gain, Camera and Hold audio editing.
Browsing is read-only; changed values isolate only selected shared ancestors
and retain mapped receipts. Cached projection preserves exact clips and sample
centers through Retime. This advances DP-05, DP-06, DP-08 and DP-21.
[Qualification](qualification/scoped-plays-2026-10-03.md) records the 3,780-test
workspace gate, final app checks in both configurations, 1,083 rendered workflow
checks, strict lint and native verification. Mark-only saves preserve the current
play and any navigation made while the save is pending.
Temporal occurrence editing, implicit gap recipes, complete
definition previews and occurrence macros remain open, along with every gate.

[Repeat selections](REPEAT_SELECTION.md) adds `r` with a motion, Visual `r`,
whole-beat `rr`, captured command targets, semantic recording and dot for Repeat
wraps. Range endpoints retain their composite contexts and one Undo restores the
whole edit. Safe count changes preserve surviving plays, gaps and suffix clocks;
independent root sounds move once. This advances DP-05, DP-06 and DP-21. Total-play
setters now share semantic recording and dot, retaining the requested count while
resolving a new explicit selected Repeat. Temporal occurrence edits, text/role selectors, the remaining
editing surface and all product gates remain open.
The [qualification](qualification/repeat-operator-2026-10-03.md) records 3,292
passing tests, 1,094 rendered workflow checks and native keyboard verification.
The later [count-setter qualification](qualification/repeat-count-2026-10-03.md)
records the 3,799-test workspace gate, 764 optional app tests, 1,043 rendered
checks and native record/run/Undo verification. These overlapping configurations
are reported separately. The QA app exited cleanly.

[Cut selector repeat](SEMANTIC_REPEAT.md) extends `.` to typed motion cuts,
whole beats and Visual cuts. Current Visual ranges override retained selectors;
empty ranges refuse, and saved Visual cuts require a new selection. The requested
count/direction and destination register survive retargeting. Versioned Apply
admission, exact retries and saved-cut proofs preserve intent through refresh
failures without reusing old coordinates. Recording dot retains its effective
instruction. This advances DP-06; remaining editing operations, semantic
text/role/occurrence selectors and all product gates remain open.
See [qualification and retained failures](qualification/selector-repeat-2026-10-03.md).

[Operator motions](SEMANTIC_MACROS.md) add Normal Edit `y`/`d` plus frame,
beat or group-boundary motions, with explicit `yy`/`dd` whole-beat selection.
The native router, CLI and macros share typed selectors and exact historical
Child/Range captures. Copy-only operations preserve history; counted macros
commit once. Prefixes retain their captured context, including absent targets,
and stale completion cannot reclaim cursor ownership. This advances DP-05,
DP-06 and DP-21. Text objects, analysis-dependent motions, broader
dot-repeat and all product gates remain open.
See [qualification and retained failures](qualification/operator-motions-2026-10-02.md).

[Visual macros](SEMANTIC_MACROS.md) extend recording and replay with oriented
Visual selections, frame/beat/group-boundary motion, range copy/cut and atomic
Original or Edited replacement. Programs can start with an existing selection
or construct a relative range. Empty selections refuse without beat fallback;
copy-only execution retains timeline history, and counted authored runs share
one Undo. Final selection follows the exact owned native receipt. This advances
DP-05, DP-06 and DP-21; full operator grammar, semantic text/role/occurrence
selectors, remaining edits and all product gates remain open.
See [qualification and retained failures](qualification/visual-macros-2026-10-02.md).

[Macro reuse](SEMANTIC_MACROS.md) adds selected-beat yank and before/after
register paste to recording and execution. Original and Edited copies use the
same staged planner in native and headless requests. Selection is independent
of the cursor, including empty siblings; copy-only runs save the bank without
changing timeline history. Counted authored runs retain one Undo. Original
mappings come from measured qualifications and are independently checked at
store admission. This extends DP-06 and DP-21; broader selectors, remaining
editing instructions and the complete product acceptance remain open.
See [qualification and retained failures](qualification/macro-reuse-2026-10-02.md).

[Headless Macro commands](SEMANTIC_MACROS.md#headless-inspection-save-and-run)
inspect consistent revision/bank snapshots, save named programs and run the
supported vocabulary from explicit Sequence coordinates. Both
closed and open-native projects share full dry-run admission and atomic commit.
Runtime copies prepare before save; exact bank-only receipts survive refresh
and reply-detail failures. Remote motion returns its position without moving
the GUI cursor. See [qualification and limits](qualification/headless-macros-2026-10-02.md).
This advances DP-06 and DP-21; neither requirement nor any gate is complete.

[Semantic macros](SEMANTIC_MACROS.md) add `q` recording and counted `@` calls
for relative frame motions, frame cuts and named programs. Schema 55 persists
typed Macro registers without changing the default copy or timeline history
when saving. Each authored run resolves sequentially and commits as one Undo;
late failure preserves both the document and bank. Native keyboard delivery,
record/save/run/Undo and exact persisted instructions were checked. The full
workspace run passed 3,633 tests; final app suites, strict lint and 823 rendered
workflow checks cover the final receipt and assertion updates. See
[qualification and limits](qualification/semantic-macros-2026-10-02.md).
Broader instructions, semantic selectors, occurrence contexts and full
layout/IME qualification remain DP-06 work.

The original [semantic frame-cut repeat](SEMANTIC_REPEAT.md) increment added `.` for the requested
count from the last committed `x` or `:delete-frames` operation, resolved at the
new current Edit cursor. A clamped first cut retains its original count. The
service observes actual saved revisions, preserves intent through marks and
Undo/Redo, and clears it after unsupported edits even when refresh fails.
Versioned feedback and requests reject stale repetition.
The selector extension above builds on that foundation. Other edit kinds remain open.
See [qualification and limits](qualification/semantic-repeat-2026-10-02.md).

[Resolved compound transactions](COMPOUND_TRANSACTIONS.md) stage ordinary edits
and copy/cut/paste register operations, then save one reversible history entry.
Per-step media admission prevents an invalid intermediate operation from hiding
behind a valid final document. Schema 54 retains capture checkpoints and every
leaf allocation, so an intermediate copy remains usable after Undo and reopen.
Bank-only programs preserve the edit history. This supplies a DP-06 execution
boundary; semantic text/role/occurrence selectors and repeat for other edits remain open.
See [qualification and limits](qualification/compound-transactions-2026-10-02.md).

Native sound edits, whole-Original insertion and first-Original initialization
now retain their saved revision before refreshing the workspace. A failed
refresh reports durable success and asks the user to reopen; stale views cannot
consume the receipt. Catalog registration retains its saved asset without
inventing an edit receipt. See [verification and limits](qualification/saved-receipts-2026-10-02.md).
This fixes a prerequisite for reliable semantic recording; broader dot-repeat
remains open. Macros now use the shared atomic Compound boundary.

[Project registers](NAMED_REGISTERS.md), introduced in SQLite schema 53, persist
a–z and default copies of Original moments and editable slices. Copies leave edit
history and Undo/Redo unchanged; cuts save deletion and registers atomically.
Reopening restores historical provenance under fresh runtime identities.
Versioned bank updates remain independent of stale UI confirmations, and new
placement waits for a pending save. Unique contents are bounded at 64 MiB.
Broader Macro instructions and dot-repeat for remaining editing operations are required for DP-06. The
[qualification](qualification/durable-registers-2026-10-02.md) records 3,534
distinct passing default tests across the full run and corrected targets,
3,896 rendered checks across 28 ordinary scenarios, and native save/reopen with
unchanged timeline history. No requirement or gate is complete.

The [compact Original layout](qualification/original-layout-2026-10-02.md)
returns picture space through a Sounds count heading, compact clocks and
measured transport rows. Focus and scale-change replays retain both cursors,
selection and copy while keeping complete controls visible. This advances
DP-05 and DP-20; full keyboard, physical-input, accessibility and visual
acceptance remain open. No schema changes or product gate completion.

The [editor keymap](KEYMAP.md) now loads optional Normal/Visual overrides from
macOS Application Support. Complete-map validation, logical or physical matching,
semantic labels and persistent fallback diagnostics share the production router.
Held custom motions retain their resolved action; ordered native text and IME
keep ownership after Command/Search entry. The remaining mode maps, strict
logical provenance, physical layout qualification, broader dot-repeat
and broader Macro instructions remain open. This does not complete DP-05, DP-06 or any gate.
The [qualification](qualification/configurable-bindings-2026-10-01.md) records
review, rendered replay and verification limits.

Native [frame cuts](EDITED_SLICES.md#frame-cuts-at-the-cursor) add `x`, counted
`12x` and captured `:delete-frames Nf` to DP-05's ordinary Sequence workflow.
One atomic cut updates history and the session copy, with the actual interval
reported after save. Group-end clipping is explicit; unsupported partial
composites and ineligible focus refuse without edits. The full grammar, nested
occurrence cuts and all product gates remain open or partial.
The [qualification](qualification/native-frame-cuts-2026-10-01.md) records
final app suites, three rendered regressions and the retained failing witnesses.

[Deadpan's identity assets](design/brand/README.md) now include a genuine layered
macOS icon, complete legacy fallback, logos and web formats. Native bare/bundled
startup and an AppKit icon readback pass; 15 logo PDFs render without clipping
and contain vector paths without embedded fonts or images. This is branding and
developer-bundle groundwork for DP-20. Signed, standalone distribution and the
full release gates remain open.

[Packaging](PACKAGING.md) adds `cargo xtask bundle`, which produces a
relocatable, hardened-runtime `Deadpan.app`. It contains release executables,
FFmpeg relocated to `Contents/Frameworks`, a pinned yt-dlp/Deno baseline the
running bundle prefers, generated notices, a CycloneDX SBOM and an `otool`
audit. `bundle-verify` passed on a scrubbed relocated copy with an ad hoc
signature, including tampered and deleted helper cases, and a real YouTube import also succeeded. This advances DP-22, DP-23
and Gate F. Developer ID signing and notarization are out of scope by owner decision
(2026-10-05); clean-machine acceptance remains open.

[Native saved-render recovery](qualification/render-history-2026-09-30.md)
adds bounded keyboard browsing through `Renders` and `:renders`. Saved jobs,
encoding checkpoints and previous destinations retain exact historical targets
through native pickers and project changes. The production replay passes 112
checks, publishes four real movies and reconciles an existing destination without
changing the current edit. IME confirmations cannot activate modal actions; Tab
reveals focused controls and Escape restores editor focus after the modal closes.
New owner-started workflows open their current-session status. Full mastering,
HDR, preparation controls and complete product acceptance remain open.
Native keyboard browsing and save-sheet cancellation also pass with all 20
stored tables unchanged. Native AX omitted the custom overlay controls, so its
accessibility acceptance remains open despite the passing harness tree checks.
The locked workspace suite passes 2,578 tests; 344 optional UI-feature tests,
strict lint, formatting and native Metal startup/shutdown also pass.

[Returning to the same Original](qualification/source-return-2026-09-30.md)
now preserves its cursor through catalog sound selection and retains the
displayed picture while refreshing. The production sound replay reproduces the
old reset and passes with distinct Original/Edit positions after the fix.
This closes the focused issue found in owner-preparation inspection; full sound,
keyboard, accessibility and UI acceptance remain open.

[Open-project commands](LIVE_PROJECT.md) now route structural edits, history,
primary geometry and Render through the native writer's authenticated local
endpoint. Exact owner/revision targeting, independent replies, retained commit
receipts and explicit preview refusal preserve the native session. Original
retention, relinking, registration and checkpoints now prepare on its bounded
import worker and complete through the captured owner. Registration preserves
the caller's exact revision, identities, insertion and selected streams.
Checkpoints use a consistent read-only SQLite backup and retain the actual
captured revision, including a published receipt if final directory sync fails.
Full mastering, HDR and expanded output/release
qualification remain required. No DP requirement or delivery gate is complete.

[Owner-preparation qualification](qualification/owner-preparation-2026-09-30.md)
records 28 actual CLI invocations, including nine failed stream preparations
followed by successful registration, exact/no-op/stale relinking and a consistent
checkpoint. Native catalog/picture refresh and keyboard navigation were inspected
without reopening; operational changes preserved authored history. The locked
workspace suite passes 2,572 tests; 338 optional UI-feature tests, strict lint,
formatting and native startup/shutdown also pass. Native relink/checkpoint controls, complete
preparation/recovery acceptance and representative preparation performance remain open.

[Open-project qualification](qualification/live-project-2026-09-30.md) covers
36 real CLI invocations against a native writer, three actual preview refusals,
concurrent editing during Render, exact cancellation, historical retry and
reconciliation. Independent readers pass all 768 picture planes and complete
authored audio in the initial/retry movies. The retained full workspace run and
scoped corrections cover 2,523 distinct passing tests; 330 optional UI-feature
tests, strict Clippy, formatting, native startup/shutdown, 79 visual Render checks
and 2,426 release replay checks pass. The saved-render boundary above adds native
persisted recovery and progress access for owner-started jobs. The endpoint crash
matrix remains open.

[Native and public Render](RENDER_JOBS.md#native-and-public-render) expose
the automatic SDR workflow through Cmd-E, `:render` and closed-project headless
commands. Camera, Gain and Room tone previews require an explicit decision and
bind rendering to the exact durable commit receipt. The production replay covers
real publication and later Undo; the public workflow covers checkpoint retry,
reconciliation, stale/collision refusal, SIGINT cancellation and JSON re-encoding.
[Qualification and retained evidence](qualification/public-render-2026-09-30.md)
record 2,463 passing workspace tests, 318 optional UI-feature tests, strict Clippy,
formatting, native startup/shutdown, 71 Render replay checks and 2,418 release
replay checks. Independent readers pass all 768 picture planes and complete
authored audio in both public exports. Final decision/result captures were inspected.
That qualification covers closed-project CLI commands; the open-project boundary
above extends the entrypoints without changing its media qualification scope.

The following milestone notes retain their original measured scope; the current
entrypoint status is described above.

[Durable automatic encoding](RENDER_JOBS.md#automatic-admission-and-recovery)
now retains an automatic job policy and an immutable decision per fresh encoding
attempt in database 42. Qualification stays Queued until the decision and
Encoding transition commit atomically. New encodes qualify again; checkpoint
retry and reconciliation preserve their original decision and freshly verify
the movie. Strict legacy adapters preserve engineering data and reject newer
vocabulary in old schemas. Native/public headless Render, complete output
qualification and the full DP-17/DP-18 requirements remain open.

[Qualification](qualification/automatic-render-jobs-2026-09-30.md) passes two
fresh native encodes, four fresh verifications, reopened checkpoint retry and
reconciliation. Independent readers check all 768 picture planes and complete
authored audio in both files. Every old cell in 19 database tables is preserved.
Final workspace coverage is 2,439 passing tests after one historical migration
expectation correction; all 310 optional UI-harness app tests, strict Clippy,
formatting and native startup/shutdown pass. The failed runs remain retained.

[Runtime-bound project encoding](qualification/encoder-runtime-2026-09-30.md)
now consumes a fresh automatic admission. The probe and project worker retain
matching mapped helper/library descriptors and require stable hashes, platform
facts and frozen SDR controls. The native example verifies a full 128-frame,
205,005-sample committed project and preserves every cell in 19 database tables.
Independent readers pass every picture plane and the complete authored audio;
final workspace coverage is 2,397 passing tests after correcting one transport
fixture. Strict Clippy, formatting and native startup/shutdown pass.
Serialized evidence cannot restore a live admission. Native/public headless
Render and full output qualification remain open; no DP
requirement or delivery gate changes status.

[Automatic encoder probes](AUTOMATIC_ENCODER_ADMISSION.md) now select a path
through fresh supervised deterministic encodes, complete file verification and
expected picture/audio checks. Ordered typed rejections remain in the decision;
non-capability and cleanup failures stop selection. The durable job/attempt
boundary is described above; public Render remains required for DP-17/DP-18.
[Qualification](qualification/encoder-admission-2026-09-30.md) records 2,384 passing
workspace tests, strict Clippy, formatting and native startup/shutdown. Four real
cases passed 190 pictures and 24 exact AVFoundation event positions. The 14x16,
16x16 and 64x64 probes explicitly failed decoder geometry admission and remain
unqualified; their failures and confirmed cleanup are retained.

[Typed encoder failures](qualification/encoded-failures-2026-09-30.md) preserve
exact native failure kinds through supervision and invalidate them after later
protocol, timeout or cleanup faults. Real media reproduces the hardware B-frame
rejection and separately verifies a no-B encode without authored changes. The
native matrix passes 872 pictures and 180 exact audio event coordinates. All
2,359 workspace tests, strict Clippy, formatting and native startup/shutdown pass.
This is DP-17/DP-18 groundwork; public Render remains open.

[Native marks](MARK_NAVIGATION.md) add persistent case-sensitive letter positions,
exact Original/Edit jumps, a keyboard-accessible list and bounded back/forward
navigation. Mark-only saves preserve editorial selection and history positions;
unrelated revisions expire transient Edit history while saved marks continue to
follow core transforms. Original history retains its qualified measured clock.
This remains partial DP-05/DP-20 work; full operator grammar, occurrence navigation
and complete physical-input/accessibility acceptance remain required.
[Qualification](qualification/native-marks-2026-10-01.md) records 448 UI-feature
and 412 default app/headless tests, formatting, strict workspace lint, 156 focused
rendered checks plus the expanded Kestrel audit, 3,589 full release checks, and
native keyboard/reopen verification with consistent SQLite backups. No release
gate is completed by these results.

[Native Trim](COMBINED_TRIM.md#native-trim) adds an implemented subset of
DP-02/DP-05/DP-20: `,v`/`:trim`, four ordered accepted In/Out/Slip/Roll values,
Ripple/Overwrite policy, paired Before/Proposed junction pictures, an admitted
Edit waveform and optional boundary audition. Apply requires the acknowledged
nonzero proposal and its current Proposed pair at the final viewer raster, then
saves one reversible transaction. Cancel preserves authored state; zero intent
has no proposal or history. Captured scope, target/right absence and both editor
cursors stay independent of inspection and heard position. Admission remains
limited to eligible direct Source/neutral unity Partition children of ordinary
Sequences; Original, Sounds, composites and Edit Visual ranges refuse entry.
The [native qualification record](qualification/native-trim-2026-10-01.md)
owns execution evidence and its limits. Broader target admission, the complete
editor workflow, physical keyboard/IME, accessibility and listening acceptance
remain open. No DP or gate status changes.

The following Trim/Slip prerequisite summaries retain their historical results
and checkpoint scope; those counts do not qualify the current native increment.

[Native Source Slip](SOURCE_SLIP.md#native-stopped-picture-preview) adds a
verified subset of DP-02/DP-05/DP-20. The `:slip`
workflow captures the selected beat and revision, reports exact clamps/no-op,
compares real stopped Before/Proposed pictures, and gates one Apply on the
current submitted picture. It preserves both cursors and retains a saved receipt
if workspace refresh fails. The app passes 464 feature-enabled tests plus 3
headless tests, and 428 default-feature tests plus the same 3 headless tests.
The focused replay passes 67 Slip checks; the full release replay passes 3,655
checks. Native cancel, apply, Undo/Redo and reopen pass, with both test instances
closed. See [qualification](qualification/native-slip-2026-10-01.md). At that
checkpoint, full Trim, waveforms/audition, unsupported target scopes and
physical-input/accessibility acceptance remained open. No requirement or gate
changes status.

[Combined Trim authoring](COMBINED_TRIM.md) is implemented,
including one accepted In/Out/Slip/Roll command, fixed-duration overwrite and
qualified store/CLI admission. Core 43/database 52 identify its vocabulary.
[Qualification](qualification/combined-trim-2026-10-01.md) records 3,366 passing
workspace tests and both documentation tests. After one equivalent stable-sort
cleanup, strict lint, 132 core tests and final formatting passed. Native Trim
was outside that backend checkpoint; no requirement or gate changes status.

[Combined Trim timing primitives](TRIM_DRAFT_FOUNDATION.md) advance
DP-02/DP-05. Complete-intent geometry, Source endpoint bindings and the root-sound
Trim map passed independent review and focused runtime checks, including 57 new
tests. The full workspace passed 3,333 unit/integration tests and both
documentation tests; strict workspace/all-target lint and formatting passed on
the same unchanged source inventory. The combined authoring command and overwrite
overlay are qualified above; native Trim was outside this foundation checkpoint.
No requirement or gate changes status.

[Adjacent Source Roll](SOURCE_ROLL.md) adds a backend subset for DP-02/DP-05.
It resolves one shared movement against both exact
Source limits, preserves combined duration, captures audio clocks once, retains
the independent sound bus and marks only the changed seam. Core 41/database 50,
store admission and cold/live headless dispatch are integrated.
[Qualification](qualification/source-roll-2026-10-01.md) records independent
picture, decoded-PCM and persistence checks. The full workspace passed 3,275
tests and both documentation tests with one stale doctor assertion; only that
test changed and its rerun passed. Strict all-target lint and formatting pass.
That backend checkpoint claimed no native Roll surface.
Full Trim requirements remain open; no requirement or gate changes status.

[Ripple Source edge trimming](SOURCE_TRIM.md) adds a verified backend subset
for DP-02/DP-05. The atomic command moves one In/Out edge with exact handle
clamping, retained physical owners, old-clock audio captures and one root sound
transform. Separate editorial audio intent adds the required fades at new Trim
edges and both Slip joins without changing raw sampling support.
[Qualification](qualification/source-trim-2026-10-01.md) records indexed-picture,
decoded-PCM and store/headless checks. The full workspace passed 3,250 tests and
both documentation tests with one outdated Slip assertion; only that test changed
and its rerun passed. Default app checks, strict all-target lint and formatting
also pass. The failed invocation and exact source comparison remain recorded.
Native Trim, overwrite, Roll and broader target admission remained open then; no
requirement or gate changes status.

[Exact Source windows](SOURCE_EDIT_WINDOWS.md) retain selected time before
whole-frame enclosure. [Atomic Source Slip](SOURCE_SLIP.md) shifts both linked
maps in one transaction under ordinary Sequences, preserving duration, effects,
bindings and root sounds. Stored admission is rechecked at commit; headless dry
runs expose exact handle clamping and no-op results. All 3,114 workspace
unit/integration tests, both documentation tests, formatting and strict all-target
Clippy with `ui-harness` pass. See [qualification](qualification/source-slip-2026-10-01.md).
Native Trim controls were outside that checkpoint; the command's explicitly
unsupported scopes remain open.

[Source effect clocks](SOURCE_EFFECT_CLOCKS.md) preserve existing camera paths,
gain envelopes and mute ranges when a physical Source grows earlier or later.
Core 38/database 47 retain explicit framing domains; gain translation uses the
existing owner coordinates. All 3,046 workspace unit/integration tests, both
documentation tests, formatting and strict workspace/all-target Clippy pass,
including lint with `ui-harness`. Physical growth, complete Trim commands and
native boundary previews remained open at that checkpoint. No
requirement or gate changes status.

[Retained Source origins](SOURCE_ORIGINS.md) add an exact translation between
current physical coordinates and captured audio clocks. Pure binding rebases
preserve sample grids, frozen layouts and chronological phase composition.
Decoded-PCM regressions pass for physical prefixes, fractional NTSC phase,
independent sample offsets and inverse restoration. Core 37/database 46 persist
the translation. All 3,019 workspace unit/integration tests, both documentation
tests, formatting and strict all-target Clippy pass. The later effect-clock work
above adds framing/treatment preservation. Atomic Trim authoring and native
controls remained open then; no requirement or gate changes status.

[Dormant linked audio](qualification/dormant-linked-audio-2026-10-01.md) retains
the Original's audio context in silent slices. Empty support emits silence without
requesting source PCM, and selection growth preserves the full affine mapping and retained
sample clock. Absent audio remains distinct. Core 36/database 45 and audio context
5 retain the new meaning while supported older grammars stay closed. This is
further Trim groundwork for DP-02/DP-05/DP-20; physical-origin translation, effect
owner clocks and the native Trim workflow remained required then. No requirement
or gate changes status.

Verification covers 2,169 distinct affected-crate tests and both documentation
tests, including no-read silence, independently expected PCM after selection
growth and durable linked context through reopen/Undo/Redo. Independent review
has no remaining findings. Workspace formatting and strict all-target Clippy pass.
No native app was opened for this increment; the final process check found none
running.

[Exact picture context](qualification/selected-video-context-2026-10-01.md)
retains the complete measured video span in new Original slices while keeping
their visible endpoints separate and exact. Rounded tails hold the last selected
picture; inverse source anchors reject hidden context. Audio placement and beat
duration remain unchanged. Core 35/database 44 reject unused development
databases 39 through 43 under the approved format policy, with old supported
grammars kept closed. This is a Trim prerequisite for DP-02/DP-05/DP-20;
native In/Out/Slip/Roll, ripple/overwrite and the complete visual Trim workflow
were outside that checkpoint.
Verification covers 2,956 distinct workspace unit/integration tests and both
documentation tests across the broad run and corrected storage run. All 270 plan
tests pass again after a private storage-layout adjustment; final strict
workspace lint and formatting pass. The evidence retains the failed attempts
and their corrections. No native app was opened, and no requirement or gate
changes status.

[Visual slice placement](spec/DEADPAN_SPEC.md#97-visual-slice-placement) is an
explicit DP-05/DP-20 requirement: select and refine a time range, see its source
endpoints and destination, audition both proposed joins, then commit or cancel
with the keyboard. Insert, replace and move require exact reversible semantics,
visible occurrence scope and stale-revision rejection. [Place slice](SLICE_PLACEMENT.md)
now previews linked Original copies at ordinary Sequence seams and direct
Source/ordinary Hold/supported-fragment interiors with decoded
endpoints, local In/Out refinement, a provisional timeline, Before/Proposed and
both-join audition. Interior placement captures its direct child and local
boundary, retains the split's original audio clocks and commits the split and
insertion as one command. The initial seam replay passed 215 checks including minimum
layout, complete Tab circuits, synthetic IME, cancellation, stale captures and
exact commit/undo. Actual decoded pictures and canonical nonzero PCM around both
joins match the committed result. Edited-slice copy/move,
picture/audio-only policies and Repeat/Retime occurrence targets
remain required; this does not complete DP-05 or DP-20.
[Qualification](qualification/slice-placement-2026-09-30.md) records native release
looping and pause/resume, unchanged SQLite rows after cancellation, all automated
checks and the measured debug-build audio starvation limitation.
[Interior placement qualification](qualification/interior-slice-2026-09-30.md)
adds Source/Hold/fragment insertion with one commit/Undo, exact retained suffix
PCM, failed-transaction rollback and 267 production replay checks. Raw count
batches and clamped motions across empty-group slots are covered by reproduced
failures and passing regressions. All 2,609 workspace and 362 UI-feature app
tests pass, with strict lint and formatting. Native release commit/Undo restores
all authored fields; the database records exactly one placement and one Undo.

[Visual replacement](qualification/slice-replacement-2026-09-30.md) adds an
independent Edit range, explicit Replace selection preview and fast `p/P`
replacement. The removed range stays fixed while source endpoints change.
One command preserves endpoint contexts and routes sounds directly from old to
final sample boundaries; one Undo restores the entire authored document. Core
34/database 43 use the approved development format break for databases 39–42.
The locked workspace passes 2,641 tests, the production replay passes 401 checks
and the UI-feature app passes 372 tests,
including pointer/keyboard switching, stale targets, exact comparison windows
and saved receipts surviving preview-refresh failure. Native release QA verifies
all four join pictures, one 14-frame replacement of `[30..60)` and one Undo.
SQLite backups confirm complete authored restoration and unchanged unrelated
tables. This remains partial DP-05/DP-20 work.

[Retained deletion](qualification/ripple-delete-2026-09-30.md) corrects the
native and CLI beat-delete path. Removing an ordinary Sequence child now keeps
each downstream structural audio owner's old sample entry and transforms the
root sound bus once. A real NTSC/44.1 kHz witness reproduced 6,405 changed samples
in a 6,406-sample suffix under historical Delete; the new command preserves the
complete decoded suffix. Core 34/database 43 stay unchanged, with old and new
commands replayable in the same history. The locked workspace passes 2,661
tests and the UI-feature app passes 375, with none failed or ignored.
Formatting and strict all-target workspace Clippy with the UI harness feature
pass. Visual range deletion is recorded below; role-only and temporal occurrence
deletion remain required. No DP requirement or gate changes status.

[Visual range deletion](qualification/delete-range-2026-09-30.md) adds one atomic
linked cut through ordinary Sequence scopes. Active and finished half-open
selections use `d`; empty selections cannot fall back to whole-beat deletion.
`:delete` captures its exact range or beat on entry, including absence. Endpoint
splits retain original clocks, downstream content retains its old sample entry,
and root sounds transform once. Six decoded-PCM tests cover NTSC Source/RoomTone,
earlier clocks, whole Repeat gaps, Preserve output and aligned cuts; persistence
and CLI tests retain exact one-command history and stale rejection after Undo.
The production range replay passes 98 checks, the slice regression passes 400,
and each passes all 11,904 Kestrel routing cases. Core 34/database 43 are unchanged.
Separate native QA verifies empty rejection, one cut, the exact rendered join
pictures and Undo. Consistent SQLite backups confirm one edit/Undo plus a
separate Redo/Undo picture check, full authored restoration and released locks.
The locked workspace passes 2,685 tests and the UI-feature app passes 380, with
none failed or ignored. Formatting and strict all-target workspace Clippy with
the UI harness feature pass on Rust 1.97.1.
Role-only deletion, temporal occurrence interiors, motion/text-object operators,
edited-content registers and copy/move remain open.

The [edited slice core](EDITED_SLICES.md) adds immutable capture and atomic seam
insertion through typed core/headless commands. Partial Source/Hold/Partition
windows retain complete owner contexts, historical clocks and bias-filtered
marks; each paste has independent authored/play/timing identities. Tests compare
exact picture plans and decoded NTSC PCM, repeated and partial copy-of-copy,
untouched destination content, historical media admission and durable Undo/Redo.
The store recaptures the named immutable revision before reusing its media, so
copies survive source deletion without accepting forged or unselected assets.
See [qualification](qualification/edited-slice-2026-09-30.md). Native edited
registers, placement, refinement and replacement, atomic move, cut-to-register and
temporal occurrence interiors remain open. No DP requirement or gate changes
status. Independent review is complete; all 2,717 locked workspace tests,
formatting and strict all-target workspace Clippy pass on Rust 1.97.1, with none
failed or ignored. Core 34/database 43 remain unchanged.

[Edited interior placement and replacement](qualification/edited-placement-2026-09-30.md)
adds `SpliceSliceAt` and `ReplaceSlice` through the same typed core/headless path.
Destination endpoint splits retain original lattices; replacement captures the
old suffix before removing selected children. Conditional clock allocation,
joint identity pools and one root-sound transform retain exact undo semantics.
Focused verification passes 126 core, 15 decoded-PCM, seven picture-plan,
39 store and two CLI tests. Independent review found no implementation defects.
All 2,739 locked workspace tests, formatting and strict all-target workspace
Clippy pass on Rust 1.97.1, with none failed or ignored. Native edited registers and
placement controls, atomic move and temporal occurrence interiors remained open
at that backend checkpoint. The native extension is recorded below.

[Native edited slices](qualification/native-edited-slice-2026-09-30.md) add
history-neutral `v`/motion/`y` capture to a shared session register. Accepted
copies survive later edits and Undo. `p/P` inserts or replaces atomically;
`:splice` refines the captured historical parent, displays composed copied
endpoints and previews the exact destination. Historical picture/audio access
uses store-issued admission with live session checks. Unsupported destinations
retain usable source endpoints, and saved receipts cannot consume a stale
visible selection. The production slice replay passes 547 checks plus the
11,904-case shortcut audit; the retained-project run adds one storage-root check.
Native keys verify copy/refinement, one insertion and Undo. SQLite backups prove
copy/cancel leave all 20 tables unchanged and both Undos restore complete authored
state. The corrected full release replay passes 3,110 checks; compact Sound,
Room tone and Gain viewers retain their existing minimum picture sizes.
All 2,766 locked workspace tests, formatting and strict all-target workspace
Clippy with the UI harness pass on Rust 1.97.1, with none failed or ignored.
Atomic move, persistent/named registers, cut-to-register, role-only placement and
temporal occurrence interiors remain open. DP-06 is now partial for its session
register; no DP requirement or gate is complete.

The [atomic MoveRange command](ATOMIC_MOVES.md) relocates current linked contents
within or between ordinary Sequence parents in one core/headless transaction.
Joint endpoint cuts retain complete owner contexts; whole moved nodes, Repeat
plays, marks and accepted providers keep their identities. Root sounds retain
their unchanged root clock and routes. Focused verification passes 101 core,
three picture-plan, nine decoded-audio, four persistence and one CLI test.
Independent implementation review found no further defect after correcting the
design's root-sound ownership rule. All 2,792 locked workspace tests, formatting
and strict all-target workspace Clippy with the UI harness pass on Rust 1.97.1,
with none failed or ignored. [Qualification](qualification/atomic-move-2026-09-30.md)
retains the initial test fixture failures. Native Move operation selection,
removal/insertion join previews, audition and final range selection were still
required at that checkpoint. The native increment below supplies them within
ordinary Sequence scopes. No DP requirement or gate changes status.

The [native Move workflow](qualification/native-move-2026-09-30.md) adds explicit
Copy/Move choice to `:splice`, local saved/proposed comparison at both joins and
one atomic commit selecting the complete moved range. Source endpoints remain
available for rejected destinations. Historical copies cannot remove current
content. Site audition uses bounded absolute-sample windows; reparenting without
timing changes compares identical global coordinates. Both service delivery
orders retain exact destination scope and selection. Qualification separates
decoded PCM/picture checks, Metal replay, native keys and remaining device limits.
Final visual replay passes 752 placement checks plus the shortcut audit; full
release replay passes 3,314. All 2,807 locked workspace tests pass, with none failed
or ignored. A final help-only sentence correction passes 414 app/harness tests,
formatting and strict all-target workspace Clippy. Native SQLite snapshots prove
copy/cancel and historical rejection preserve all 20 tables, one Move adds one
revision/history entry, and Undo restores complete authored state. No DP or gate
is complete.

The [nested fragment deletion check](qualification/nested-delete-2026-09-30.md)
extends `DeleteRange` to partial Source/ordinary Hold endpoints behind nested
unity Partition windows. Exact Split pools retain owner contexts and original
sample clocks; independent root sounds transform once. An isolated old-code
regression fails at admission. Current core, indexed-picture, decoded-PCM and
native service tests cover preserved content, marks, rejection bounds and one
durable deletion with reopen/Undo/Redo. Cut-to-register and empty structural
capture/paste were still open at that checkpoint. All 2,823 locked workspace tests
pass with none failed or ignored, along with formatting and strict all-target
Clippy. No DP requirement or gate changes status.

The [whole-child and cut register increment](qualification/structural-capture-2026-10-01.md)
adds exact selected-child capture, including nested empty Sequence groups.
Native `y` captures that child when no Visual range exists. Visual `d`, `dd`
and captured `:delete` prepare the copy privately, save one deletion and then
publish the historical copy. Failed capture or commit retains the previous
register; saved refresh failures retain the successful cut and reopen guidance.
An empty group pastes at an explicit sibling slot without changing picture or
sample time. Its placement card shows structure and named neighboring groups.
All 2,883 workspace tests pass; final formatting, strict all-target Clippy and
386 default-feature app/headless tests pass. Rendered cut/placement runs pass
150/820 checks, each plus the Kestrel audit. Native keys and SQLite snapshots
verify one saved cut, historical paste, cancellation and Undo; the QA app was
closed afterward. Full release replay passes 3,434 checks with no findings or
failed timing samples. Source identities and explicit coverage limits remain
in the qualification record. DP-06 remains partial.

The [shared render workflow](RENDER_JOBS.md#shared-workflow-and-native-ownership)
connects capture, encoding, retained checkpoints, fresh verification, publication
and reconciliation to the native project service. Close/switch/shutdown retain
the writer until worker release, and explicit subprocess cleanup evidence gates
terminal failure. Render progress and editor feedback have separate retained
state. The automatic policy is now connected to this shared workflow. Public
Render controls, full mastering/effects and HDR remain open, as do DP-17 and DP-18.

[Workflow qualification](qualification/render-workflow-2026-09-30.md) passes
21 real media assertions over two complete encodes, one cancelled encode, six
fresh verifications, checkpoint retries and destination reconciliation. Independent
readers pass 138 pictures, 221,021 authored sample frames and 414 complete planes.
All 2,347 locked workspace tests and strict all-target Clippy pass. Native service
tests cover editing and shutdown ownership; public Render interaction remains open.

The [publication journal](RENDER_PUBLICATION.md#durable-publication-journal)
records exact destination stages and authorizes renames only after checked
SQLite/database/WAL durability barriers. Explicit restart reconciliation requires
fresh checkpoint verification, recorded APFS identity and complete byte hashes.
It preserves committed movie knowledge, partials and foreign files. Database 41
adds publication tables while preserving schema-40 authored and operational cells.
Native Render, public headless rendering, automatic policy, complete audio/effects,
HDR and release qualification remain open. DP-17 remains Open.

[Publication recovery qualification](qualification/publication-recovery-2026-09-30.md)
passes 20 real SIGKILL cases and 40 fresh verifications across Source and Generated
fixtures. Exact authored/history cells are unchanged. Independent final-file
readers pass 138 pictures, 221,021 authored sample frames and 414 complete planes.
Process death is tested between completed host calls; physical power loss remains
unqualified. The full locked workspace passes 2,318 tests, strict all-target Clippy
and formatting on the same source inventory used for native qualification.

[Render checkpoint qualification](qualification/render-jobs-2026-09-29.md)
passes two real restart/reverification cases: 138 pictures, 221,021 authored sample
frames and 414 complete planes checked by independent final-file readers. Workspace
coverage totals 2,284 passing tests after three scoped target corrections, with zero
remaining failures or ignored tests. Strict workspace Clippy and formatting pass.
The original failed full run, corrected target runs and exact source differences
are retained. No native UI behavior changed.

The library [publication host](RENDER_PUBLICATION.md) now accepts a private verified
candidate, captures its historical revision and dependency evidence, and copies it
to an exclusive destination `.partial`. Exact descriptor readback preserves the
verified movie identity. It publishes a bounded local report before the movie's
atomic no-replace rename, preserves retryable candidates on earlier failure and
reports later failures as `PublishedUnconfirmed`. Report and movie publication
are separate commits. Native Render, public headless
render commands, automatic platform policy, complete mastering/effects, HDR and
release qualification remain open. DP-17 stays Open; no requirement or gate is
completed by this library boundary.

[Publication qualification](qualification/render-publication-2026-09-29.md)
passes seven fresh project movies through destination publication and independent
final-file readers: 304 pictures, 390,695 authored sample frames, 912 complete
planes and 18 exact audio marker observations. A separate audit checks all movie
and report hashes, historical document/Original receipts and effective Generated
intervals. All 2,248 locked workspace tests pass, with zero failed or ignored;
strict workspace Clippy and formatting pass. No native UI behavior changed.

The [finished-file verifier](FINISHED_FILE_VERIFICATION.md) now checks private
MP4 candidates in a separate supervised process: actual container/packet clocks
and edit lists, every decoded picture, complete fresh-IDR GOP comparisons, and
manual/ordinary AAC agreement over exact sample coordinates. Failure retains the
candidate for retry. [Current evidence](qualification/finished-file-verification-2026-09-29.md)
passes seven fresh project MP4s through normal and instrumented verifiers:
304 pictures, 390,695 authored sample frames and 21 GOPs per run. Independent
readers pass all 912 planes and 18 exact audio marker observations. The workspace
passes 2,222 tests, Clippy and formatting; 241 selected tests pass with native C
ASan/UBSan instrumentation.
Source header/table/packet limits still constrain capacity below the encoder's;
content, event-sync and runtime qualification remain separate. Library destination
publication is described above. Native Render,
automatic platform policy, full mastering/effects and HDR remain required. DP-17
stays open and no requirement or gate is complete.

The [encoded render worker](ENCODED_RENDER.md) now streams committed pictures
and canonical PCM into private H.264/AAC candidates with complete document
binding and clean-exit hash admission. [Actual project qualification](qualification/encoded-render-2026-09-29.md)
retains seven MP4s, complete decoded picture/PCM comparisons, exact edit-list
clocks, fresh-GOP checks and absolute audio markers through normal and sanitized
readers. Live edit/undo/redo, real cancellation, byte exhaustion and recovery pass.
This advances DP-16/17/18 groundwork. The production structural/decode verifier
is now described above. Native Render, automatic platform
policy, full audio/effects and HDR remain required. Statuses do not change; every
requirement and gate remains open or partial.

The [native SDR encoder and offline audio reader](NATIVE_ENCODING.md) now add
exact chronological I420/PCM admission, shared deadlines, descriptor-only MP4
fast-start and complete codec drain. [Normal and sanitized qualification](qualification/native-encoding-2026-09-29.md)
checks ten usable synthetic encoding cases, six deliberate failures and retained
hardware B-frame rejection per run. Three independent audio read modes preserve
absolute event timing; fresh decoders reproduce every observed GOP suffix.
This advances DP-13/16/17/18 groundwork. Isolated verification and library
publication now exist; native Render, automatic platform
policy, full audio/effects and HDR remain required. No requirement or gate is
complete.

The [render worker](RENDER_WORKER.md) now isolates committed SDR picture
preparation behind strict versioned messages and the shared checked process
supervisor. It binds the complete document and exact output contract, then admits
bounded raw I420 only after clean teardown and independent artifact checks.
This advances DP-16, DP-17 and DP-18 groundwork. Later boundaries add encoding,
structural/decode verification and library publication; complete audio/effects and native Render remain required.
[Qualification](qualification/render-worker-2026-09-29.md)
passes 2,140 workspace tests and 128 actual worker checks, including 82 complete
isolated frames and cancellation/recovery. No status changes.

[Committed encoder pictures](qualification/export-pictures-2026-09-29.md) now
produce exact timestamped SDR I420 from one immutable revision/range, preserving
authored framing through legal even output geometry and retaining one completed
frame. Actual Metal passes 118 checks, including all 30 accepted Generated frames
and complete-plane numerical comparisons. This advances DP-16 and the DP-17
foundation. Later boundaries add encoding, structural/decode verification and
library publication. Complete audio/effects and native Render remain required;
statuses do not change.

[Accepted Generated Hold pictures](qualification/generated-pictures-2026-09-29.md)
now use the same model-independent cold reader in native preview and captured
revision preparation. It verifies six retained objects, strict durable
provenance, both asset records and freshly decoded canonical picture timing.
Real conversion/acceptance tests cover relocation, stale requests, historical
revisions, prefix reuse and damaged dependencies. Native replay checks every
sampled frame through production keyboard navigation and Metal, with visible
Picture/Sound policy at both window sizes. This advances DP-12, DP-16, DP-19
and DP-20 without completing them. App generation/acceptance, full-size media,
model corpus, offline export and release qualification remain required.

Specification 1.1 applies them to [one Original](SINGLE_ORIGINAL.md): the full
video is the initial edit, native projects live in Documents/Deadpan, same-video
moments can be reused, and external media contributes sound only. Accepted AI
extensions remain in scope. Generic backend and legacy multi-video projects are
preserved. [Current design targets](design/README.md) replace the earlier
multi-video workspace; concept screens do not establish completed capabilities.

**Open** means required behavior has no qualifying implementation. **Partial**
identifies concrete groundwork while acceptance remains unmet. **Complete**
requires linked code, passing relevant tests, and a demonstrable acceptance
result. No requirement or delivery gate is complete. [Development](DEVELOPMENT.md)
defines the repository gate and the [UI feedback loop](UI_FEEDBACK.md) defines
application replay, image review and separate responsiveness measurements.
Neither a passing harness nor a screenshot establishes full acceptance. Current
interaction suggestions and personal shortcut conflicts are tracked in
[Interaction review](INTERACTION_REVIEW.md); each implemented fix needs its own
evidence. The initial optional UI harness passed nine visual replay scenarios
and its shortcut audit; its release run missed Repeat/Hold feedback targets.
The [expanded qualification](qualification/workspace-layout-2026-09-27.md)
records fourteen-scenario composite coverage and fresh release p95 values of
4.67 ms for navigation, 6.85 ms for Repeat and 9.11 ms for Hold, all within their
existing targets. The earlier misses remain retained. Native accessibility,
physical display behavior and full-size workloads remain open.

The [footer qualification](qualification/footer-layout-2026-09-27.md) resolves
command-exit layout gaps, clipped Original shortcuts and immediate-Escape text
loss. It retains failing development cases, controlled async test delivery,
final workspace checks, same-frame replay assertions and reviewed Metal captures.
The minimum-size picture still needs more room; no requirement or gate closes.

The [repaint-wait correction](qualification/repaint-wake-2026-09-26.md) replaces
harness polling with bounded egui wakeups and separates worker phases from UI
delivery. That increment's feature lint and 193 unit plus 2 integration tests
passed, while both GPU replays stopped before scenarios because Metal was
unavailable. The later
[layout qualification](qualification/workspace-layout-2026-09-27.md) restores
actual Metal evidence for all fourteen current scenarios through a full run and
affected-path follow-ups. It fixes common Camera visibility, first-frame error
and speed-hint clipping, and pinned sound controls. Default app tests (208) and
feature tests (236), both Clippy configurations and the final focused replays
pass. These checks do not complete the full editing grammar or native
accessibility acceptance.

[Authored framing and Camera](FRAMING.md) implement
per-node static/enveloped framing, retained effect clocks and intermediate clips,
core-18/database-24 history, shared native/offline spatial rendering, and a
temporary keyboard Camera mode. The [new Camera target](design/README.md) retains
the intended saved-target and region workflows, which are not implemented yet.
[Qualification](qualification/framing-2026-09-24.md) records headless, actual Metal,
migration and native aesthetics/keyboard evidence. This work does not complete
DP-08 or any gate.

[Captured framing](CAPTURED_FRAMING.md) adds retained canvas fits and intermediate
clips to Hold recipes, independent of provider changes and the new Hold's Camera
settings. Root pause insertion samples descendant framing and inherits the root's
live framing once. Core 19/database 25 preserve earlier history through a frozen
core-18 adapter. Its [qualification record](qualification/captured-framing-2026-09-26.md)
separates headless checks from blocked Metal and native visual review; this work
does not complete DP-08 or any gate.

[Exact Original moments](SOURCE_MOMENTS.md) derive selected VFR picture intervals
and preserve fractional audio boundaries with a separate audible selection over
the full measured sample span. Core 20/database 26 and audio-context schema 2
retain that intent. Native selection and Sequence-slot paste now build on this layer; persistent
registers, named moments and the complete general splice remain required. [Verification](qualification/source-moments-2026-09-26.md)
records the tests, review and remaining acceptance. No requirement or gate changes status.

[Repeat-gap definition clocks](AUDIO_DEFINITIONS.md) provide current gap recipes
on intrinsic PointCeil or explicit root/point placements, including one-play
Repeats without a rendered gap. The bounded frozen-layout support query resolves
actual stable gap occurrences without crossing an opaque Preserve clock.
Authored gap bindings now consume these operands; general atomic splice
remains open. Verification of the original operand increment is recorded in
[the gap-clock qualification](qualification/gap-clocks-2026-09-26.md).

[Compact audio reanchors](AUDIO_REANCHORS.md) retain per-occurrence visible
allocation entries and chronological sample-phase changes without expanding
Repeats. Core 21/database 27 preserve these steps through copied scopes and
durable history; older histories gain no invented steps. General cursor splice and the full register workflow remain open.
[Qualification](qualification/audio-reanchors-2026-09-26.md) records verification
and limitations. No DP requirement or gate changes status.

[Authored gap bindings](GAP_AUDIO_BINDINGS.md) add core-22/database-28 ownership
for a Repeat's gap recipe, separate from its full timeline. Capture includes
unplayed gaps; stable existing gaps retain their sampling phase while newly born
gaps use their canonical definition clock. Root, point and physical-domain
evaluation share the PCM path, current policy and source admission. Complete
range splice and persistent register support remain required.
[Qualification](qualification/gap-bindings-2026-09-26.md) records passing focused
checks, strict history replay, independent review and the full-run failures with
their diagnostic follow-up. No requirement or gate changes status.

[Editable Repeat gaps](REPEAT_GAP_BRANCHES.md) add core-23/database-29 sparse
owned branches, including zero-gap suppression, dormant final branches,
occurrence-safe copies and exact default-gap materialization. The current Node
recipe retains historical gap clocks without freezing its raw audio policy.
Core, plan and decoded-PCM tests cover these paths; complete project-boundary
splice, persistent registers and full media acceptance remain open.
[Qualification](qualification/gap-branches-2026-09-26.md) records current CLI
history checks, independent reviews, strict migrations and workspace/harness
results, including the remaining sandbox-denied socket check.

[Composite suffix insertion](INSERT_TIME.md) adds core-24/database-30 pause edits
at existing root Sequence seams, with current-clock entry dispatch through
Repeat/gap branches and Preserve outputs. Old-admitted edits keep their prior
reducer, and older history cannot acquire the broader admission. Core, picture,
decoded-PCM, native service and migration checks are recorded in
[qualification](qualification/composite-insertion-2026-09-26.md). The new UI
replay steps are present but their visual run is blocked by the sandbox's missing
Metal adapter. Arbitrary interior splice and Visual replacement remain open;
no requirement or gate changes status.

[Interior pause insertion](INSERT_TIME.md) adds core-25/database-31 atomic cuts
inside root Source/ordinary Hold fragments before composite suffixes. Separate
pre-Split sampling and post-Split placement records retain exact rounded audio
entries, captured framing and one-step undo. Frozen core 24 retains its narrower
contextual admission. [Qualification](qualification/interior-insertion-2026-09-26.md)
records this increment's checks and limitations. Arbitrary nested insertion and
Visual replacement remain required; no requirement or gate changes status.

[Nested Sequence pauses](INSERT_TIME.md) add core-26/database-32 admission for
cuts inside ordinary groups. The Hold belongs to its actual Sequence, live
ancestor framing applies once, and every later sibling resumes from its own
audio entry. The native completion records the exact cursor and selects the
visible enclosing child. Repeat/Retime interiors, rational owner clocks and
Visual replacement remain required. See
[qualification](qualification/nested-sequence-2026-09-26.md) for the checks and
limits. No requirement or gate changes status.

[Sequence group navigation](GROUP_NAVIGATION.md) adds native Enter/Backspace,
breadcrumbs, current-depth inspector edits, scoped Original reuse and Camera.
Service requests retain their exact scope across preparation and completion;
history reconciles paths without retargeting the cursor. The contributed UI
harness now covers nested navigation and editing. Repeat/Retime occurrence
navigation and the full keyboard editing grammar remain open.
See [group navigation qualification](qualification/group-navigation-2026-09-26.md)
for actual checks, review and the unavailable GPU replay.

[Original moment selection and paste](SOURCE_MOMENTS.md) add native v/y and p/P,
an identity-bound session copy, measured temporal range bar, and exact explicit
Sequence-slot insertion. Core 27/database 33 retain shifted audio entries and
admit prepared receipts atomically with history/relevance. Persistent/named
registers, Visual replacement, arbitrary occurrence/cursor splice and native
visual/performance acceptance remain open. See
[paste qualification](qualification/moment-paste-2026-09-27.md) for actual evidence.
No requirement or gate changes status.

[Exact boundary descent](STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent)
now exposes the complete revision-bound owner path through Sequences, Retimes,
stable Repeat plays and implicit/owned gaps. Core and headless queries retain
fractional coordinates and shared work bounds without editing the project.
This is a prerequisite for arbitrary nested insertion, not its implementation.
[Qualification](qualification/boundary-location-2026-09-26.md) records the scope
and verification. No requirement or gate changes status.

[Derived-clock framing](FRAMING.md) evaluates exact rational owner extents with
bounded numeric arithmetic while preserving existing integer callers. It is a
prerequisite for the [nested splice representation](STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks),
not support for fractional authored frames or a new editing command. Structural
clock maps, retained DSP contexts and arbitrary nested splice remain open.
[Qualification](qualification/framing-clocks-2026-09-26.md) records the independent
reference cases, reviews, regression checks and unchanged schema boundary.

[Live audio input tapes](AUDIO_INPUT_TAPES.md) project exact current-tree windows
onto one PointCeil grid and read their PCM through the shared renderer. They
retain sampling support and scoped identities. Checked `AudioStageProjection`
views now supply nested intrinsic operands, independent output-policy clocks
and request-local PCM memoization; a PointCeil tape can schedule that output
around an inserted pause. [Projected root placement](AUDIO_PROJECTED_ROOT.md)
separately supplies absolute RoundEven allocation and repeated resume that
preserves phase for one physical projection. Persisted splice routes, effective owner
clocks, aggregate output scheduling and arbitrary nested editing remain open.
These evaluation APIs do not complete a requirement or gate.
[Qualification](qualification/audio-input-tapes-2026-09-26.md) records its exact
mapping tests, real PCM comparisons, independent review and repository checks.
[Intrinsic projection qualification](qualification/preserve-projections-2026-09-26.md)
records nested-history PCM, output-policy clocks, admission and memory limits,
review corrections and the unchanged authored-schema boundary.
[Root projection qualification](qualification/projected-root-2026-09-26.md)
records independent sample-phase and policy tests, admission review and the
required repository checks.

[Scoped sound mixing](SOUND_EVENTS.md) adds borrowed ordered voices, exact gates
and per-voice silence on a common intrinsic grid. Explicit aggregate Preserve
input uses one canonical preparation and retains its independent output-policy clock. This is
raw preparation before creative voice effects and mastering. Those borrowed
operands alone provide no persisted recipes, edit transforms, native placement,
voice treatments, final bus integration or preview/export evidence. Catalog import
alone does not place a sound, and this prerequisite does not promote a DP or gate.
[Qualification](qualification/scoped-mix-2026-09-27.md) records decoded PCM,
scope/policy checks, review corrections and remaining acceptance work.

The sound increment adds separate catalog audition through canonical playback,
a bounded exact recipe-route kernel and a CLI LRU source cache. Sound playback
uses measured sample endpoints and preserves the stopped picture, edit clocks
and selection. The route kernel is not yet installed in authored sound events;
no placement, migration, DSP lattice or final mix is implied. The
[audition board](design/boards/sound-audition-board-v1.png) and exact prompt are
retained targets. [Qualification](qualification/sound-audition-2026-09-27.md)
records reviews, actual PCM, harness checks and acceptance limits. All DP and gate
statuses remain unchanged.

The [sound-clock preparation increment](qualification/sound-clocks-2026-09-27.md)
adds physical sample-grid history to exact routes, separate current Hold issuer
queries, and bounded LRU eviction to playback's source cache. Window/Keep
projections preserve both the complete recipe and the old selection's audible
mask, so displaced rounding cannot expose audio past a cut. These remain
preparation APIs; that increment did not add authored sound persistence,
placement, voice processing or final mixing. It left core 28/database 34 unchanged. No DP or
gate status is promoted.

The [catalog source-voice increment](qualification/source-voices-2026-09-27.md)
adds a checked independent audio operand with exact natural-rate source placement
and separate input/output Hold policy. It reuses the tape, source admission and
canonical Preserve readers without adding authored Source nodes. This does not
install sampled routes or persisted events, grant allowances, implement voice
effects or populate the final bus. All DP and gate statuses remain unchanged.

[Retained sample routes](qualification/routed-voices-2026-09-27.md) now have checked
PCM preparation handles for independent source input, intrinsic Preserve output
and captured root output. Reads preserve
the old physical samples and complete filter/DSP history instead of reconstructing
phase from the edited frame clock. Captured output policy and old selected masks
stay separate from current consuming Hold gates. These remain preparation APIs;
that increment did not add persisted sound commands, allowances, voice effects
or final bus integration. It left core 28/database 34 and all DP/gate statuses unchanged.

The [playback test scheduling record](qualification/playback-waits-2026-09-27.md)
tracks recurring cold-preparation timeouts separately from product failures.
The test-only PCM reservation includes detached worker teardown; production
timeouts, PCM assertions and all requirement/gate statuses remain unchanged.

The [root sound-event increment](SOUND_EVENTS.md#persisted-root-sounds) adds
qualified, durable `SetSound`/`DeleteSound` recipes in core 29/database 35.
Events keep picture duration unchanged and enter the real pre-master bus with
independent sample phase, edges and gain before one shared limiter. Root
RoundEven evaluation, reopened history, scalar decoded-PCM sums, cached source
revocation and source-only audition are tested headlessly. Temporal edits are
guarded except for the root ripple subset below. Nested ownership, Hold allowances,
voice effects, listening and preview/export acceptance remain required. Native
placement is covered by the later increment below; no DP or gate is promoted.

The [root-sound qualification](qualification/root-sounds-2026-09-27.md) retains
decoded-PCM witnesses, review corrections, migration provenance and the original
test-gate failure. The [sound-placement board](design/boards/sound-placement-board-v2.png)
and exact prompts define the intended native interface; the later native
qualification below records implemented placement controls.

Core 30/database 36 add [persisted root ripple edits](SOUND_EVENTS.md#persisted-root-ripple-edits).
InsertTime, SpliceSource and ordinary Sequence Delete retain complete sound
recipes and chronological physical sample clocks; non-root Split keeps the
root bus unchanged. Parameter changes retain routing, and explicit ReplaceSound
clears it. Actual database-35 history migrates without invented routes, while
forged sound-bearing legacy Split histories remain rejected. Root Split,
temporal occurrence edits, nested ownership, remaining structural sound edits,
allowances and effects remain open. All DP and gate statuses
remain unchanged. The [routing qualification](qualification/root-sound-routing-2026-09-27.md)
records review corrections and verification scope.

The [native sound-placement increment](qualification/native-sound-placement-2026-09-27.md)
connects that guarded root subset to catalog placement, a distinct placed-event
selection and inspector, exact frame nudges, fine sample entry, gain, edges,
removal and durable history. Picture duration and the retained beat stay intact.
Production replay covers focus entry and command targets across delayed writer
completion. The [ImageGen board](design/boards/sound-placement-board-v2.png)
guides the appearance; it does not implement nested ownership, scoped Hold
allowances, effects, listening or export acceptance. Every DP and gate remains
open or partial.

Core 31/database 37 add [persisted root sound allowances](SOUND_EVENTS.md#persisted-root-sound-allowances).
One sound can pass through one concrete silent Hold or Repeat gap while other
contributions and occurrences remain suppressed. Structural edits retain or
remap the relation, old histories gain no permission, and source admission runs
for allowance-only transactions. Native inspector controls and
`:sound-allow` / `:sound-silence` capture the sound, Edit frame and exact issuer.
Retained route gaps remain empty. Nested ownership, send/tail rules, the full
voice graph, listening and preview/export acceptance remain required; no DP or
gate is promoted by this subset.
The [allowance qualification](qualification/sound-allowances-2026-09-27.md)
records review corrections, migration provenance, actual PCM and UI evidence.

Core 32/database 38 add [atomic Hold audio policy changes](ROOM_TONE_AUDIO.md).
The direct and occurrence setters retain timing and picture, validate new
source choices against measured admission, and retire obsolete silence
permissions with exact undo. Frozen core-31 replay preserves prior allowances
without admitting the new command. The [authoring record](qualification/hold-audio-2026-09-27.md)
tracks migration, source admission and actual PCM verification. The
[room-tone board](design/boards/room-tone-board-v2.png) supplies the target for
native range selection, distinct source/pause audition and explicit application.
The [native workflow](ROOM_TONE_AUDIO.md#native-selection-and-audition) now
implements that ordinary-Hold subset: measured inward source-sample boundaries,
editable fields, identity-bound preparation, separate source audition, Apply,
silence and undo. Waveforms, Repeat-gap/fragment controls, physical input and
listening acceptance, full mixing and export remain required. No DP or gate is
promoted. See [native qualification](qualification/native-room-tone-2026-09-27.md).

[Gain recipes and owner clocks](AUDIO_GAIN.md) now provide bounded standalone
gain/mute evaluation and checked structural clock inspection. Core 33/database
39 now attach those recipes to nodes with direct/occurrence reversible commands,
frozen core-32 history replay and context-schema-4 treatment evidence. The
canonical authored bus applies gain after time/pitch and edges, before the
limiter; `inspect-audio --authored-bus` exposes bounded pre-limiter PCM. The
[gain design board](design/boards/clip-gain-board-v2.png) defines the native target.
The native increment adds captured beat trim/mute commands and an unsaved editor
for exact owner-output envelopes and mute ranges. Before/Draft comparison uses
separate proposed-content identities and one retained heard-sample window; only
Apply writes history. Verification is recorded separately in the
[native-gain qualification](qualification/native-gain-2026-09-28.md).
Exact-field and comparison replay, both app configurations and a native macOS
command/focus/text/cancellation pass are verified within that recorded scope.
The complete release replay passes 2,156 checks, retaining separate cold and
warm timing populations and the small-fixture/offscreen qualification boundary.
Waveform editing, physical keyboard/IME and acoustic qualification, full voice
processing and the complete DP-09 workflow remain required.
The [measured beat overview](WAVEFORMS.md) has scoped
[qualification](qualification/gain-waveform-2026-09-28.md), with 32 focused
waveform tests and all 2,062 normal workspace tests passing. After an app-only
visual alignment fix, final normal/optional app tests pass 267/302 cases,
and painted Gain/room-tone replays pass 291/221 checks plus their routing audits.
The full release replay passes 2,348 checks across 18 scenarios. This is a bounded
pre-effects reference for one
captured committed owner, not a final-mix or timeline-wide waveform editor.
The [compact workspace increment](qualification/compact-workspace-2026-09-28.md)
also consolidates empty Placed sounds into a visible Beats-heading target at
short window heights. Copied-range controls retain a measured 143-point minimum
picture in the qualified Hold state; nested navigation, focus, scale changes
and first placement/undo have actual painted replay evidence. Full UI and
performance acceptance remain open.
See the [authored-gain qualification](qualification/authored-gain-2026-09-28.md)
for review corrections and verification limits, and the
[earlier gain/clock record](qualification/gain-clocks-2026-09-27.md) for the pure
recipe/query checkpoint.

[Original and edit audition](PLAYBACK.md) connects immutable qualified originals and
canonical limited PCM to the native device, with Space Play/Pause, a monitor
level, exact audio-clock picture scheduling and explicit interruption. Bounded
preparation runs separately from device control. Original Space playback uses
the complete measured A/V union; Shift+Space loops the selected Original moment
or edited beat with adjustable context. Delivery remains monotonic across laps,
and read-only audition leaves history unchanged. Full mastering,
acoustic synchronization, long-source/performance qualification and
preview/export equivalence remain required. This increment does not complete a
requirement or gate; [qualification](qualification/playback-2026-09-24.md) records
its native, headless and review evidence. Older evidence below describes its own
historical boundary.
The [Original/selection increment](qualification/original-audition-2026-09-27.md)
records the separate full-source and selected-picture endpoint policies, real
PCM and native routing regressions, and contributed harness coverage. All product
requirements and gates retain their existing open/partial status.

[Master gain research](qualification/audio-limiter-gain-search-2026-09-24.md)
retains independently audited finite-fixture solutions, the corrected unwanted
muting failure, and a longer stress failure in a faster finite-context design.
Neither of those earlier candidates is adopted. The [shared limited reader](AUDIO_MASTERING.md)
now implements a pinned finite oversampled gain path over the current edge-faded
bus, consumed by playback and `inspect-audio --limited`. It uses canonical tiles,
complete source provenance, one cumulative preparation budget and final-f32
verification, with [qualification evidence](qualification/audio-limited-2026-09-24.md).
Full voice/group processing, gain-motion/listening acceptance and
encoded output remain open. No requirement or gate changes status.

[Pause insertion](INSERT_TIME.md) adds atomic root Source/Hold splices with
sample-preserving resume for every shifted fragment, including earlier splits
and repeated pauses. Native `,h`, counts and exact `:hold` duration entry resolve
a measured freeze frame, retain the Original baseline and select the committed
pause. Core 17/database 23 freeze prior command history. Arbitrary nested scope,
compact Repeat-gap ownership, active generation context resolution, playback and
export remain required. [Qualification](qualification/insert-time-2026-09-24.md)
records exact timing, native interaction and actual baseline migration evidence.
No requirement or gate changes status.

[Logical mark bindings](MARK_FRAGMENTS.md) preserve one named mark across physical
bindings, with atomic loss/copy transforms, biased Partition seam visibility and
exact named resolution. Core 13/database 19 introduced frozen mark grammars.
[Qualification](qualification/mark-fragments-2026-09-23.md)
records review, migration and test evidence for this layer.

[Structural Split](STRUCTURAL_SPLIT.md) now cuts explicit beats and isolated
occurrences without changing duration or output, retains full processing contexts,
and refines repeated cuts without increasing wrapper depth. Native `s`/`:split`
captures an interior boundary in the selected child and selects the committed right fragment.
Core 14/database 20 freeze prior history, including multi-binding marks.
[Qualification](qualification/structural-split-2026-09-23.md) records parity,
history and native review. Arbitrary Hold insertion, shifted-fragment sample
resume, minimal nested range planning and the full editing workflow remain open.

[Audio sampling clocks](AUDIO_SAMPLING.md) now separate allocation grids, exact
sample maps and retained envelope progress in all source/stage readers. Contract
tests exercise fractional-rate resume composition and envelope exhaustion without
changing filter support or rate. These are derived plan values, not persisted
Hold edits. Authored reference domains, genuine seam fades and arbitrary-boundary
insertion remain open.
[Qualification](qualification/audio-sampling-2026-09-23.md) records 985 passing
tests, the full gate and the reviewed wide-envelope compatibility fix.

[Frozen audio reference layouts](AUDIO_REFERENCE.md) retain old timing, compact
play identity, exact Source placement and Hold/gap policy without live sibling
dependencies or media ownership. Explicit root/Preserve clocks query old
audibility, and a bounded root-resume consumer combines it with current silence.
Actual DSP regressions cover both one-sample NTSC suppression mismatches.
This is a capture/query/PCM foundation, not an authored insertion or persistence
change. [Qualification](qualification/audio-reference-2026-09-23.md) records
1,011 passing tests, the full gate and independent review; schema versions
remain unchanged.

[Sampled-root transfer](AUDIO_SIGNAL_TRANSFER.md) converts admitted raw root PCM
onto an explicit point grid, retaining old silent samples before interpolation
and exact output suppression afterwards. StageAudio keeps full preparation
contexts and shares work, provenance checks and one deadline across all halo
reads. Creative fades remain after time mapping. Tests include real decoder and
canonical DSP paths. [Qualification](qualification/audio-transfer-2026-09-23.md)
records 1,021 passing tests, the full gate and independent review. Authored live-to-frozen bindings, policy replacement,
compact Repeat lifecycle, atomic Hold insertion and application playback remain
open. This conversion introduced no authored-state change and did not complete a gate.

[Physical reference-domain lookup](AUDIO_REFERENCE.md#physical-processing-domains-and-root-maps)
now retains opaque Preserve and meaningful context independently of visible
Partition allocation. Borrowed root maps compose the active cut and independently
anchor later domains. Tests cover NTSC phase, compact repeats, clock ownership,
real canonical DSP and fractional transfer. This does not author an insertion:
persisted sample bindings and their live edit lifecycle remain required. [Qualification](qualification/audio-domains-2026-09-23.md)
records 1,026 passing tests, the full gate and independent review; no requirement
or gate changes status.

[Authored audio copy lineage](AUDIO_LINEAGE.md) now survives Split, occurrence
isolation and durable history. Raw audio changes detach affected contexts and
ancestors while unrelated copies retain their relationships. Frozen reference
plans compare explicit lineage separately from physical identity and require
compatible clocks, phase and stable occurrence paths. Core 15/database 21 migrate
actual old copy history without inferring lineage from an initial snapshot.
[Qualification](qualification/audio-lineage-2026-09-23.md) records lifecycle,
strict migration and bounded reference tests. Live sample bindings, policy
replacement, atomic Hold insertion and application playback remain open.

[Retained audio contexts](AUDIO_CONTEXT.md) now capture complete raw audio trees,
exact media inputs and immutable asset contracts. Direct audio-only compilation
retains Source absence, RoomTone, nested Preserve, edges and compact Repeat
structure. The headless host authenticates each context against its exact
historical revision before verified media reads, including after undo and alias
reuse. This adds the old signal body; authored live bindings, phase composition,
policy replacement and atomic Hold insertion remain open. Core 15/database 21
are unchanged. [Qualification](qualification/audio-context-2026-09-23.md) records
the decoder/DSP parity, strict ingress and host-admission evidence.

[Physical audio domains](AUDIO_PHYSICAL_DOMAINS.md) now render retained Source,
RoomTone and Preserve context outside visible Partitions on the original signed
root grid. Both processing and policy queries stay inside the captured physical
subtree. Point-grid transfer preserves fractional phase and old silence through
one shared preparation allowance. Headless inspection reads actual qualified
historical media. [Qualification](qualification/audio-physical-domain-2026-09-23.md)
records hidden-sibling, negative-coordinate, decoder/DSP and admission tests.
Authored bindings, retained evaluation ownership, lifecycle/policy replacement
and atomic Hold insertion remain open; no requirement or gate changes status.

[Authored audio definitions](AUDIO_DEFINITIONS.md) now expose the actual Repeat
default independently of audible occurrences, with scoped point-grid queries,
real source/DSP rendering and historical headless admission. This resolves the
all-overridden-default counterexample for a future new-play operand.
[Qualification](qualification/audio-definition-2026-09-23.md) records exact
grid, silence, cache and media tests. Authored birth rules, binding lifecycle
and Hold insertion remain open; no requirement or gate changes status.

[Owned recipe clocks](OWNED_AUDIO_CLOCKS.md) evaluate current physical definitions
in explicit signed root placements through the shared reader. Source alignment
and silence-policy edits affect that revision's raw recipe; explicit historical
reads retain their old meaning. This supplies the evaluation boundary for the
proposed owned-tree binding model, without a second frozen raw-body graph.
Persisted anchors, compact Repeat birth/survivor rules, command lifecycle and
atomic Hold insertion remain open. No requirement or gate changes status.

[Owned-clock qualification](qualification/owned-audio-clock-2026-09-23.md)
records 11 new tests, the 1,108-test full gate and two independent reviews for
this evaluation boundary.

[Owned audio bindings](OWNED_AUDIO_BINDINGS.md) add the core 16/database 22
representation for timing lattices, stable Repeat scope, explicit births and
bounded symbolic phase. Split/isolation copy live arguments while retaining old
aliases; removal and inverse patches retain atomic ownership. PointCeil owned
evaluation preserves selected origins and nested preparation budgets. InsertTime
now captures bindings for supported root splices. Normal StageAudio rendering
now consumes root/point bindings, current and retained-placement policies, exact
resume phase and virtual post-mapping fades. A pure capture helper retains
compact birth scope and existing bindings, including configured Repeat gaps
through the separate [gap ownership map](GAP_AUDIO_BINDINGS.md).
Context-schema-1 capture and source-only SequenceAudio still reject nonempty
bindings. Full command lifecycle and arbitrary Hold insertion remain open.
No requirement or gate changes status.

[Binding qualification](qualification/owned-audio-bindings-2026-09-23.md)
records 1,140 passing tests, the complete repository gate and three independent
reviews for this representation and evaluation boundary.

[Consumer qualification](qualification/owned-audio-consumer-2026-09-23.md)
records 1,172 passing tests and the full gate for retained PCM,
post-mapping fades, compact capture, independent grid policies and shared
preparation admission. Review fixed tiny-clip fade changes, crop leakage,
dense policy inventory rejection and unintended muting of Preserve decay.
That qualification covers the engine milestone. The subsequent
[pause insertion](INSERT_TIME.md) command connects it to native authoring.

Current measured evidence: [editing foundation verification](FOUNDATION_VERIFICATION.md), [picture plan and migration verification](PLAN_MIGRATION_VERIFICATION.md), [exact boundary verification](ANCHOR_VERIFICATION.md), [persistent mark verification](MARK_VERIFICATION.md), [sparse override verification](OVERRIDE_VERIFICATION.md), [nested occurrence verification](OCCURRENCE_VERIFICATION.md), [native media qualification](qualification/media-2026-09-20.md), [compatible FFmpeg/Rust qualification](qualification/media-compatible-2026-09-20.md), [audio candidate qualification](qualification/audio-2026-09-20.md), and [canonical audio qualification](qualification/audio-canonical-2026-09-20.md).

The [native host conversion qualification](qualification/media-host-conversion-2026-09-21.md)
adds actual sampled/native model-output conversion, exact RGB comparisons,
bounded failure tests, and adapter sanitizer evidence. Complete media/provenance bundles are now implemented through
[host qualification and Ready storage](GENERATION_BUNDLES.md).
Schema 9 adds [explicit durable acceptance](GENERATION_ACCEPTANCE.md), measured
source spans and six-object admission. Application acceptance remains open.
The [bridge sampling qualification](qualification/media-bridge-2026-09-21.md)
adds paired native/sampled masters from one immutable input, exact integer
interpolation, and comparison with the earlier captured-model pixels. This
validates media derivation; it does not qualify candidate acceptance or seams.
The [native bundle qualification](qualification/model-bundle-2026-09-21.md) adds
a real protocol-2 local generation, required provenance binding, exact host-derived
masters, three-object publication/readback, and schema-8 Ready persistence.
That earlier run did not exercise authored acceptance or application integration.
Host qualification also retains the exact context manifest and both prepared
input byte streams before generation, binds them to worker provenance, and
returns all six objects for durable publication. [Retained conditioning evidence](qualification/conditioning-2026-09-21.md)
records a fresh local model run and relocated six-object readback. This is byte provenance, not
source-clock, image-decoding, color or seam qualification. Admission-bearing Ready
and acceptance now verify all six objects. A captured real generation has been
accepted, relocated, undone/redone/reverted and independently decoded in a
[developer probe](qualification/acceptance-2026-09-21.md). History reference
inventory and application rendering remain open.

[Source preview qualification](qualification/source-preview-2026-09-21.md) adds
persistent source decoding, identity-verified measured indexes, a shared SDR GPU
baseline and a native preview with frame navigation. Actual Metal comparisons,
source/session sanitizer tests and native visual/keyboard observations are
recorded separately. Project import, editorial playback and export remain open.

[Video admission qualification](qualification/video-admission-2026-09-21.md)
adds closed MP4/Matroska checks before demux allocation, FFV1 configuration
expansion limits, controlled first-frame probing and per-packet bounds. It
preserves the measured source fixtures and an accepted generated master. The
full required format matrix and authored import remain open.

[Original ownership qualification](qualification/original-media-2026-09-21.md)
adds managed complete-file retention, APFS clone/copy, linked locations,
identity-checked relinking, private snapshots and schema-10 migration. A bounded
native audio inventory reports observed metadata; that earlier ownership change
did not establish decoded audio bounds or complete authored import.

[Source audio](SOURCE_AUDIO.md) now provides a separate persistent AAC/PCM
decoder, identity-bound measured sample indexes and bounded private PCM range
reads. Video/audio share one verified original snapshot. Explicit skip/discard
and measured terminal-duration evidence remain distinct from unknown priming and
container claims. [Qualification](qualification/source-audio-2026-09-21.md)
records exact PCM and AAC fixtures. The [independent audio mapping](SOURCE_AUDIO_MAPPING.md) represents exact natural-rate
durations and offsets, with reversible commands and schema-11 history migration.
[Mapping qualification](qualification/audio-mapping-2026-09-21.md) records exact
boundaries, headless history behavior and old-binary fixture reproduction.
[Exact source picture mapping](SOURCE_VIDEO_MAPPING.md) now separates original
video speed from rounded beat duration, retaining selected-span endpoint policy
in picture plans and exact inverse anchors. Reversible mapping commands and
schema-12 migration preserve prior audio and picture decisions.
[Picture-mapping qualification](qualification/video-mapping-2026-09-21.md) records
VFR boundary cases, actual decoded pixels and old-binary migration evidence.
Authored import, resampling, playback, device output and full format qualification remain open.

[Measured import timing](SOURCE_IMPORT_TIMING.md) adds exact independent stream
starts, common-origin candidates, complete available-audio preservation, outward
source enclosure and measured cadence/geometry candidates. Core schema 8 and
database schema 13 retain earlier mappings through frozen history replay.
[Import-timing qualification](qualification/import-timing-2026-09-21.md) records
actual media and migration evidence. These candidates do not establish import
readiness or adopt a project's presentation basis.

[Source registration](SOURCE_REGISTRATION.md) adds live selected-stream
qualification, durable measured indexes and revision-bound receipts, plus atomic
asset registration and optional full-source insertion through the headless host.
Core schema 9/database schema 14 preserve older histories without inventing source
evidence. [Qualification](qualification/source-registration-2026-09-21.md) records
actual-media, rollback, relocation, historical alias reuse and migration checks.
The native workspace connects registration and explicit whole-source insertion;
complete import/relink/format acceptance remains open.

[Automatic presentation basis](PRESENTATION_BASIS.md) selects measured cadence
and geometry on the first primary picture insertion in an untimed project.
Audio or other timed editing locks the existing clock; later primary geometry
adoption is a separate previewable, undoable transaction that preserves timing.
Core schema 15/database schema 21 retain this policy and migrate older histories
as explicit choices. [Qualification](qualification/presentation-basis-2026-09-21.md)
records real-media insertion, unchanged sample/mark coordinates, rollback,
headless previews and authentic schema-14 history migration. Native canvas
previews, framing-effect reevaluation and rendered-output checks remain open.

[Background import preparation](IMPORT_PREPARATION.md) separates complete-file
verification and source receipt preparation from the project writer. Opaque
results are tied to a live writer session; final namespace/version checks reject
changed originals. Current-revision insertion and atomic persistence retain the
existing command path. The native workspace now connects bounded import preparation; measured large-media
latency remains open. The [qualification report](qualification/import-preparation-2026-09-21.md)
records the headless concurrency, freshness and rollback evidence.

[Native project workspace](NATIVE_WORKSPACE.md) connects creation/reopen,
managed/linked import, explicit whole-source insertion, saved history and exact
Source/Sequence frame inspection. Root-beat Repeat wrapping/setters, deletion and
existing Hold-duration edits now share the service/core/store command path.
The [root editing qualification](qualification/native-editing-2026-09-23.md)
records durable edit/history, selection, concurrent import and native keyboard
checks, including pointer/text-focus regressions.
The [initial qualification report](qualification/native-workspace-2026-09-21.md)
records actual media, deterministic service interleavings, headless keyboard/focus
checks and native panel/picture observations. Full editing, playback, generated
provider preview, export and release acceptance remain open.

[Single-original initialization](SINGLE_ORIGINAL.md) adds the optional schema-17
profile, full measured baseline and protected undo floor, with strict historical
validation. The native host creates these projects in the system Documents
library and separates Original identity from audio-only catalog registration.
[Profile tests](../crates/deadpan-store/tests/single_source.rs) exercise baseline,
history, rollback, immutable Original and forged-profile rejection. Native
[service tests](../crates/deadpan-app/src/project/tests.rs) cover creation, sound
selection, recovery and backed-up opening of authentic schema-16 projects.
The [single-Original review](qualification/single-original-2026-09-23.md) records
the actual native workflow, imagegen comparison and verification limits.
Generic projects migrate without invented profiles. Full SFX placement/mixing,
range reuse and the remaining editorial session are still required (YouTube
acquisition is now [headless and native](YOUTUBE_IMPORT.md)); audio-only sequential insertion is not a sound overlay.

[Transparent partitions](AUDIO_PARTITIONS.md) add a retained audio-context
building block for splices. Core 12/database 18 separate allocation, exact source
filter support and full envelope width; ordinary authored trims retain their
old meaning. Real PCM tests compare retained partitions against the original,
including fractional rates, short fades, nested pitch stages and RoomTone.
The [qualification report](qualification/audio-partitions-2026-09-23.md) records
tests and review for that layer. The later Split implementation uses these
retained contexts. InsertTime now applies exact resume anchors to supported root
Source/Hold splices; arbitrary nested insertion remains open.

[Preview presentation](qualification/preview-presentation-2026-09-21.md) separates
requested, decoded and displayed identities, retains pending GPU work, rejects
late replies and labels the submitted Source/Sequence position. Headless
interleavings, real decoded freeze frames and native keyboard/appearance checks
cover this boundary. Continuous playback scheduling, full accessibility, physical
display timing and forced GPU-loss qualification remain open.

[Structural audio plans](AUDIO_PLAN.md) now expose bounded sample intervals with
original coordinates and distinct authored policies. The [native DSP boundary](AUDIO_DSP.md)
binds the canonical stretch schedule to owned Rust PCM. [Qualification](qualification/audio-foundation-2026-09-21.md)
records independent interval references, 50 prior PCM hash matches, native ABI
sanitizers and the 648-test repository gate. [Source preparation](AUDIO_PREPARATION.md)
now adds exact-phase resampling, explicit stereo speaker mixing and full-index
verification of original PCM sessions. Its [qualification](qualification/audio-preparation-2026-09-21.md)
records real PCM/AAC tests, signal checks and measured worker cost.
[Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) now connects immutable spans to
historical source receipts and original bytes, including silent Holds, repeats,
fractional placements and FollowSpeed retimes. Its
[qualification](qualification/sequence-audio-2026-09-21.md) records real CLI/app
headless parity and cold-reader identity after undo and asset-alias reuse. The
[exact-rate DSP extension](qualification/audio-exact-rate-2026-09-21.md) now keeps
consumption speed independent of rounded storage/output counts.
[Continuous stage preparation](AUDIO_STAGE_PREPARATION.md) connects that adapter
through seams, nested retimes, exact fractional grids and immutable source
provenance. Its [qualification](qualification/audio-stages-2026-09-21.md) covers
PCM parity, Hold policies, cache changes and shared work admission.
[Room-tone preparation](ROOM_TONE_AUDIO.md) now loops explicit retained source
ranges through exact short crossfades, including retimes and repeat gaps. Full
voice processing, native audio playback, effects and export remain open.
[Prepared output](AUDIO_OUTPUT.md) adds a bounded generation-aware queue and a
narrow macOS device boundary, with explicit prepare/activate, starvation and
fault states. Its [hardware qualification](qualification/audio-output-2026-09-21.md)
is separate from application playback, full device/lifecycle coverage and
acoustic or stress acceptance.

[Informational audio measurement](AUDIO_METERING.md) adds shared integrated
loudness and true-peak meters, generated standard cases and an independent
[reference qualification](qualification/audio-metering-2026-09-21.md). Meters
preserve PCM; the failed limiter prototypes remain historical evidence. The
current [limited reader](AUDIO_MASTERING.md) separately applies gain and reports
reduction through the headless API. Reduction UI and final mix integration remain
open.

[Worker cleanup verification](qualification/worker-cleanup-2026-09-21.md) records
a failed macOS CI run and the shared membership-confirmed teardown for media
and model workers. Deterministic signal/ownership/pipe regressions and actual
forking descendants cover these fixes; process-group cleanup is not a sandbox.
[Conditioned limiter research](qualification/audio-limiter-masks-2026-09-21.md)
retains a 42-fixture pass and two subsequent suppression-mask counterexamples.
The prototype remains unadopted; this does not advance mastering acceptance.
[Post-mask gain research](qualification/audio-limiter-postmask-2026-09-22.md)
retains 16 further outputs and a complete-sinc audit. Two distinct rapid-mask
outputs still fail the broader reconstruction check despite passing both finite
meters. Fixed fades that excessively quiet tiny fragments are rejected as a
default; production limiting remains open. The shared edge stage described
below uses shortened fades with an explicit tiny-fragment contract.

[Audio boundary provenance](AUDIO_PLAN.md) now retains exact structural,
placement and Repeat-gap edge owners, including coincident constraints and each
owner's own occurrence path. Root query crops preserve this metadata, and copy
work is bounded. [Authored edge policies and shared fades](AUDIO_EDGES.md) add
reversible node/occurrence commands, strict core-11/database-16 migration, and
sample-centered 2 ms fades shortened within each root allocation after continuous
time mapping. The explicit headless stage preserves silence and read partition
invariance. Native controls, listening qualification, simultaneous voices,
downstream effects/mastering, playback and export remain open.

## Product requirements

[Structural speed editing](RETIME_EDITING.md) adds exact native Retime creation
and parameter adjustment with explicit preserve/tape pitch, retained input range,
atomic history and a closed core-28/database-34 migration boundary. Full range
operators, variable speed, independent pitch, rendered GUI acceptance and encoded
preview/export equivalence remain required; this does not complete DP-07 or DP-09.

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Documents library, one-Original initialization/baseline, reopen, autosave, undo/redo, migration, recovery. | Partial | [`deadpan-store`](../crates/deadpan-store/): durable packages/history, atomic mark transforms and generation relevance, writer ownership, WAL checkpoints and interrupted-attempt recovery. [Current development formats](DEVELOPMENT_FORMATS.md) reject obsolete development formats before writes, and the native app now explains the refusal. [Recovery](RECOVERY.md): writer session markers detect an unclean exit ([process-kill test](../crates/deadpan-store/tests/recovery.rs)), `open_recovery` reports interrupted render/publication/generation attempts, the app offers to reopen the project a crashed launch left open and shows what was recovered, a missing Original opens degraded and is relinked/restored only from identical content, and real ENOSPC/EROFS/EACCES failures on APFS disk images leave the project valid with truthful "Not saved" messaging ([store](../crates/deadpan-store/tests/storage_failures.rs), [app](../crates/deadpan-app/src/project/tests/recovery.rs), destination publication). Closing with open previews asks first. Replays `recovery`, `relink` and `storage-failure` pass 37 checks (2026-10-05). | Native create/open/history have [workspace evidence](qualification/native-workspace-2026-09-21.md). Rotating automatic backups, a read-only view of newer schemas, release migration policy, bookmarks, history limits, ENOSPC coverage for proxies/model downloads/generated media, physical power loss and the full failure/chaos suite remain open. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Partial | Typed rational clocks, VFR intervals, [independent picture mappings](SOURCE_VIDEO_MAPPING.md) and explicit selected-span endpoints in core and plan. [Original moment candidates](SOURCE_MOMENTS.md) retain exact VFR and fractional-sample selection boundaries. [`SourceSession`](../crates/deadpan-media/src/source_session.rs) builds original-PTS indexes from private verified media and performs persistent exact seeks. [Registration](SOURCE_REGISTRATION.md) retains validated indexes and exact common origin by historical revision. [Native source evidence](qualification/source-preview-2026-09-21.md) retains measured VFR terminal-duration loss. | Complete source policies and actual shared playback/export, including 10,000 fractional-rate edits. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | Validated tree, reversible commands, and [`deadpan-plan`](../crates/deadpan-plan/) picture mapping and [bounded structural audio queries](AUDIO_PLAN.md) through nested primitives, sparse overrides and compact repeat indexes. Audio keeps absolute sample allocation, original source coordinates, pitch stages and distinct Hold policies. | Semantic range selectors, incremental fragment reuse, actual golden picture/audio renders, and full preview/export integration. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Partial | Compact stable play IDs and exact revision-aware boundary/range queries. [`marks.rs`](../crates/deadpan-core/src/marks.rs) adds persistent marks, ownership/loss policies, biased structural transforms, and named-mark selection with [integration/property tests](../crates/deadpan-core/tests/marks.rs). [Sparse overrides](OVERRIDE_VERIFICATION.md) and [automatic nested occurrence edits](OCCURRENCE_VERIFICATION.md) preserve variable durations, owned marks, exact picture mappings, and atomic history. [Independent sound clocks](OWNED_SOUND_VOICES.md#copying-retained-sound-clocks) retain attached PCM through supported ordinary Sequence moves and whole-owner copies, with fresh historical aliases, durable registers/history and current gate/gain policy. | Complete temporal attachment transforms, partial-range and multi-target occurrence operations, explode/duplicate transforms, and complete structural edit property tests. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Partial | [Native workspace](NATIVE_WORKSPACE.md) adds counted navigation, persistent prefixes, pane focus, search, command entry, text/IME suppression, Sequence Enter/Backspace and breadcrumbs, current-depth `s`/`rr`/`dd`, typed `y`/`d` frame, beat and group-boundary selectors with `yy` whole-beat copy and [group objects](STRUCTURAL_SELECTIONS.md#group-object-workflow) for `ig`/`ag` operators and Visual editing, [counted frame cuts](EDITED_SLICES.md#frame-cuts-at-the-cursor), exact Hold commands and history. [Bounded explicit Repeat input](qualification/repeat-input-2026-09-27.md) retains eight batched wraps as separate commits and undo steps, preserving partial keys and reporting overflow/cancellation. [Same-frame footer and text focus](qualification/footer-layout-2026-09-27.md) preserve final text through immediate Escape and combined resize/input, keep pending keys single, and anchor the painted mode before presentation. `,i` reuses the Original without taking Kestrel's Cmd+Return. [Expanded UI qualification](qualification/workspace-layout-2026-09-27.md) checks 3,472 production routing cases against 62 global reservations and replays real editing/focus, Original moment, audition and Retime paths with visible controls and hints. | Full editing grammar, nested occurrence navigation, native IME/layout and physical key delivery coverage, and keyboard-only editorial session. |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Partial | [Persistent registers](NAMED_REGISTERS.md) retain Original ranges, edited slices and Macro programs across edits, Undo and reopen. [Semantic macros](SEMANTIC_MACROS.md) record frame/beat/group motion, Visual selections, frame/beat/range cuts, beat/range/group copies, typed operator-motion and `ig`/`ag` selectors, Original or Edited paste/replacement and named calls. They expand with bounded work and commit one [resolved Compound](COMPOUND_TRANSACTIONS.md) with retained intermediate captures; copy-only runs preserve timeline history. [Headless Macro commands](SEMANTIC_MACROS.md#headless-inspection-save-and-run) inspect, save, preview and execute against explicit revision/bank/context in closed and live projects. [Semantic cut repeat](SEMANTIC_REPEAT.md) re-resolves frame, beat, motion or current Visual targets while retaining the requested count/direction and register; exact saved proofs survive refresh failure. [Configurable Normal/Visual bindings](KEYMAP.md) share bounded loading/compilation, execution metadata, prefix teaching and audit enumeration; invalid files retain the shipped map with a diagnostic. Macros also record pauses (`,h`, `:hold`) as exact `InsertPause` lengths, framing (`,z`, `,c`) and [gag recipes](GAGS.md), and gain/saturation changes, J-/L-cuts and role-only deletes; `.` repeats those creative edits, reverses, tails and gags with their exact parameters at the new selection ([semantic repeat](SEMANTIC_REPEAT.md)). The native `,h`, `:hold-duration`, `:retime`, `:pitch`, `+`/`-` (also over an Edit range), `:gain`, `,m`, `:audio-lag`, whole-beat `,z`/`,c`, cutaways and captions (beat or range), `:edge` and `:gag-set` now commit through that semantic Apply outside recordings too, so each is recordable and dot-repeatable with one Undo (`creative-dot` replay). | Remaining Macro editing instructions (sound placement, the room-tone sheet, Trim/Slip, ranged and one-play framing), beat/analysis/role selectors and temporal occurrence scopes; dot-repeat of one-play edits and ranged or face zoom; remaining mode maps, strict logical provenance, physical layout/IME qualification and full parser/transaction/replay acceptance. |
| DP-07 | All time/delivery operations in Section 8. | Partial | Core commands insert/delete/move/group/ungroup nodes, wrap/update structural repeats and [Retimes](RETIME_EDITING.md), and change Hold duration/provider. [Atomic linked range moves](ATOMIC_MOVES.md) retain whole-unit identities and exact audio entries within and between ordinary Sequence parents; native `:splice` previews both joins and selects the complete committed range. Pure Split retains complete contexts and logical marks. Native current-depth Split/Repeat/Retime/delete/Hold-duration edits use that path and refresh the stopped-frame picture plan. Atomic InsertTime retains sample phase through Source/Hold fragments under ordinary Sequence groups and freezes a measured original picture below live ancestors. [Repeat escalation](REPEAT_ESCALATION.md) adds exact per-play gain and centered scale steps (additive or multiplicative) to an editable Repeat through one reversible `SetRepeatEscalation`, validated against every play, applied by the shared picture plan and authored audio bus, and set natively with `:repeat 3 gain-step=3dB zoom-step=0.08` (core, plan, real-PCM and replay tests). [Gag recipes](GAGS.md): `:gag long-answer`, `escalator`, `non-sequitur`, `one-more-time` (gaps that shorten after each play) and `nothing-happens` (room tone from a copied Original moment cut to true silence) expand versioned recipes to ordinary beats under a group pinning recipe, version and parameters, as one Undo (core expansion/inverse and `gags`/`recipes` replay tests). `SetRepeatGaps` sets a Repeat's default gap and independent gap Holds as one retained-clock transaction, and `:repeat N gap=120ms gap-step=-40ms gain-step=3dB zoom-step=0.08` changes count, gaps and steps together (or wraps a plain beat) as one recorded instruction; the One More Time recipe export matches its preview. [Reversed pauses](REVERSE_AND_TAILS.md): `:reverse 8f` (reverse hiccup) and `:ping-pong 12f` insert a Hold whose `Reverse` picture and sound play the preceding Original passage backwards (the ping-pong without repeating its turning picture), resolved by the shared pause resolver, recordable, and exported with their reversed click located independently. `:gag are-we-done` adds the sixth recipe. `:lift` cuts a Visual range and refills its time with a silent black pause, and `,b`/`:bleep` refills it with the same pictures played forward over a synthesized tone (`HoldVideo::Play`, `HoldAudio::Tone`); both are one recorded compound and export-verified. `:gag-inspect` lists a recipe's exact expansion before applying it, One More Time takes seeded variation (`vary=20% seed=7`) whose seed is pinned and whose resolved gaps are stored, and `:recipe-save a` / `:recipe a` / `:recipe-inspect a` keep a modified group as a project-local recipe ([gags](GAGS.md); `recipe-library` replay; the varied export matches its preview). Black-frame punctuation and the remaining rows are tracked in [Section 8 coverage](SECTION8_COVERAGE.md). | Reversing composite structure or at other speeds, variable rate, arbitrary nested/gapped Hold insertion, compact occurrence resume dispatch, nested range planning, remaining operations, semantic targeting, recipe fixture renders, and editable inspector demos. |
| DP-08 | All framing/picture operations and keyboard target selection. | Partial | [Authored framing and Camera](FRAMING.md): static/enveloped operations, exact owner clocks, intermediate clipping, counted pan/zoom, numeric fields and a numbered picker of saved targets, center and corners; Camera follows a picked target (`t`), adjusts a follow's scale with a plan-exact preview (outer follows re-resolved; the `targets` replay checks preview equals committed picture), refuses center nudges while following, and draws or corrects target rectangles with the keyboard ([framing](FRAMING.md#keyboard-target-rectangles), [targets](TARGETS.md#in-the-native-app)); [earlier qualification](qualification/framing-2026-09-24.md) includes actual Metal/CPU comparisons, migration and native review. [Captured pauses](CAPTURED_FRAMING.md) retain input composition separately from live framing; [their evidence](qualification/captured-framing-2026-09-26.md) records remaining GPU/native checks. [Canvas transactions](PRESENTATION_BASIS.md) preserve time and reevaluate normalized framing. Escalating crop: a Repeat's centered per-play scale layer composes inside its own framing ([Repeat escalation](REPEAT_ESCALATION.md)). [Cutaways](CUTAWAYS.md): picture-only attachments on a host beat show an exact Original selection (hold, loop or gap fit) through the shared picture plan while the host's sound is untouched; they follow Split, copies, isolation, deletion and Trim prefix growth, and `:cutaway register=r` places one over the Edit range (core, plan split-survival and replay tests). [Zoom and creep](FRAMING.md#zoom-and-creep-commands): `,z` punches in to 1.35× following the selected target (center fallback with a message), `:zoom S target=current\|center\|ID curve=step\|linear\|smoothstep` is a smash zoom or eased change, `:creep from= to= target=` eases toward the target's plan-exact position where the creep arrives, `:zoom off` is an abrupt return, and each applies to the Edit range inside the beat; a follow keeps following on a scale change and a camera path is replaced only by an explicit `target=`, as one `SetFraming` captured at command entry (pure construction and capture unit tests and the `zoom` replay). Macros record whole-beat framing and black pauses (`InsertPause { black }`); `:framing-save a` keeps the selected beat's framing (for example an off-center stare) as a one-instruction macro that `@a` applies to another beat, and a static off-center export matches its preview. Black-frame punctuation: `:hold 12f video=black` inserts a silent Background pause ([pause insertion](INSERT_TIME.md); service test and replay). [Captions](CAPTIONS.md): one-line text attachments on Source/Hold hosts with a delay and per-play reveal, rasterized from the bundled Inter font and composited by the shared GPU pass before display and encoder readback in the viewer and export (`:caption`, recordable; Metal composite test, viewer readback in the `captions` replay, and an export that a caption-free revision flags exactly where the caption was). `:zoom S target=face:N` and `:creep … target=face:N` run Apple Vision face detection on the displayed Original picture in the process-isolated `deadpan-track` worker; faces are proposals ordered left to right, and only the chosen one is saved as a target together with the framing in one Compound and one Undo when the captured context is unchanged ([framing](FRAMING.md#face-proposals); `faces` replay with scripted faces through the real service, real Vision on a drawn-face fixture). Video-only deletes (`:delete role=video`) show the background through a removed-picture cutaway, and J-/L-cuts keep every picture through a cutaway over their rolled stretch ([role edits](ROLE_EDITS.md)). [Section 8 coverage](SECTION8_COVERAGE.md) inventories every operation. | Point targets, target renaming/deletion and pointer dragging in the app, region detection other than faces and detected faces in the Camera picker, a creep that keeps following a moving target, letterbox-corrected follow centers, per-play escalation, native occurrence selection, equivalent framed Ungroup, Metal/native acceptance for captured pauses, complete Section 8 effects and preview/export acceptance. |
| DP-09 | All audio operations with preserved intentional dynamics. | Partial | Limited [Original/edit/sound audition](PLAYBACK.md) connects canonical PCM to the native device. [Root sound events](SOUND_EVENTS.md#persisted-root-sounds) have qualified durable commands, sample-exact overlays and per-voice gain before the shared limiter; chronological root ripple edits preserve sample phase; [native root placement](qualification/native-sound-placement-2026-09-27.md) adds exact movement, gain, edges and removal. [Beat-owned sounds and independent clocks](OWNED_SOUND_VOICES.md) add qualified owner-local recipes, independent occurrence processing and exact PCM transport for unchanged scopes through supported Sequence edits, whole-owner copies and recopies. Remaining structural transforms stay open. The shared [finite oversampled limiter](AUDIO_MASTERING.md) verifies emitted tiles and preserves source-aware context across reads; full audio authoring remains open. [Raw DSP qualification](qualification/audio-2026-09-20.md) retains failed targets. The [canonical worker prototype](qualification/audio-canonical-2026-09-20.md) supplies the single schedule now used by the bounded [production DSP adapter](AUDIO_DSP.md). [Source preparation](AUDIO_PREPARATION.md) implements exact-phase resampling and explicit matrices without loudness normalization. [Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) binds exact spans to historical qualified media, including repeats, silent Holds and FollowSpeed retimes. [Continuous Preserve stages](AUDIO_STAGE_PREPARATION.md) retain exact fractional grids and nested history with bounded preparation and source/layout-aware caches. [Room-tone loops](ROOM_TONE_AUDIO.md) use explicit ranges and exact crossfades, with [qualification](qualification/room-tone-audio-2026-09-21.md). [Authored edges](AUDIO_EDGES.md) add reversible hard exceptions and shared post-mapping fades. Per-play escalation gain is added on the Repeat owner before mixing and limiting ([Repeat escalation](REPEAT_ESCALATION.md)). Native `,m` mutes a Visual range inside a beat or the whole beat, `:audio-lag` sets a Source's explicit audio offset, `:sound-cut` ends a placed sound abruptly at the Edit cursor (bed drop) and `:gag nothing-happens` authors room tone then silence; mute-range, audio-lag, bed-drop and room-tone exports match their previews with independently located sound ([preview/export](PREVIEW_EXPORT_VERIFICATION.md)). [Hanging tails](REVERSE_AND_TAILS.md): `HoldAudio::Tail` renders a deterministic per-channel comb/all-pass reverb (RT60 measured 1.09 s) or 300 ms echo of the processed Original sound heard over the two seconds before it, read live from the current plan at render time, ringing for its maximum and fading to exact silence, as one canonical cached Hold block (`,t`, `:tail`, recordable); reversed Hold sound uses the same preparation; reverb and echo exports match their previews. A serialized [saturation stage](AUDIO_GAIN.md#saturation) (`:saturate 12dB`, tanh after clip gain by default, per voice before the limiter), [fixed pitch shift](RETIME_EDITING.md#pitch-shift) (`:pitch +3st`, `PitchPolicy::Shift` on the canonical processor at any speed), [J- and L-cuts](ROLE_EDITS.md#j-and-l-cuts) (`:jcut`/`:lcut`: a Roll plus a picture-keeping cutaway) and [role-only deletes](ROLE_EDITS.md#role-only-deletes) (`:delete role=audio\|video`, `:select role=`) are recordable semantic instructions with real-PCM or plan tests, replays and export fixtures that match their previews. | Effect sends from arbitrary beats and sounds, room-tone waveform and Repeat-gap/fragment authoring, full voice processing, authored layout choice, full signal/format/listening corpus, remaining fade integration, gain envelopes/tails and nested mixing, all remaining audio operations, preview/export equivalence, devices, long-clip preparation and cache lifecycle. |
| DP-10 | Local transcript, timing refinement, shot/silence proposals. | Partial | [Local transcription](TRANSCRIPTION.md): validated word-timed transcripts with exact Original timing and search; a process-isolated whisper.cpp 1.8.3/Metal worker under the shared supervisor with verified model/PCM inputs, hashed artifact admission and real cancellation; analysis PCM from the Original's retained audio through the canonical resampler; schema-59 storage outside history; `transcribe`/`transcript` commands; automatic app transcription with a rail transcript, search and exact picture jumps; and word/sentence motions (`w b e W B`) in Original and Your edit with `iw aw is as` objects composing after `d y r`, resolved by projecting the transcript through each staged document, `/` with `n`/`N` transcript search in both contexts, and the rail's Your edit transcript in playback order (core, planner, projection and replay tests). [Qualification](qualification/transcription-2026-10-04.md) includes a native run. [Speech activity](SPEECH_ACTIVITY.md): Silero VAD v6.2.0 through the same worker and analysis PCM (protocol 2 `DetectSpeech`), measured 10 ms energies, the `deadpan-silence-1` pause rule, schema-60 storage outside history (additive upgrade from 59), a `pauses` command, automatic app detection including Originals already transcribed, and pack version 2 reusing an installed recognizer; on a real 18 s interview the rule reports all nine 180–220 ms between-phrase pauses within 10–30 ms of their energy silences. Pauses project onto Your edit like words (quiet Original pictures plus freezes, generated pictures and gaps) and drive `]p`/`[p` motions and `ip`/`ap` objects composing after `d y r`, with each analysis reporting its own absence (core, projection and replay tests, including `dip`). [Shot detection](SHOT_DETECTION.md): every Original picture decoded exactly against its qualified index from the verified snapshot and reduced to a grid/histogram signature with three-byte changes (including a skip-one comparison that rejects returning flashes), the `deadpan-shots-1` rule, schema-61 storage outside history, `detect-shots`/`shots` commands, an automatic app scan with the count on the Original card, and `]s`/`[s` motions and `iS`/`aS` objects over projected shot occurrences; a synthesized 1080p clip's five hard cuts are found at the exact picture, a returning flash is rejected, and the release scan measures 69 pictures/s at 1080p24 (decode-bound) (analysis, media, store, CLI, service and replay tests including `diS`). | Original operators with word motions, scheduling by visible range, word refinement from speech activity, pause rule qualification on varied real speech, manual pause and transcript correction, pause display in the rail, non-US physical delivery of `[`/`]`, dissolve/fade detection and shot qualification on real edited footage, a faster or resumable picture scan for long Originals, and accuracy on real speech. |
| DP-11 | Selected target tracking with manual correction and loss handling. | Partial | [Selected-target tracking](TRACKING.md): a process-isolated `deadpan-track` worker decodes the verified Original through the pinned descriptor-only decoder and runs Apple Vision `VNTrackObjectRequest` under the shared supervisor with hashed-artifact admission, bounded cancellation/deadline drain and an exact check of every decoded and analysed PTS; the pure `deadpan-track-1` policy stops at the first stored shot boundary, measures motion in the display aspect, rejects low-confidence and implausible jumps instead of following another subject, interpolates gaps of at most six pictures, marks longer gaps lost and holds the last confident position until a manual keyframe, and applies corrections all-or-nothing to their range only (unit and property tests). `track --save` maps the path to a core [attention target](TARGETS.md) with stated rounding, state and confidence rules, provenance and tolerance-bounded compaction (core `region_at` now interpolates moving samples), committed as reversible `SetTarget` against the head captured before tracking (stale heads refused, existing ids only with `--replace`); `track-correct` re-tracks only a correction's range. On a synthetic moving-square fixture with occlusion and a hard cut the path is within 1 px, marked lost behind the occluder, stopped at the cut, never jumps to the second square; the saved target reloads, reproduces the path, resolves a `Follow` layer in `RenderPlan`, and a correction changes only its range. [In the app](TRACKING.md#in-the-app), Camera `T` and `:track` run the same host code on one bounded, cancellable job thread drained on close/shutdown, saving against the head captured at command entry; Camera `c` corrects at the displayed picture and re-tracks only that range; service tests and the `targets` replay use a scripted Vision seam with real range, policy and saves. | Real-person footage for tracking quality (spec Section 25); rotated/anamorphic real media; saving through the open app's live endpoint exercised; the app's real-worker path exercised in tests or replay; incremental scheduling; point targets and face/region proposals. |
| DP-12 | Local AI hold generation, exact seams/duration, variants, acceptance. | Partial | A [real supervised MLX development adapter](qualification/model-worker-2026-09-21.md) uses exact bridge planning and interior sampling, with decoded-file timing/color/hash checks. [Generated Hold semantics](GENERATED_HOLDS.md) retain sampling and resize fallback. [Dedicated store acceptance](GENERATION_ACCEPTANCE.md) binds the selected Ready receipt, retained inputs and derived assets to one reversible edit. Generic ingress remains guarded. [Accepted picture admission](qualification/generated-pictures-2026-09-29.md) decodes retained schema-3 Generated Holds without a model in native and captured-revision readers; [The headless AI pause chain](AI_HOLDS.md) runs conditioning from the Hold's boundary pictures, the supervised development LTX MLX worker, bundle qualification, publication, Ready and explicit `accept-hold`; a measured real run generated, accepted and rendered a 30-frame bridge with a symmetric canvas raster policy, and ordinary edits keep pending requests reconciled through a boundary-context resolver. The [native app workflow](AI_HOLDS.md#native-app-workflow) runs conditioning and the real worker on one bounded job thread per project with writer-applied records, shows stage/step/elapsed progress and the runtime's own unavailable text, reads Ready candidates from the store, previews them through the shared generated-picture path without an edit, and accepts through one undoable edit (`,a`, `:generate`, `:cancel-ai`, `:preview-ai`, `:accept-ai`, `:discard-ai`); session changes drain the job before releasing the writer. A real app-service run reached Ready in 96 s and accepted; the `ai-pause` (scripted worker) and `ai-pause-ready` (real bundles) replays pass. [Variants](AI_HOLDS.md#native-app-workflow): every attempt of a request is a seeded variant (`ProviderSelection::for_attempt`, checked by the store against the attempt ordinal); `:generate N` runs up to four attempts in one job and adds them to the Hold's current request while its boundary pictures are unchanged; the inspector lists variants with thumbnails decoded off the UI thread from each sampled master; `:next-ai`/`:prev-ai`/`:pick-ai N` choose through the store's selection; Discard is the durable eviction (not undoable). `:audition-ai` and playback while previewing play the proposed acceptance document, admitted against the exact committed revision (`Snapshot::proposed_generated`), so the pause's own sound plays with the candidate pictures. `generate-hold` (`--variants`, `--another`) and `accept-hold --attempt` route through an open app's live endpoint as the app's own job. A real run generated two variants of one request (Ready at 81.9 s and 173.3 s, consecutive seeds, different masters) and accepted the first; the `ai-variants` replay (synthetic footage through real qualification) and store, service, CLI, live-endpoint and playback tests pass. A packaged `Deadpan.app` now carries the private runtime and uses the installed bridge pack: from a scrubbed copy it generated (76.8–98.9 s), accepted and rendered an AI pause, with decoded pixels identical to the development environment for the same seed ([record](qualification/ai-runtime-2026-10-05.md)). No qualified model corpus. | Source joins, speech preservation, source/color context, side-by-side or same-sample A/B comparison of variants, a retention policy for discarded variants, listening qualification of the audition (replays simulate delivery), native physical-input and VoiceOver checks of the AI controls, and the full qualified model corpus. |
| DP-13 | Model/runtime manager, safe downloads, offline pack installation. | Partial | [Model packs](MODEL_PACKS.md): schema-2 manifests compiled into the app with relative file paths, separate license layers (SPDX or LicenseRef, attribution, link, terms summary, compiled full text, explicit `acceptance_required`) and an approved `ltx-2.3-q4-bridge` pack (31 files, 36.15 GB, LTX-2 Community License and Gemma Terms of Use) matching the worker's qualified receipt; HTTPS range-resumable staging with the requested user agent, free-space checks, exact size/SHA-256 verification, license acceptance enforced before any byte is staged and recorded in the receipt, a smoke test before activation (whisper recognition, or the bundled AI runtime's `worker.py --check`), side-by-side versions, an exclusive install lock and cancellable stall-detecting downloads. Offline installation from a folder (APFS clone, every file hashed, symlinks refused) or an uncompressed ustar/pax archive (names compared with the manifest only; links, duplicates, bad checksums and wrong sizes refused) and `export` to that archive form. CLI parity: `models list/license/install/import/export/remove [--partial]` with `--accept-license` and codes `ModelPackLicense`/`ModelPackVerification`. Unit tests cover manifests and licenses, resume, range-ignoring servers, unexpected offsets, stalls, locking, corruption, space, cancellation, tampered receipts, consent refusal, folder and archive import, tamper and hostile-archive refusal and export round trips; CLI tests cover listing, licenses and refusal before staging. A scrubbed copy of the packaged app imported the 36.2 GB pack offline with its bundled smoke test in 12.4–13.4 s ([record](qualification/ai-runtime-2026-10-05.md)). The native Models panel is recorded in [Model packs](MODEL_PACKS.md#in-the-app). | Signed update manifests and pack rollback, a built full offline distribution carrying exported packs, gated-weight flows, an interrupted real 36 GB download, and clean-machine acceptance. |
| DP-14 | One Original from a YouTube URL with bundled JavaScript support. | Partial | [Headless YouTube import](YOUTUBE_IMPORT.md): pure URL normalization (watch/share/Shorts/embed/live/music/mobile hosts to one 11-character ID; playlists only with an explicit `v=`; HTTPS-only, no authority tricks) with property tests; pinned yt-dlp 2026.08.19 (embedded yt-dlp-ejs 0.8.0) and Deno 2.9.7 installed by `downloader install` with exact size/SHA-256 verification, atomic never-overwrite publication and re-verification before every use, `downloader status --probe` and `doctor` versions; supervised yt-dlp runs by absolute path with argument vectors, `--ignore-config`, `--no-plugin-dirs`, no remote components, only the pinned Deno, a cleared private environment and explicit-file cookies copied owner-only and deleted; metadata-first refusal of playlists, live/upcoming streams and inadmissible videos before transfer or package creation; Deadpan's own selection of the best direct H.264 picture and original-language uncompressed AAC sound; stream-copy assembly in the isolated media worker (new `remux` mode, pinned FFmpeg 8.0.3); creation through the shared single-Original path (`project create-original`), and schema-62 private provenance outside history. Unit, worker and stand-in end-to-end tests cover arguments, environment, selection, refusals and error mapping; a real run imported a 146 s 1080p24 CC-BY Blender short as a Ready project in 15 s. [Native New-from-URL](YOUTUBE_IMPORT.md#in-the-app): the start card's URL field and an over-project sheet (`⌘⇧N`, `:youtube`, File menu) with live normalization or the exact refusal, an explicit verified helper install (never silent), title/uploader/length/streams/destination confirmation before any transfer or package, staged progress with cancellation, explicit cookies file, actionable failure codes and opening through the ordinary Open path, on one app-owned job thread split into `inspect`/`download_and_create`. Job, router and library tests use a scripted downloader; the `youtube` UI replay passes 59 checks; env-gated real app-job runs created a Ready project in 16.6–16.7 s. | Thumbnail in the confirmation; physical keyboard, VoiceOver and native picker acceptance of the URL step; signed update manifests and rollback for the [bundled helper baseline](PACKAGING.md#downloader-baseline) (DP-22; no Developer ID by owner decision); VP9/AV1 and above-1080p originals (decoder qualification); clean-machine acceptance; resumable transfer; Linux/Intel helper builds. |
| DP-15 | One local Original plus external audio-only effects, managed/linked assets and relinking. | Partial | [Original ownership](ORIGINAL_MEDIA.md) retains complete originals through APFS clone/verified copy, records linked locations, checks identity on relink, and returns private snapshots. [Source registration](SOURCE_REGISTRATION.md) qualifies explicitly selected streams, retains measured indexes and receipts, and registers/inserts with exact common-origin placements atomically. [Store tests](../crates/deadpan-store/tests/source_registration.rs) cover historical alias reuse, rollback, deduplication, undo/redo and relocation. [Automatic basis tests](../crates/deadpan-store/tests/presentation_basis.rs) cover primary intent, final-rate placement, audio clock locking and geometry adoption. [Background preparation](IMPORT_PREPARATION.md) keeps file verification and receipt preparation independent of the writer and rechecks source freshness at commit. | Native register/insert has [workspace evidence](qualification/native-workspace-2026-09-21.md); single-original initialization retry is implemented; relink and basis-preview UI, bookmark resolution, legacy asset requalification, full sound ownership and full format/failure matrix remain open. [Native root sound placement](qualification/native-sound-placement-2026-09-27.md) is a qualified-catalog subset. |
| DP-16 | Shared realtime/offline renderer, bounded decode and proxy paths. | Partial | Structural picture plans plus persistent source decoding and [`deadpan-render`](../crates/deadpan-render/) shared SDR composition. [Metal qualification](qualification/source-preview-2026-09-21.md) compares 76 synthetic cases with a CPU reference; the app displays real decoded sources and exact plan-driven sequence frames through this pipeline. [Presentation state](qualification/preview-presentation-2026-09-21.md) retains actual sequence/revision identity through decode and GPU delays. Native Original/edit audition adds device-clock picture coalescing, context-bounded selection loops and exact paused-sample resume. The [SDR encoder pixel boundary](qualification/sdr-encoder-pixels-2026-09-28.md) reads the same composed working target into bounded owned Rec.709 I420; 172,260 synthetic codes exactly match its independent reference. [Committed project pictures](PROJECT_PICTURES.md) capture historical source receipts and immutable geometry; actual Metal retains repeated/frozen pixels through live writer edits, undo and redo. [Generated pictures](qualification/generated-pictures-2026-09-29.md) share strict six-object admission, canonical sampled-master decoding and retained geometry between both consumers. [Committed encoder pictures](qualification/export-pictures-2026-09-29.md) preserve that revision and framing through even-raster output, exact rational timestamps and one retained I420 result; actual Metal passes all 118 checks and 31 complete-plane references. [Committed encoded candidates](qualification/encoded-render-2026-09-29.md) now preserve those pictures and canonical PCM through a supervised native MP4 encoder, with bounded complete-plane and absolute-PCM fixture comparisons.  The [preview/export verification](PREVIEW_EXPORT_VERIFICATION.md) harness compares the shared committed picture path (at the SDR encoder pixel boundary) and limited audition bus with decoded public-Render output: sixteen recipe fixtures measured 53.1–62.2 dB minimum luma PSNR and exact zero audio offsets ([qualification](qualification/preview-export-2026-10-04.md)). Interactive serving decoders now use FFmpeg frame/slice threading (one codec thread per core, at most 12). Seeks skip non-reference preroll pictures before the target when every SPS declares a fixed reorder depth and frame-only coding. Index measurement stays single-threaded, so receipts never depend on threading. [Tests](../native/deadpan-source/tests/threaded_seek.rs) compare every returned picture with a sequential single-threaded decode. Interactive preview admits Originals [progressively](PROJECT_PICTURES.md#original-admission-and-ownership): receipt-verified pictures immediately, complete fresh measurement in the background, with mismatch and interruption handled separately. Export keeps complete admission before the first picture ([seek qualification](qualification/seek-2026-10-05.md)). A rebuildable [preview proxy](PROXIES.md) tier covers Originals above 1080p. The isolated media worker encodes an intra-only VideoToolbox H.264 MP4 of at most 1920×1080 under a stall watch with one confirmed-teardown retry. Each picture keeps its Original picture's exact PTS and duration, verified independently along with sampled pixel fidelity and per-channel bias. Proxies are published atomically into the per-user `~/Library/Caches/Deadpan/Proxies` and read in place, never authoritative. Builds pause during playback, renders, battery, Low Power Mode and thermal pressure. The main viewer shows a proxy picture with a visible Proxy chip for a stopped seek, then the exact Original picture for the same request after a 150 ms rest. Proxy pictures never satisfy Camera, Slip or Trim gates, and export, render, verification, conditioning, tracking and thumbnails never read proxies. 4K30 long-GOP warm seek is 8.2 ms p95 through the proxy (Original 208 ms); the exact picture follows 390 ms p95 after the request ([proxy qualification](qualification/proxy-2026-10-05.md)). The shared renderer now also takes RGBA64 PQ/HLG sources into the same working space (1.0 = 203 cd/m²) and converts the composite to Rec.2100 10-bit encoder pixels; 344,520 actual Metal codes match an independent reference (PQ exact, HLG within one) ([HDR pixels](HDR_PIXELS.md)). | Legacy Accepted/Still readers, full format/color matrix, mastered playback, proxy generation resumable by completed ranges, proxy use in playback, exact-picture latency at 4K after rest (390 ms p95), acoustic synchronization, effects, preview/export comparison beyond the sixteen SDR recipe fixtures (Generated Holds, larger media; HDR covers two 42-frame recipes) and stress benchmarks. |
| DP-17 | One-action automatic SDR/HDR YouTube-oriented output. | Partial | The [encoder experiment](qualification/encoder-timing-2026-09-28.md) and [AVFoundation comparison](qualification/native-audio-2026-09-28.md) retain real CFR/Rec.709/AAC files and exact event checks. Without edit lists, FFmpeg reads events 1,024 samples late; AVFoundation loses the opening event and reads later events 1,088 samples early. The reference aligns in both. On 2026-09-28 the user approved edit lists for verified encoder delay, padding and frame reordering; §22.3 retains complete emitted-file timing verification. [Actual renderer-plane H.264 checks](qualification/sdr-encoder-pixels-2026-09-28.md) pass normal/sanitized full-plane comparisons for two video-only fixtures, without extending AAC qualification. The [committed encoder-picture host](EXPORT_PICTURES.md) now supplies immutable revision/range, exact output timestamps, legal raster mapping and bounded real I420 preparation. The [isolated worker](RENDER_WORKER.md) now supplies bounded raw ranges with document binding and clean-exit artifact admission. The [native encoder and offline PCM boundary](qualification/native-encoding-2026-09-29.md) adds exact descriptor-only MP4, historical canonical audio reads, independently measured edit-list timing and fresh-decoder GOP evidence for bounded synthetic fixtures; hardware B-frame rejection remains retained. [Real project encoding](qualification/encoded-render-2026-09-29.md) adds seven isolated MP4 candidates with full fixture decode, exact markers and fresh-GOP evidence. The [production verifier](FINISHED_FILE_VERIFICATION.md) now checks actual clocks/edit lists, complete fresh-IDR GOPs and both AAC decode modes, retaining failed candidates for retry; [current evidence](qualification/finished-file-verification-2026-09-29.md) covers those seven fresh files. The [publication host](RENDER_PUBLICATION.md) adds exclusive destination staging, exact byte readback, a historical local report and atomic movie publication with explicit post-commit outcomes. The [durable render boundary](RENDER_JOBS.md) retains completed encodes across reopen and requires a fresh verification attempt; the [publication journal](RENDER_PUBLICATION.md#durable-publication-journal) adds exact durable rename permits and explicit APFS crash reconciliation. [Recovery qualification](qualification/publication-recovery-2026-09-30.md) covers twenty real process kills and forty fresh verifications. [Automatic admission](AUTOMATIC_ENCODER_ADMISSION.md) and [durable decisions](RENDER_JOBS.md#automatic-admission-and-recovery) now connect runtime-bound qualification to each fresh encode and preserve its original evidence through checkpoint retry and reconciliation. [Native/public Render](RENDER_JOBS.md#native-and-public-render) adds explicit preview decisions, exact committed-revision capture, native progress/cancel and closed-project CLI start/recovery. [Open-project routing](LIVE_PROJECT.md) extends Render to the existing native owner with exact cancellation and temporary-preview refusal. [Native saved-render recovery](qualification/render-history-2026-09-30.md) adds bounded stored-job browsing, checkpoint retry, historical re-encoding and exact destination reconciliation. [Preview/export verification](PREVIEW_EXPORT_VERIFICATION.md) decodes published movies and compares them with their committed revisions; sixteen recipe exports matched frame count, output grid, declared 1,024-sample priming and exact zero measured audio offset ([qualification](qualification/preview-export-2026-10-04.md)). The complete release export path is not qualified. [Automatic HDR output](HDR_OUTPUT.md) adds the branch end to end: HEVC Main10/H.264 High10 PQ and HLG Originals are admitted and decoded to RGBA64, the basis records the Original's transfer, and one receipt-derived decision per committed revision chooses HDR or tone-mapped SDR (BT.2408-style shoulder) for both preview and export. `AutomaticHdrV1` admits hardware HEVC Main10 with tags, a source `mdcv` and a measured `clli` for PQ; the verifier checks HEVC, color boxes, IDR GOPs and content-light bounds. On the M5 Max, PQ and HLG recipe exports measured 59.9 and 60.4 dB minimum luma PSNR in 10-bit code values with zero audio offset; SDR outputs stay byte-identical ([qualification](qualification/hdr-output-2026-10-05.md)). | Full mastering/effects, HDR display/EDR presentation, real camera HDR files, Dolby Vision/HDR10+, expanded raster/content/runtime qualification, and the complete native workflow and emitted-file corpus. |
| DP-18 | Nonblocking worker lifecycle, cancellation, stale result handling. | Partial | [`deadpan-jobs`](../crates/deadpan-jobs/) adds bounded typed framing, a revision-aware attempt lifecycle, native subprocess supervision, and [contained hash-verified snapshots](ARTIFACT_VERIFICATION.md). A [real MLX development worker](qualification/model-worker-2026-09-21.md) exercises this boundary. [Persistent requests](GENERATION_REQUESTS.md) atomically reconcile relevance; [attempts](GENERATION_ATTEMPTS.md) retain retries, validation receipts, candidate selection, and interrupted states across restart. The [render worker](qualification/render-worker-2026-09-29.md) isolates real SDR picture preparation with a separate protocol, exact document binding and clean-exit raw artifact admission. The [encoded child](ENCODED_RENDER.md) adds shared-deadline native picture/audio encoding, hostile-protocol tests and actual cancellation/byte-exhaustion/retry evidence. The separate [finished-file verifier](FINISHED_FILE_VERIFICATION.md) binds its report to private candidate bytes after clean teardown and preserves the candidate when verification fails. The [publication host](RENDER_PUBLICATION.md) preserves the verified candidate and retained paths on precommit failures, while distinguishing a committed movie whose final durability or integrity could not be confirmed. [Durable render attempts](RENDER_JOBS.md) add immutable intent, exact transition tokens, retained movie/manifest checkpoints, restart interruption and fresh verification retries; owner closure cancels live encoder/verifier supervision. The shared native service workflow now qualifies automatic attempts while Queued and atomically binds each selected decision before encoding. [Native recovery](qualification/render-history-2026-09-30.md) binds stored-job queries and recovery actions to exact sessions and historical targets, including current-session status for owner-started work. | Bounded priority scheduling, app-connected inference and its recovery UI, expanded media qualification, application lifecycle, and full concurrency/chaos coverage. |
| DP-19 | Cache integrity and accepted-media portability. | Partial | [Host FFV1 conversion](MEDIA_CONVERSION.md) verifies generated pixels/timing; shared [object storage](ORIGINAL_MEDIA.md) verifies generated objects and complete originals. [Admission](GENERATION_ACCEPTANCE.md) requires six retained objects before Ready/acceptance and derives assets from measured spans. Relocation, undo/redo/revert and independent real-media readback are exercised. [Generated picture qualification](qualification/generated-pictures-2026-09-29.md) adds cold historical decoding after request staleness, exact prefix reuse, revocable worker readers and per-dependency corruption checks. Legacy receipts gain no inferred admission evidence. | Source-clock/color evidence, dependency/history reference tracking, cache eviction, portable copy, and offline-project rendering. |
| DP-20 | Focused one-Original UI with visible keybindings and native accessibility. | Partial | [`deadpan-app`](../crates/deadpan-app/) has labeled controls, visible context, shortcut help and native file panels. [Expanded UI replay and visual review](qualification/workspace-layout-2026-09-27.md) cover fourteen scenarios through a full run and focused follow-ups. Actual Metal captures show a larger viewer, visible Camera/pause actions, first-frame notices and pinned sound controls; pointer/wheel/text ownership, resize and 10,000-beat selection checks pass. [Repeat queue feedback](qualification/repeat-input-2026-09-27.md) exposes pending work and keeps the header Working through completion. [Footer qualification](qualification/footer-layout-2026-09-27.md) resolves first-frame command-exit gaps and clipped Original shortcuts, with same-input mode/geometry checks and actual Metal captures. Native keyboard/accent evidence remains in the [earlier qualification](qualification/source-preview-2026-09-21.md). The [design pass](qualification/ui-design-pass-2026-10-04.md) adds system fonts, consistent hierarchy and focus cues, two-row priority footer keys and a larger picture across 5,388 replay checks, plus bounded visible-card thumbnails through the shared renderer. The empty start surface now follows the product board's "Choose one Original" card (Choose video, YouTube URL, Open project) at default and minimum sizes, with keyboard-only URL entry, confirmation and cancellation covered by the `youtube` replay. The [accessibility audit](qualification/accessibility-2026-10-05.md) inspected the running app through the macOS Accessibility API and its announcement notifications. It fixed an AccessKit abort on the start screen's second Tab, gave painted panes spoken summaries (viewer frame, time, beat and Camera draft; selected beat), made saves and refusals live regions, labelled modal sheets as dialogs with initial focus, exposed full truncated text and named fields, and follows macOS Reduce motion and Increase contrast; the `accessibility` replay asserts the AccessKit tree, focus and focus ring. | Full workflow, VoiceOver speech and navigation (announcement requests are verified, not speech), CJK IME and non-US layout acceptance, timeline waveform and thumbnail strips at other scales, other busy-command behavior and minimum-size picture priority in the [interaction review](INTERACTION_REVIEW.md). |
| DP-21 | CLI/JSON API with revision checks and dry-run. | Partial | [Shared headless API](HEADLESS.md): named Macro inspection/save/run/dry-run with exact bank receipts, explicit Child/Time/Object selections and qualified register pastes; automatic Render for closed and open native projects with checkpoint retry/reconciliation; project/command/history operations, atomic linked range move, explicit migration, picture/audio-plan and source-PCM inspection, exact boundary and named-mark range resolution, original retention/inventory/verification/relinking, automatic project creation, source registration/insertion, geometry preview/adoption and structured errors. [Owner preparation](LIVE_PROJECT.md#background-preparation) routes retention, relinking, exact registration and consistent checkpoints through the native import worker and captured writer. [Registration subprocess tests](../crates/deadpan-cli/tests/source_registration.rs) and [audio inspection tests](../crates/deadpan-cli/tests/audio_inspection.rs) cover actual media, historical identity, read-only coexistence and stable failures. | Complete command/selector surface, complete recovery/failure acceptance, and headless/GUI parity. |
| DP-22 | Self-contained zero-manual-setup personal bundle (no Developer ID signing or notarization, owner decision 2026-10-05). | Partial | [Packaging](PACKAGING.md): `cargo xtask bundle` builds a relocatable `Deadpan.app` in its own path-remapped target directory. It contains release `deadpan-app`, `deadpan-cli`, media, transcription and tracking workers in `Contents/MacOS`; the pinned LGPL FFmpeg 8.0.3 libraries relocated to `Contents/Frameworks` through `@rpath`; and a read-only yt-dlp/Deno baseline. An `Info.plist` key marks the packaged app. It always uses its own baseline and never falls back to the managed root. Each helper is anchored to its compiled pin: Deno by exact bytes plus Deno Land's signer requirement, re-signed yt-dlp by a signature-independent Mach-O content hash plus signature validity (and the app's Developer ID team when present). A [private AI runtime](PACKAGING.md#ai-runtime) (python-build-standalone CPython 3.12.13, the 36 locked wheels with the macOS 26 MLX build, the pinned LTX source, the worker and a separate GPL `ffmpeg`/`ffprobe`) is assembled from verified pins into `Contents/Resources/ai-runtime`, signed file by file without entitlements, and is the only runtime a packaged app uses (developers may opt in to explicit variables); the bundle is 710.1 MiB (288.5 MB ZIP). Code is signed inside out with the hardened runtime. Deadpan and yt-dlp have no entitlements, which was measured, and Deno keeps its own. `codesign --verify --deep --strict`, a scripted `otool` audit and the staged bundle's own helper check must pass before publication. `bundle-verify` passes positive checks (smoke test, doctor, hardened helper probe, `create-original`, `render`, the bundled AI runtime and an offline pack import with its smoke test) and negative checks (tampered and deleted helpers with a managed fallback present) on scrubbed relocated copies with an isolated `HOME`. A real YouTube import from such a copy also succeeded. | Developer ID signing and notarization are out of scope by owner decision (personal app, no developer account); the scripted path is kept unused, and personal signing uses the dotfiles local identity. The GPL FFmpeg question does not arise for a personal, undistributed build. Still open: clean-machine online and offline acceptance, downloader update manifests and rollback, app updates, and bit-for-bit reproducibility. |
| DP-23 | License/SBOM/privacy/security requirements. | Partial | [Dependency inventory](DEPENDENCIES.md), native harness build/license hashes, strict bounded domain JSON, schema checks, and initial package-path protections. Qualification explicitly excludes the developer GPL FFmpeg build from distribution. [Bundled notices and SBOM](PACKAGING.md#notices-and-sbom): crate licenses are normalized (legacy `/` to `OR`) and validated against the vendored SPDX 3.27.0 identifiers, and crates without license files must be covered by a bundled SPDX text or the build fails. The generated `THIRD_PARTY_NOTICES.txt` covers the shipped binaries' normal Cargo closure (247 crates, with each crate's published license files including vendored whisper.cpp, SQLite, Signalsmith and fonts, plus authors, copyright lines and SPDX texts for the 40 without files). It also covers the FFmpeg LGPL-2.1+ notice (built unmodified, relocated and re-signed) with the exact source archive, hash, commit, host-path-free configuration and shipped library hashes, and the vendored, hash-checked yt-dlp/Python, yt-dlp-ejs and Deno notices. A CycloneDX 1.5 SBOM lists crates, FFmpeg, both helpers and the vendored native components, and per-file SHA-256 sums and build provenance accompany the bundle. The AI runtime adds `AI_RUNTIME_NOTICES.txt` (every component with validated SPDX expression, source and hash, the GPL statement with exact sources and configuration, and modified files) and each package's license files, plus SBOM components. Model packs carry separate weight and text-encoder license layers with compiled full texts and recorded sources, require explicit acceptance, and refuse hostile offline archives. Only the documented folder-access usage strings are declared. The [adversarial suite](ADVERSARIAL.md) fuzzes every untrusted input boundary (containers, codec records, decoders, project/command/patch/slice/context JSON, tampered packages, worker protocols, the authenticated live endpoint, yt-dlp metadata, pack archives and manifests, stored indexes and analysis artifacts) in regression, campaign and sanitized modes; the [2026-10-05 run](qualification/adversarial-2026-10-05.md) found no crash or bound violation, and its undetected register slot-row tampering is fixed by the schema-65 register bank digest. | Release audit, deeper hostile-worker tests (encoders, render/tracking/transcription/model workers beyond their protocols), coverage-guided fuzzing, privacy checks, and aggregated Deno/V8 notices. Distribution obligations (FFmpeg source offer, GPL AI FFmpeg programs, model-weight redistribution) do not arise: Deadpan is a personal, undistributed app (owner decision 2026-10-05). |
| DP-24 | Measured performance budgets and diagnostics. | Partial | [Performance measurement](PERFORMANCE.md) maps every Section 25 target to a measurement. `cargo xtask perf` runs it on locked copies of real and generated media. A target row PASSes or FAILs only for a successful stage on a quiet machine with enough samples; quick, flagged or short runs are INFO. The [2026-10-04 run 3](qualification/performance-2026-10-04.md) (M5 Max, release, hashed binaries) passes the release UI replays (key-to-state 0.21 ms, navigation to picture 1.53 ms, cached edit 6.24 ms, Hold fallback 7.22 ms and 10,000-beat navigation 0.45 ms p95), 30 s real-device auditions with 0 underruns and no dropped, leading or trailing pictures at 1080p24, 1080p60 and 4K30, and warm seek on the 640×360 fixture (71 ms p95). It failed long-GOP warm seek at 1080p and 4K (265–1,862 ms p95). The [2026-10-05 seek qualification](qualification/seek-2026-10-05.md) adds threaded decode, non-reference preroll skipping and progressive preview admission; see that record for the current 1080p and 4K results. Run 3 also failed headless real-project edits as pauses accumulated (54–109 ms p95), and 10,000-beat edits (1.6–3.8 s, pauses refused after 4), because each pause retained a whole-structure timing layout. [Compact timing storage](TIMING_STORAGE.md) slices new timing tables to their referenced paths, omits provably inert reanchor steps, records granular binding patches and stores revisions as keyframes plus patches (database 63), with bit-exact PCM against the previous representation over 300 random release sequences. The [2026-10-05 record](qualification/timing-storage-2026-10-05.md) passes real and generated project edits (p95 9.8–24.9 ms, databases 7.7–9.2 MB instead of 192–194 MB) and commits all 30 pauses on 10,000 beats; those edits then failed at 340–711 ms p95 (174 MB instead of 4.8 GB), and reopening 121 revisions took 28.9 s instead of 105 s. [Incremental commits](TIMING_STORAGE.md#in-memory-head-and-validation-reuse) keep the validated head in memory, store only each commit's patch with periodic keyframes and size bounds (database 64), validate each result once while reusing unchanged binding owners' proofs, and compile the refreshed plan without revalidating; a [hash-chained history receipt](TIMING_STORAGE.md#verified-history-receipts) replaces replay on open, with `project validate --full` still recomputing everything. The [incremental commit record](qualification/incremental-commit-2026-10-05.md) brings generated-project edits to 3.9–5.1 ms p95 and pauses to 14.9–16.1 ms, and 10,000-beat edits from 372–683 ms to 28.0 ms p95 for undo (PASS), 52.6–52.8 ms for split and wrap and 163 ms for pause (FAIL); opening that 123-revision package read-only takes 0.44 s instead of 24 s. Run 3's cold first picture (6.9–15.9 s, page-cache-warm) was INFO because admission decoded the whole Original first. Export runs at 0.135–2.21× real time. `Engine::diagnostics()`, `ProjectPictureSession::stats()` and `doctor --project` add underrun, decoder-cache and per-revision timing diagnostics, and the cursor boundary lookup is now O(log n). `doctor` reports actual core/SQLite probes. [Final footer release qualification](qualification/footer-layout-2026-09-27.md) passes warm navigation p95 5.20 ms, input CPU 0.70 ms, cached Repeat 7.76 ms and Hold fallback 8.59 ms against unchanged targets, after the bounded Repeat queue checks restore the Original. It uses the corrected repaint wait and actual offscreen Metal completion. [Earlier qualification](qualification/ui-feedback-2026-09-26.md) retains the 75.26/138.65 ms edit misses and 1.23 ms 10,000-beat navigation CPU result; the latter performance workload was not rerun here. | Meet cold seek at 1080p60/4K (one verified copy instead of two; warm 4K long-GOP seek now passes through the [preview proxy](qualification/proxy-2026-10-05.md) at 8.2 ms p95, while the exact Original picture follows 390 ms p95 after the request), remove the remaining whole-document command work on 10,000 beats (result validation, Split and pause revalidation, lineage, marks and diff walks, the pause's binding serialization) to bring split, wrap and pause under budget, reduce the first pause's whole-project lattice capture, rerun edits on an idle machine, and measure native-window playback, inference, physical display latency, idle CPU, memory pressure and lower tiers. Add decode-queue, file I/O, PCM-cache and live model-memory counters and a native diagnostics panel. Tiny-fixture offscreen results do not establish those budgets. |

## Delivery gates

[Specification Section 30](spec/DEADPAN_SPEC.md#30-implementation-workstreams-and-delivery-gates) defines the complete ordered build plan. Gates may have parallel work behind their interfaces; none permits calling an earlier subset the completed product.

The [first actual LTX MLX probe](qualification/model-smoke-2026-09-20.md) adds
single-file generation evidence to Gate A. It does not select a model or satisfy
the corpus, warm-performance, exact seam, color, or packaging gates.
The [supervised adapter](qualification/model-worker-2026-09-21.md) adds real
worker-boundary and exact interior-file evidence while those gates remain open.
The [lossless master probe](qualification/ffv1-2026-09-21.md) adds actual generated
frame conversion through LGPL FFmpeg, with normal/sanitizer runs, failed initial
duration metadata, rejected corrupt payloads, and retained trailer-truncation
limitations. Durable promotion and application rendering remain open.

| Gate | Status | Required work and exit evidence |
| --- | --- | --- |
| A: Qualify risky dependencies | Partial | [Native](qualification/media-2026-09-20.md) and [compatible media](qualification/media-compatible-2026-09-20.md) harnesses include actual decode/seek/audio, sanitizers, and retained frame ownership. Signed LGPL FFmpeg 8.0.3 works with pinned rsmpeg; B-frame mux and VFR terminal-duration failures remain. The [raw DSP candidate](qualification/audio-2026-09-20.md) retains 82/605 failures. A [canonical audio prototype](qualification/audio-canonical-2026-09-20.md) passes 3,447 checks per normal/sanitized run, with 558 identical PCM hashes and failed analysis-window alternatives retained. Still required: integrated shipping adapters, format/color matrix, GPU viewport, application audio scheduling/cache lifecycle/device output, listening, model candidates, private-runtime packaging, full-size benchmarks, and model-pack qualification. |
| B: Establish the pure editing foundation | Partial | Typed time/nodes, commands/inverses, stable nested instance paths, persistent marks and edit transforms, sparse play subtrees with variable-duration indexing, atomic edits through complete occurrence paths, exact boundary/range queries, schema/history migration, headless API, indexed exact picture plans and bounded structural audio plans exist. Still required: temporal attachments, remaining semantic range/text operators, incremental fragments and full audio rendering. Representative nested edits must render with exact picture/sample selection; serialization and inverse transactions must preserve all authored meaning. |
| C: Build the interactive media workspace | Partial | Non-destructive source preview uses actual persistent decode/index and the shared SDR GPU baseline, with tested frame navigation. Managed/linked original storage is available through the headless boundary. The native workspace creates one-Original projects in Documents/Deadpan, initializes the full video, protects that undo baseline, registers separate sound media, reuses the original and preserves legacy projects. Imagegen targets guide the visible Original/Your edit layout and contextual keyboard teaching. Still required: full import management, proxies, mastered audio/playback, complete pane/selection/inspector workflows, full keyboard grammar, focus/IME/accessibility corpus and measured editorial latency without drift. |
| D: Complete the creative operation surface | Open | Every Section 8 operation and starter recipe, per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags, registers, semantic macros, and shared command/help registry. Each must remain editable/portable and pass preview/export verification without no-op placeholders. The [Section 8 coverage inventory](SECTION8_COVERAGE.md) tracks each operation's status and evidence. A [preview/export verification](PREVIEW_EXPORT_VERIFICATION.md) harness (`verify-export`) now compares exported pictures and audio with the committed preview path; thirty-five headless recipe fixtures (ranged gain steps, `:gag-set` on One More Time, a bounce micro-loop with marked play seams, the synthesized triumphant sting, saturation, pitch shift, J-cut, L-cut, seeded One More Time, role-only deletes, audio-only and video-only repeats, repeat with gap, One More Time, Nothing Happens and Are We Done? through the headless semantic path, the recorded `:repeat 3 gap= gain-step= zoom-step=`, reverse hiccup, ping-pong, hanging reverb tail, echo tail, delayed captions over Original and black pictures, bleep, lift, audio lag, bed drop, mute range, off-center stare, freeze and black pauses, 50% Preserve retime, zoom/creep/follow framing, cutaway, escalating repeat, gain trim, placed sound) pass it after public Render ([qualification](qualification/preview-export-2026-10-04.md)), recorded in the coverage inventory's Export verified column. Generated Holds, overrides and many rows remain without export comparison, and native key paths are not exercised by it. The [inventory](SECTION8_COVERAGE.md) counts 83 implemented, 12 partial and 0 missing rows; no row is complete for Gate D. Remaining partial rows: AI holds (Living stare, `:hold … video=ai`), Escalation speed progression, `audio-shift`, role-repeat `extend=hold`/fit policies, per-play timing edits, effect sends, variable-rate stretch, framing point/region targets, one-play and ranged-zoom dot/macro coverage, and the shared command/help registry. |
| E: Add analysis and real AI holds | Open | Local analysis and correction, tracking, runtime/model manager, generation planning/validation, audition/acceptance, stale-job handling, and caching. Actual qualified local generations must meet duration/seam contracts; accepted projects must render offline without the model. Publish latency and quality measurements. |
| F: Complete import, export, and distribution | Partial | Groundwork only: the [relocatable bundle](PACKAGING.md) ships the pinned helpers, relocated FFmpeg, workers, notices and an SBOM, with ad hoc hardened-runtime signing, and passes a scrubbed relocated import/render check. Safe updates and clean-machine acceptance remain (no Developer ID signing or notarization by owner decision). Required: bundled single-video yt-dlp/EJS/Deno, Original provenance and full-source initialization, audio-only effect import, safe updates, automatic output, HDR/SDR and codec/mux verification, notices, signed runtimes (ad hoc or personal identity), and release migration. [Recovery](RECOVERY.md) adds unclean-exit detection and reopen, interrupted-job reporting with explicit retries, missing-Original relink and real disk-full/read-only/permission failure tests with truthful native messaging; rotating backups and release migration remain. Complete the keyboard-only source-URL-to-MP4 workflow from the distribution without external setup. |
| G: Release qualification | Open | Run every requirement, crash/chaos/malicious-input suite, long-project stress, preview/export comparisons, clean-machine online/offline installation, accessibility, and performance measurements. Deliver app, approved packs, documentation, fixture/benchmark reports, SBOM/notices, and migration policy; explicitly report any deviation. The reproducible benchmark suite and its first [performance report](qualification/performance-2026-10-04.md) exist; its warm/cold seek, real- and large-project edit and pause-history storage failures remain open deviations, and lower-memory tiers, display latency, idle CPU and inference stress are unmeasured. The [adversarial suite](ADVERSARIAL.md) provides the crash/chaos/malicious-input suite (deterministic regression in the normal tests, `cargo xtask chaos` campaigns with ASan/UBSan for the source adapters) and a synthetic long-project stress (`--stress`: 10,000 beats over a two-hour Original, hundreds of revisions, reopen, full replay validation and render-plan range queries within budgets). Its [first run](qualification/adversarial-2026-10-05.md) found no crash; the one integrity finding (register slot rows outside tamper detection) is fixed. Still open: coverage-guided fuzzing, hostile inputs inside encoder/render/tracking/transcription/model workers, sanitizers beyond the source adapters, stress on real long media with export, and release-candidate reruns of the whole suite. |

## Updating evidence

For each completed slice, link the implementation and named tests plus an acceptance report containing the revision, fixture/input, command or interaction, expected/observed result, and environment. Include hardware/OS, dependency/runtime versions, power/cache state, latency distribution, and failed samples where relevant. Preserve unfulfilled behavior explicitly.

Before marking any creative operation complete, establish that it is editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A passing test double proves only its tested boundary. A claimed release requires actual media, actual local generations, verified emitted files, and the distributed clean-machine workflow.
