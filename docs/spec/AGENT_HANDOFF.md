# Deadpan — implementation-agent handoff

Read version 1.1 of `DEADPAN_SPEC.md` as the current normative full-product specification. The imported 1.0 package is preserved in `archive/1.0/` and does not override the revised single-original V1 policy. Designs and examples are not implementation evidence; keep actual progress and measured capability in the requirement tracker.

## Development format policy, 2026-09-30

The user confirmed that Deadpan has no users and will remain unused throughout
this goal and session. New work may break project formats and schemas without
migrations when that simplifies implementation. Prefer this permission over
historical compatibility requirements; it does not require deleting working
adapters or reduce eventual runtime/recovery requirements.

## Native QA app lifecycle, 2026-09-30

The user clarified that the separately positioned Deadpan Cursor QA window is
an agent test instance and is not in use. Agents may open, close and test it;
the earlier instruction to leave it untouched no longer applies. Close native
test instances after testing instead of leaving them idle. The previously
running `dev.thiesen.deadpan.cursor-qa` instance has been quit, and the native
app inventory confirmed no Deadpan app remained running.

## Declarative editor paths, current implementation

The shipped Normal/Visual grammar uses one bounded compiler with separate
terminal actions and prefix metadata. Preserve outer native shortcut/text/IME
priority and semantic mark/Trim prefix captures. Hints derive valid continuations
from declarations; every structural branch enters the Kestrel audit, including
unannotated branches. Held input may execute only an explicitly repeatable leaf
at the current position; it cannot consume a pending prefix or insert a Hold
through comma-H. See [the contract and remaining work](../KEYMAP.md).
User keymap loading, other mode maps, registers, semantic dot-repeat and macros
remain required. No project schema changes.
The [qualification](../qualification/declarative-bindings-2026-10-01.md) records
the before/after held-key replay, final gates and remaining verification limits.

## Native frame cuts, current implementation

`x` and counted `12x` use one atomic `CutEditSlice` at the retained Edit cursor,
clamped only to the displayed ordinary Sequence's end. `:delete-frames Nf`
captures exact entry context and absence, then checks session/revision/scope
before submission. Visual selections, Original, Sources, Placed sounds and
unsupported partial endpoints refuse; the saved receipt reports the actual
interval and preserves the existing register/Undo contract. No schema changes.
See [frame-cut semantics](../EDITED_SLICES.md#frame-cuts-at-the-cursor).
The [qualification](../qualification/native-frame-cuts-2026-10-01.md) records
final app tests, input-race corrections and rendered layout checks. No ordinary
native window was opened; the short-lived replay processes exited.
The full editing grammar, arbitrary nested cuts and all product gates remain open.

## Native Trim, current implementation

Native `,v` and `:trim` now connect the combined command to one unsaved
In/Out/Slip/Roll draft. Ordered adjustments start from the last accepted tuple;
clamping changes only the active value, and a refused adjustment or Ripple/
Overwrite toggle preserves all four. Entry captures the exact ordinary Sequence,
eligible direct Source/neutral unity Partition, literal right sibling or absence,
session, revision and both cursors. Original, Sounds, composite targets and Edit
Visual ranges, including empty ranges, refuse entry.

Before/Proposed shows an atomic outgoing/incoming pair at the entry/final target
junction, with explicit exterior slots. Enter requires the fully acknowledged
nonzero proposal and current Proposed pair at the final viewer raster. An
admitted Edit waveform measures exact absolute 48 kHz context before limiting
and monitor gain. Optional audition advances only the draft's heard position;
the pair and editor cursors stay fixed. Empty/unavailable audio context does not
block valid picture inspection or Apply. Cancel discards the draft; Apply
consumes one retained command and keeps its durable receipt across refresh
failure. Zero intent has no proposal or history.

See [current controls and scope](../COMBINED_TRIM.md#native-trim),
[waveform semantics](../EDIT_WAVEFORMS.md#native-trim-consumer) and the
[native qualification record](../qualification/native-trim-2026-10-01.md) for
execution evidence and its limits. Broader target admission, complete editor
acceptance, physical keyboard/layout and IME coverage, accessibility and listening
remain open. No requirement or gate is complete.

The following dated Trim and Slip sections retain their checkpoint-specific
results and then-outstanding work. Their test counts do not qualify this native
increment.

## Native Source Slip, 2026-10-01

The native integration connects `:slip +5f` to the shared qualified command
through a captured-target service, stopped Before/Proposed picture preview and
one saved transaction. Capture absence as well as valid session/revision/group/
child context. Separate the temporary inspection frame from both editor cursors.
Apply requires the current Proposed identity and frame to be GPU-submitted at
the current raster; preparation or decode alone is insufficient. Cancel and draft
changes revoke pending picture admission before a replacement is scheduled.

Keep the exact service-owned request and an independent saved receipt. A saved
edit survives refresh failure; historical receipts cannot reselect or replay
warnings after navigation/Undo. Zero movement has no proposed snapshot, Apply or
history. See [Source Slip](../SOURCE_SLIP.md#native-stopped-picture-preview) and
[keyboard routing](../KEYBINDING_COMPATIBILITY.md#source-slip-preview).

Service, UI and harness reviews found no outstanding actionable findings.
All 464 feature-enabled app tests and 3 headless tests pass; the default suite
passes 428 and the same 3. Formatting and strict workspace/all-target Clippy pass.
The final visual replay passes 67 Slip checks and 17,360 Kestrel routing cases
with a matching live registry. The full release replay passes 3,655 checks.
Native keyboard, cancel, one Apply, Undo/Redo and reopen were verified against
consistent SQLite backups. Both test instances exited 0; no Deadpan process or
writer lock remains. Source identities, the two-label glyph correction and
remaining physical-input limits are in the
[qualification record](../qualification/native-slip-2026-10-01.md).
At this checkpoint, full Trim, waveform/audition,
In/Out/Roll, ripple/overwrite and broader target admission remained open. No
requirement or gate is complete.

## Combined Trim authoring, 2026-10-01

The timing checkpoint `8ddf586` is committed and pushed. The complete
`ApplySourceTrim` core command and qualified store/CLI path are implemented in
core 43/database 52. Its corrected scratch stages received
independent review. All 19 combined core integration tests, five qualified store
tests, three CLI parity tests and five decoded-audio/indexed-picture tests pass.
The complete workspace passed 3,366 unit/integration tests and both documentation
tests. Strict lint found one equivalent stable-sort spelling correction; after
that one-line change, strict workspace/all-target lint, 132 core tests and final
formatting passed. Exact source inventories and original failures are retained in
[qualification](../qualification/combined-trim-2026-10-01.md). The first compiler
errors were an ambiguous glob
export and two wrong error adapters, both corrected. Capacity fixtures now
distinguish geometric preflight from the independent capture budget; a store
oracle now retains hidden physical Source context behind the visible crop.
A picture test also needed a named VFR index to retain its frame borrow.
See [the backend contract](../COMBINED_TRIM.md). At this backend checkpoint,
waveform, junction-pair, service, keyboard and native UI work were separate
staged changes. Their static reviews found and corrected proposal identity,
partial waveform retention, final-raster Apply, same-batch native focus, transport
resume and modal/notice issues. Native compilation, focused tests, production
replay, keyboard audit and real native QA were outside this checkpoint. No native
GUI was opened for this backend increment; no Deadpan executable remained at the
final scan.

## Combined Trim timing foundation, 2026-10-01

The Roll checkpoint `3a5e7b7` is committed and pushed. Complete-intent geometry,
Source endpoint phase bindings and the root-sound Trim map are integrated after
independent review. Focused runtime checks cover 57 new tests plus existing
editing and PCM behavior. The complete workspace passed 3,333 unit/integration
tests and both documentation tests; strict workspace/all-target lint and
formatting also passed on the same unchanged source inventory.
Core 42/database 51 refuse prior unused development formats without migration.
See [the foundation](../TRIM_DRAFT_FOUNDATION.md) and
[retained qualification](../qualification/trim-foundation-2026-10-01.md).

Keep the four accepted I/O/S/R values distinct from active inspection. Preserve
all values on a policy toggle or refuse. Overwrite geometry is not structural
admission. Keep the entry-anchored root Keep/Gap intent, normalize contiguous
equal-shift Keeps and preserve prior route history. Disjoint Source allocations
need exact historical closed endpoint phase; never narrow handles or skip the
reanchor to avoid that case. Combined authoring is qualified above; native Trim
was outside this foundation checkpoint.
No native GUI was open at this checkpoint, and no requirement or gate is complete.

## Adjacent Source Roll backend, 2026-10-01

The shared backend command adds `RollSources` for literally adjacent admitted
Source/Partition children in an ordinary Sequence. It intersects both exact edge
intervals once, preserves pair and project duration, captures unbound audio clocks
from the unchanged tree and adds no ripple reanchor. Preserve root sounds/routes
exactly and mark only the changed seam. See [the contract](../SOURCE_ROLL.md).

The reviewed staged core and independent picture/PCM tests are integrated with
store receipt validation, cold/live headless dry runs and persistence tests.
Core schema 41/database 50 refuse the prior unused development package format.
The workspace passed 3,275 unit/integration tests and both documentation tests
with one stale doctor schema assertion. Its test-only correction passed with
the same feature graph; the original failed run and exact source difference
remain recorded. Strict all-target workspace lint and final formatting pass. See
[qualification](../qualification/source-roll-2026-10-01.md), based on the pushed
ripple Trim checkpoint `0304852`. No native GUI was opened; the final process scan
found no Deadpan executable. At this checkpoint, native Trim,
overwrite and combined draft timing/sound semantics remained required.
Do not implement one Enter as several saved commands. No requirement or gate
is complete.

## Ripple Source edge Trim backend, 2026-10-01

The shared command adds atomic `TrimSource` In/Out with explicit ripple
policy, exact handle limits, grow-only physical Source ownership, retained effects
and audio clocks, and one independent root sound transform. Core schema 40 and
database 49 refuse unused intermediate development formats without migration.
See [the contract](../SOURCE_TRIM.md) and
[qualification](../qualification/source-trim-2026-10-01.md), based on the native
Slip checkpoint `40f3320`.

Keep the one old-tree capture: append separate target and suffix reanchor windows
before target-only physical rebasing. Partitions preserve hidden filtering
context. A final spec audit found that the initial Trim implementation omitted
the required fade at a new cut; its raw-equals-faded test encoded that mistake.
The correction records separate one-sided editorial intent on the target and
incident neighbor without changing raw sampling support or retained phase.
Independent PCM checks now cover that intent, delivered fade widths and
retained bound-owner context.
The same audit found unfaded joins when Slip changes material inside a neutral
Split Partition. The correction also marks both Slip joins and their incident
neighbors; raw clocks and root sounds remain fixed. Keep this regression in
the shared plan/audio tests.
Source PTS and physical-local marks can stay
stored behind a crop; ancestor-local and concrete Occurrence marks retain their
existing loss policies. Independent picture and decoded-PCM tests cover exact
extensions, prior resume/reanchors, dormant audio and shifted composite suffixes.
The workspace run passed 3,250 unit/integration tests and both documentation
tests; one outdated native Slip assertion failed. Only that assertion file
changed, and its same-feature rerun passed. Together the runs cover all 3,251
workspace tests. Default app checks pass 428 app and 3 headless tests; strict
all-target lint and formatting pass. Exact manifests and original failures are
retained. No native GUI was opened and the final process scan found no Deadpan
instance running. At this checkpoint, full Trim mode, overwrite, Roll and broader
scopes remained required; no requirement or gate is complete.

## Exact Source windows and atomic Slip, 2026-10-01

Current core schema 39/database 48 retain an exact `SourceNode.edit_window`
before whole-frame enclosure. Full imports, selected Original moments and
temporary sound-range audition construct it; generic mapping changes clear it.
The shared `SlipSource` command shifts qualified linked material atomically while
preserving placement, duration, bindings, effects and root sounds. Its store
preview and headless dry run report exact handles, applied delta and a nullable
edit for zero movement. See [the command contract](../SOURCE_SLIP.md).

All 3,114 workspace unit/integration tests, both compile-fail documentation
tests, formatting and strict workspace/all-target Clippy with `ui-harness` pass.
Independent window, command and PCM reviews found no outstanding issues.
Original fixture/oracle/import failures remain with their corrected runs.
See [qualification](../qualification/source-slip-2026-10-01.md). No native app was
opened for that backend checkpoint. Full Trim/Roll, physical growth, nested/treated
targets and audio-only picture lead/tail remained open then. No requirement/gate
changes status. Unused databases 39 through 47 are refused without migration.

## Source effect clocks, 2026-10-01

Core schema 38/database 47 add an explicit retained framing duration/offset.
Physical prefix or tail growth holds camera endpoint poses in new handles while
preserving the original path. Audio-treatment prefix translation moves every
gain/mute key once, retaining values and the existing voice clock. Crops keep
complete owners behind Partitions. Camera adjustments retain the declared
clock; explicit reset creates a new static owner-clock pose. See
[the contract](../SOURCE_EFFECT_CLOCKS.md).

All 3,046 workspace unit/integration tests, both compile-fail documentation
tests, formatting and strict workspace/all-target Clippy pass. The focused Camera
test and lint also include `ui-harness`. No native app was opened, and the final
process check found none running. At this checkpoint, exact linked windows,
handle clamping, atomic Trim and native controls remained open. No requirement
or gate changed status. Unused databases 39 through 46 were rejected; existing
frozen adapters for 1 through 38 retain closed historical framing vocabulary.

## Retained Source origins, 2026-10-01

Introduced in core schema 37/database 46, exact translations map current physical-local
coordinates to captured audio clocks. `OwnedAudioBinding::rebase_local` updates
every placement, resume boundary and phase endpoint atomically in a returned
value. Frozen layouts and enclosing reanchor windows remain unchanged. Reanchor
entries convert back to current-local coordinates before phase composition.
See [the contract](../SOURCE_ORIGINS.md).

The decoded-PCM regressions preserve fractional NTSC phase, independent sample
offsets, edge fades and chronological resumes behind a unity Partition, with
exact inverse restoration. All 3,019 workspace unit/integration tests, both
compile-fail documentation tests, formatting and strict all-target Clippy pass. Retained
commands and failed fixture setup are in
[the evidence](../../tools/media-qualification/evidence/2026-10-01-source-origins/README.md).
No native app was opened. This adds no public Trim operation. The subsequent
effect-clock work above preserves framing and audio treatments; atomic authoring
and native boundary controls were still required then. At this checkpoint, unused
development databases 39 through 45 reject without migration or writes.

## Dormant linked audio, 2026-10-01

New Original moments outside measured audio retain the complete audio span,
affine mapping and `Linked` intent with equal selected endpoints. Ordinary and
captured audio plans return silence without requesting source PCM. Growing the
selection reveals the retained phase. `audio: null` remains intentionally absent, placed
sounds still require positive support, and dormant audio has no source anchor.
See [the contract](../SOURCE_MOMENTS.md) and
[qualification](../qualification/dormant-linked-audio-2026-10-01.md).

At this checkpoint, core schema 36/database 45 and audio context schema 5 store this meaning.
Supported historical document, command, patch and context grammars remain closed.
Unused development databases 39 through 44 reject without mutation or migration;
create fresh native QA packages. The subsequent origin work above adds audio
clock translation. Full Trim still needed retained framing/treatment owner clocks
and native boundary controls at this checkpoint.

Verification covers 2,169 distinct affected-crate unit/integration tests and both
compile-fail documentation tests, workspace formatting and strict all-target
Clippy. The new PCM tests check exact no-read silence,
fractional-phase selection growth and inverse restoration. Real-media storage
tests retain the link through close/reopen and Undo/Redo. The evidence keeps the
old-code failures and corrected test setup; independent review has no findings.
No native app was opened.

## Exact picture selection context, 2026-10-01

New Original moments retain the complete measured video span and affine mapping
with a separate exact `SelectedPlacement` window. The shared picture plan holds
only the selected first/last intersecting PTS interval, including rounded end
slack. Source anchors reject hidden context and retain exact selected endpoints.
Audio placement and beat duration are unchanged. See
[source picture timing](../SOURCE_VIDEO_MAPPING.md) and
[qualification](../qualification/selected-video-context-2026-10-01.md).

At this checkpoint, core schema 35 and database 44 reject unused development databases
39 through 43 without writes or migration. Existing frozen adapters for 1 through
38 remain. Create fresh native QA packages.

This was a Trim prerequisite. Native `,v`, in/out/slip/roll commands, ripple versus
overwrite, clamped handles, outgoing/incoming pictures and candidate waveform
remained open at this checkpoint. Reuse the splice/gain captured draft, proposed
snapshot and worker identity patterns. Do not resize a Source to trim it: that changes FitBeat rate
and normalized framing. Partition crops preserve owner domains, but extension
beyond those domains still needs explicit semantics. Linked edits must retain
exact A/V alignment and the captured audio lattice through one atomic commit.
Extending before a Source's local zero needs an exact translation between live
and retained audio clocks. The subsequent dormant-audio work above retains
linked intent for moments with no audio overlap.

Qualification covers 2,956 distinct workspace unit/integration tests plus both
documentation tests. All 270 plan tests pass after removing duplicate clock
storage from the private compiled selection. Strict workspace/all-target Clippy
and formatting pass. The archived range fixture remains unchanged and now proves
rejection of its unsupported package while retaining command-shape checks. No
native app was opened; Trim was outside this checkpoint and full product
acceptance remains pending.

## Native marks and jump history, 2026-10-01

`m` + letter, `'` + letter and the Marks list use persisted core marks. Original
positions retain exact measured PTS; Edit positions retain concrete host
occurrences and follow core edit transforms. `native-mark-{letter}` plus the
exact single-letter label is the shared native/headless address. Different-label
collisions reject. Copies have independent IDs. No schema change is needed.

Mark-only saves have separate durable receipts and never carry a selection-changing
edit completion. Preserve both cursors, pane, beat and live Visual range. Capture
prefix/command entry including absence, reject stale revisions, and discard a
late jump after navigation. A failed post-save refresh retains the reopen warning.

Ctrl-O/Ctrl-I navigate bounded session history with exact fractional positions.
Only mark-only commits rebase Edit history; unrelated revisions expire it.
Prune expired entries in both directions while preserving qualified Original
positions. Saved marks still follow structural changes. See
[the contract](../MARK_NAVIGATION.md). Full editing grammar and occurrence-level
navigation remain open; this does not complete DP-05 or DP-20.

[Qualification](../qualification/native-marks-2026-10-01.md) retains 448 UI-feature
and 412 default app/headless passing tests, strict all-target workspace Clippy,
formatting, 156 focused rendered checks and the 16,368-case Kestrel audit.
Full release replay passes 3,589 checks with no findings or failed timing samples.
Native keyboard input and seven consistent backups verify both clocks, modal
focus, removal/Undo and restart persistence. Both QA processes exited normally;
no Deadpan app remains running and its project lock is released.

## Visual slice replacement, 2026-09-30

Your edit now has an independent half-open selection (`v`, motion, `v`).
`:splice` captures it and explicitly toggles **Replace selection · r**, retaining
the fixed removed interval while refining the Original slice. Before/Proposed
uses exact absolute sample boundaries. Fast `p/P` uses the same atomic command.
One `ReplaceSource` transaction splits endpoints, removes the selected time,
inserts the qualified Source and transforms placed sounds once; one Undo
restores all authored state. Ordinary Sequence scopes support Source/Hold/
fragment endpoints and complete intervening composites.

Core 34 persists a direct sound replacement map and database 43 stores it.
Development databases 39–42 are refused without writes or backup creation;
existing frozen migration adapters for 1–38 remain. Use a new project for
current native testing. Follow the native QA app lifecycle policy above.

[Qualification](../qualification/slice-replacement-2026-09-30.md) records the
production replay, actual decoded media, failure corrections and review.
Native release QA verifies all four join pictures, one replacement and one
Undo. SQLite backups confirm exact authored restoration except the fresh
revision, exactly two new revisions and unchanged rows in all 16 unrelated
tables. The isolated QA app released its writer lock; the user's window stayed
untouched. Automated verification passes 2,641 locked workspace tests, 372
UI-feature tests, 401 slice replay checks, strict Clippy and formatting.
Edited-slice copy/move, role-only placement and Repeat/Retime occurrence targets
remain open. The deletion phase defect found during this work is addressed
below through a distinct current command; historical Delete stays unchanged.

## Retained audio through deletion, 2026-09-30

Native `dd`/`:delete` and the public CLI `delete` verb now use `DeleteRipple`.
Before detaching an ordinary Sequence child, retain each downstream owner's
old sample entry on the original tree. Later siblings at every Sequence level
keep their clocks, including compact Repeat gaps and opaque Preserve outputs.
Empty and terminal deletion do not allocate an unused clock. Root sounds move
once; marks and removed Hold allowances follow their existing policies.

The 44.1 kHz Source witness at 30000/1001 fps reproduced the old defect:
deleting a one-frame prefix changed 6,405 of the surviving 6,406 samples.
The corrected path retains the complete decoded suffix, including cold final
reads and reversed irregular read order. Independent scalar entry and complete
stretch-history references supplement comparisons with the original document.
See [the contract](../AUDIO_REANCHORS.md#ripple-deletion) and
[qualification](../qualification/ripple-delete-2026-09-30.md).

Core 34/database 43 remain unchanged. Historical core `Delete` replays its old
patches; normalize the public CLI verb at ingress only. Current stores can hold
both commands in one validated history. Native tests retain the captured group,
successor selection, protected Original baseline and exact Undo; store tests
reopen mixed histories and reject stale revisions after Undo. Role-only and
Repeat/Retime occurrence deletion remain required; ordinary Visual ranges are
implemented below.

The locked workspace passes 2,661 tests and the UI-feature app passes 375, with
none failed or ignored, including eight decoded-PCM deletion cases and the
native, CLI and mixed-history tests. Formatting and strict all-target workspace
Clippy with the UI harness feature pass.

## Visual range deletion, 2026-09-30

`gg 20l v 10l d` now removes `[20,30)` from the 120-frame fixture in one command:
duration 110, cursor 20 and two retained fragments. The displayed join resolves
to old frame 30. Active and finished ranges work in both directions; an empty
selection cannot delete a beat. `:delete` independently captures the exact range
or beat at command entry, including absence, and rejects stale targets.

`DeleteRange { parent, range, identities, timing }` shares replacement endpoint
splitting without allocating an extra Source. Original clocks are captured
before any Split, suffix entries before removal, and root sounds transform once.
Aligned terminal/full deletion captures no unused clocks; allocate a second
ordinal only when splits and a surviving suffix both need it. The seven-frame
NTSC/44.1 kHz witness retains old prefix `[0,1602)` and old suffix `[4805,11211)`
at new `[1602,8008)`, including cold reverse reads. Six PCM cases include nested
RoomTone, earlier bindings, Repeat gaps, full Preserve history and aligned cuts.

The range replay passes 98 checks and the slice regression passes 400, each with
11,904 Kestrel routing cases. Saved edits retain their durable receipt on refresh
failure and explicitly require reopening; stale workspace delivery cannot move
the cursor or selection. Independent review checked this recovery and preserved
coalesced registration behavior. See [qualification](../qualification/delete-range-2026-09-30.md).

Separate native QA verifies empty rejection, one `[5,10)` cut, the exact join
slates `004`/`010` and Undo. The first SQLite comparison records one edit and
one Undo; a second records only the picture-check Redo/Undo cycle. Both restore
every authored field except the fresh revision and release the writer lock.
Native screenshots were inspected but could not be saved through CUA; retained
images come from the GPU replay. The user's Cursor QA window stayed untouched.

The locked workspace passes 2,685 tests and the UI-feature app passes 380, with
none failed or ignored. Formatting and strict all-target workspace Clippy with
the UI harness feature pass on Rust 1.97.1. The resumed runner had selected
Homebrew 1.98, whose new media-code lints failed; the corrected checks use
`rustup run 1.97.1 cargo ...`. Compiler identities in all 162 workspace binaries
and the native/replay binary confirm the original passing runs used 1.97.1.

Core 34/database 43 remain unchanged. Role-only deletion, motion/text-object
operators, edited-content registers/copy/move and Repeat/Retime occurrence
interiors remain required. A range cut currently does not populate an editable
register, so this does not complete the full delete/register contract.

## Edited slice core, 2026-09-30

`CapturedEditSlice` now captures immutable edited contents without history writes;
`SpliceSlice` inserts them at an ordinary Sequence seam in one reversible command.
See the [contract](../EDITED_SLICES.md) and
[qualification](../qualification/edited-slice-2026-09-30.md). Partial Source,
ordinary Hold and nested unity Partition endpoints retain complete owner contexts
behind neutral output windows. Whole intervening composites stay editable.
Group contents exclude the unselected parent's treatment; whole owned groups
retain it. Independent root sounds stay separate and transform once on paste.

Paste gives fresh identities to authored nodes, marks, scoped Repeat families,
lineage allocation/origin pairs and historical timing aliases. It retains compact
order, complete birth Run support, old sample entries and opaque Preserve
contexts, then rebuilds frozen indexes. Partial marks use exact bias-aware
fragment filtering; whole units retain hidden intent. Unresolved marks and
absolute Sequence pins keep their semantics. Core 34/database 43 and existing
runtime clock types are unchanged.

The store reads the named immutable revision and recaptures the declared range
before admitting historical media. A serialized copy can survive deletion of
its source or accepted Hold without admitting unselected catalog media or
reviving generation requests. Tests cover independent repeated pastes, partial
copy-of-copy, exact picture plans and decoded PCM, untouched destination content,
read-only preview, one-command persistence and fresh-revision Undo/Redo.

Independent review is complete. All 2,717 locked workspace tests, formatting and
strict all-target workspace Clippy pass on Rust 1.97.1, with no failed or ignored
tests. The qualification record retains exact source hashes and test logs. No
native controls changed, so this checkpoint adds no new UI or acoustic evidence.

Next connect capture to the native register on the service worker. Reject late
copy completions by project session and request identity; extend the existing
visible placement proposal, endpoint pictures, local refinement, audition joins
and saved-edit recovery. Interior insertion and edited replacement now have
atomic commands, recorded below; move still needs its own atomic command.
Named registers, role-only placement, cut-to-register, motion/text-object
operators and partial Repeat/Retime/generated-Hold occurrence interiors remain
required. The seam insertion boundary alone does not complete those workflows.

## Edited interior insertion and replacement, 2026-09-30

`SpliceSliceAt` pastes strictly inside a named direct child and `ReplaceSlice`
replaces a nonempty global range under an ordinary Sequence. Both preserve the
captured edited structure and commit as one reversible transaction. See the
[contract](../EDITED_SLICES.md) and
[qualification](../qualification/edited-placement-2026-09-30.md).

Destination lattices are captured before endpoint Split, and the original
suffix entry is retained before removing anything. No deletion-only clock is
observed. Joint Split/import identity pools, conditional timing ordinals,
mark fragments, sound routes and allowances are checked atomically. New slice
commands admit nested unity Partition destinations without widening existing
Original command admission. Store recapture and historical media authority
cover all three slice commands. Core 34/database 43 remain unchanged.

Focused checks pass 126 core, 15 decoded-PCM, seven picture-plan, 39 store and
two CLI tests. An initial PCM oracle omitted the old rounded discrete support;
the corrected test preserves the entire output allocation and explicitly
checks the one exhausted sample. Independent core inspection confirmed the
diagnosis; production code did not change. The qualification record preserves
this failure and the three initially invalid Source test fixtures. Independent
implementation review found no defects. All 2,739 locked workspace tests,
formatting and strict all-target workspace Clippy pass on Rust 1.97.1, with none
failed or ignored. The qualification record retains exact source hashes and
logs; all source hashes still matched after the gate. No native UI or acoustic
qualification was added by this backend checkpoint.

Next connect edited capture to the native service/register and existing visible
proposal. Preserve immutable capture revision and parent during local refinement;
newer copy intent supersedes late replies, but accepted copies survive edits and
Undo. Historical assets require an explicitly admitted preview inventory. Keep
the existing strict default proposal asset checks. Source endpoints need copied
owner contexts, which can be materialized by pure in-memory paste into a neutral
document and rendered through the shared pipeline. Whole historical-document
frames can include unselected ancestor framing and are not a sufficient source
strip. Native integration, atomic move and the remaining full-spec operations
are still open.

## Native edited slices, 2026-09-30

`v`, motion and `y` in Your edit now capture immutable historical contents on
the service worker without writing history. One session register holds either
Original or edited content. New copy intent supersedes late replies; accepted
edited copies survive subsequent edits and Undo. Closing or switching projects
clears the register. `p/P` inserts at a seam or replaces a selected Edit range
using the existing atomic commands.

`:splice` materializes a neutral copied source view, refines against its exact
historical parent and previews the proposed destination. Source endpoint
preparation survives destination rejection. Copied pictures exclude unselected
historical/destination ancestors, retain owned clocks/framing, carry a distinct
presentation identity and never become Camera targets. Store-issued opaque
views admit historical Original and accepted generated media to picture/audio;
strict ordinary Original proposal validation remains separate. Temporary edited
views check session liveness during warm reuse; ordinary committed warm private
PCM behavior stays unchanged. Full catalog checks occur once per immutable view.

See [qualification](../qualification/native-edited-slice-2026-09-30.md) for
review corrections, retained failures, rendered checks and remaining evidence.
The slice replay passes 547 checks plus the 11,904-case Kestrel audit, including
genuine historical capture, unsupported destinations, refinement, actual commit,
Undo and deliberately delayed workspace delivery. Final minimum/default Metal
captures were compared with the design target. Audition delivery in that replay
is injected; decoded PCM has separate tests.

The retained-project run adds one storage-root check. Separate native keys verify
copy/refinement, decoded endpoint slates, one 18-frame insertion and Undo. The
accepted register retains its historical Hold after undoing the source edit.
SQLite backups prove copy/cancel leave all 20 tables unchanged, insertion adds
one revision, and both Undos restore complete authored state. The temporary app
exits and releases its writer lock. The corrected full release replay passes
3,110 checks. Compact viewer padding now returns 8 points to the picture while
keeping control reserves and hit sizes; failed Sound/Room tone/Gain minimums and
their passing corrections remain in the evidence record.
All 2,766 locked workspace tests pass with none failed or ignored. Formatting
and strict all-target workspace Clippy with the UI harness also pass on Rust
1.97.1. The collector rechecks all 1,330 inputs against the final source manifest.

Core 34/database 43 are unchanged. Atomic move, named/persistent registers,
cut-to-register, role-only placement, motion/text-object operators and temporal
occurrence interiors remain required. DP-06 is partial for the session register;
all DP requirements and release gates remain incomplete. Follow the current
native QA app lifecycle policy above.

## Atomic linked moves, 2026-09-30

`MoveRange` now addresses a current source range and destination in one pre-edit
revision. It supports ordinary Sequence scopes, joint endpoint splitting,
cross-parent moves and whole composite units, retaining their authored/play/mark
identities. Exact no-ops need no nodes or clocks. Partial temporal occurrences
remain unsupported. See the [contract](../ATOMIC_MOVES.md).

Capture original audio lattices before cuts and use one unchanged-time placement
layout for moved and displaced owners. Preserve complete provider support and
compute final allocation from absolute boundaries. Root sound recipes/routes
stay in their unchanged root clock; live Hold gates and grants follow issuers.
Independent review corrected the original cut-plus-gap root-sound design before
implementation and found no further actionable defect in the final code.

Focused checks pass 101 core, three picture-plan, nine decoded-audio, four store
and one CLI test. Retained initial failures concern invalid test setup and two
module-order formatting corrections. All 2,792 locked workspace tests pass with
none failed or ignored, in 1,399.00 seconds. Formatting and strict all-target
workspace Clippy with the UI harness pass on Rust 1.97.1. All final checks share
source manifest `6609d6d091868ae4b71a87a039419bf61209ed4de868f28faf5754e27975e484`;
the collector rechecked all 1,338 inputs.
[Qualification](../qualification/atomic-move-2026-09-30.md) records exact evidence
and limits. Core 34/database 43 are unchanged.

The native increment below adds the operation choice and join inspection.
All requirements and delivery gates remain incomplete.

## Native linked Move, 2026-09-30

Native `:splice` now exposes Copy/Move with `m`, removal/insertion sites with
`s/f`, and saved/proposed comparison with `b`. Source refinement stays local to
the draft. Move leaves Replace and restores the independent insertion target;
Replace always uses Copy. Current source authority is checked through ordinary
command preparation. Historical copying remains valid after edits and Undo but
cannot authorize removal. No-ops allocate nothing and disable Apply while
retaining usable source endpoints.

Move previews and commits the same contiguous forest without a synthetic group.
Keep its first child's identity separate from its complete interval. Selection
requires the matching session, project, revision and destination parent; scope
restoration must work whether receipt or workspace arrives first. Duplicate and
old-session completion cannot retarget later navigation. Site comparison uses
exact absolute frame/sample pairs and context capped by the other site. Boundary
reparenting retains global coordinates and labels unchanged timing. A stopped
terminal sample stays at the exact end boundary; only picture lookup clamps to
the last frame.

The [qualification](../qualification/native-move-2026-09-30.md) records media,
interaction, final gate evidence and remaining limits. The compact layout measures
viewer/footer text before paint and preserves both 44-point timeline rows.
Final verification passes 2,807 workspace tests, 414 app/harness tests, formatting
and strict workspace/all-target Clippy. The final app run covers a last help-only
sentence correction; the evidence collector verifies that exact delta from the
workspace/replay build. Visual replay passes 752 placement checks plus Kestrel;
release replay passes 3,314 checks. Native keys and consistent SQLite backups
verify both join comparisons, cancel, one Move commit, complete authored Undo
and historical Move rejection. The separate app exited and released its lock.
Core 34/database 43 are unchanged. Named/persistent registers, cut-to-register,
role-only placement, motion/text-object operators and temporal occurrence
interiors remain open. Follow the current native QA app lifecycle policy above.
No requirement or delivery gate is complete.

## Nested fragment deletion, 2026-09-30

`DeleteRange` now accepts partial Source/ordinary Hold endpoints behind nested
unity Partition windows, matching the edited-slice capture/placement boundary.
It selects the existing recursive preflight and retains complete owner contexts
through the shared Split path. Capture original clocks before splitting, suffix
entries before removal, and transform independent root sounds once. Exact pools,
temporary node limits and inverse validation remain in force. Other commands'
admission and core 34/database 43 are unchanged.

The [qualification](../qualification/nested-delete-2026-09-30.md) records an
isolated old-code failure, nine new core tests, two independent indexed-picture
tests, four decoded-PCM tests and one native service persistence test. Retained
records include the initial invalid sound fixture and oversized oracle-block
failures, their corrections, independent review and the final gate results.
All 2,823 locked workspace tests pass with none failed or ignored, along with
formatting and strict all-target Clippy including the UI harness. The evidence
collector rechecked all 1,348 final source inputs. No new UI or device
qualification is claimed.

The whole-child and cut register increment below follows this checkpoint.
No DP requirement or release gate is complete.

## Whole-child copy and cut register, 2026-10-01

`SliceCaptureSelection::Child` names exactly one direct child of an ordinary
Sequence, including zero-duration nested groups. Never infer that identity from
its interval: several empty children can share a boundary. Range captures keep
their previous wire form; the literal old-binary fixture checks saved history
without requiring a project migration. Empty insertion uses an explicit child
slot, fresh imported identities and no timing allocation. Existing picture,
audio, marks and sound clocks remain unchanged.

Native `y` copies the selected child when no Visual range exists. Empty Visual
selection remains an error. Visual `d`, `dd` and captured `:delete` submit one
typed cut request: capture privately from the pre-edit revision, commit one
deletion, then publish the retained copy. Failed capture/commit preserves the
prior register. Keep the last durable cut receipt independent of the latest
query or failed cut reply. A newer yank may accept its copy while the old
workspace remains visible, but must not consume that stale selection or erase
the saved refresh warning. Undo has a fresh revision and must not revive that
warning. Register supersession never cancels an already queued authored cut.

Place slice shows an empty group's retained name/path and nested structure.
It schedules no source pictures or audio and selects exact same-time sibling
slots with `j/k`. A wholly empty destination has no invented frame zero.
Empty sources reject refinement, Move, Replace and interior insertion. Positive
whole-child captures retain ordinary Move support; its endpoint rules exclude
adjacent empty siblings. Move always permits returning to Copy after a no-op.

See [qualification](../qualification/structural-capture-2026-10-01.md) for review
corrections, retained failures and exact verification boundaries. All 2,883
workspace tests pass. Final formatting, strict all-target Clippy and 386
default-feature app/headless tests pass; rendered cut/placement checks pass
150/820 cases plus their Kestrel audits. Native keys and SQLite backups verify
cut, paste, cancellation and Undo. The QA app was closed and its writer lock
released. Full release replay passes 3,434 checks with no findings or failed
timing samples. Core 34/database 43 are unchanged.
Named/persistent registers, role-only placement, motion/text-object
operators and temporal occurrence interiors remain open. No DP requirement or
release gate is complete.

## Native identity, 2026-09-30

The user requested a complete ImageGen icon/logo set and app integration.
[Identity assets and reproduction](../design/brand/README.md) retain the generated
concepts, exact prompts, editable vector layers, six native appearance previews,
full ICNS/iconset, PNGs, outlined logo SVG/PDF/PNG variants and web icons.
`tools/build-app.py` wraps an existing executable with a compiled icon catalog
and macOS 15 metadata. Bare launches use the embedded icon; declared bundle icons
retain system appearance handling. The wrapper leaves external FFmpeg libraries
in place, so signing, relocation, installation and release qualification remain
open.

During the earlier cursor check, native Open remained pending after a desktop
move; automation could not find a visible sheet or reach other Spaces. The
single-Original round trip passed before that attempt. All project tables stayed
unchanged, and the user-requested window position/size was retained. Inspection
of pinned rfd confirms inferred sheet parenting and retained completion, with no
confirmed deadlock. Explicit parenting through `eframe::Frame` is a possible
hardening step that still needs a real native reproduction and verification.
The user subsequently confirmed that all such QA windows belong to this session:
open, close and test them as needed, then close them when testing finishes. The
earlier window and the structural-capture QA window have both been closed.

## Native saved-render recovery, 2026-09-30

`Renders` and `:renders` browse stored jobs, attempts and destinations through
session/ticket-bound owner queries. Eight-row pages preserve command receipts.
Recovery captures the saved job, original encoding checkpoint or exact previous
destination before any picker. It preserves the current edit and unsaved previews,
rejects changed sessions and reuses the public coordinator's fresh admission and
verification. Core schema 33 and database schema 42 are unchanged.

The production Render replay passes 112 checks, including actual checkpoint
retry, historical re-encoding, destination reconciliation and a CLI-style owner
start delivered with a reopened session. Raw Enter/Space/Escape cannot turn IME
confirmation into a modal action. Tab reveals controls immediately; returning
focus waits until the modal's outer frame ends. Status filters retained receipts
by the current project/session. See [the contract](../RENDER_JOBS.md#native-saved-render-browser)
and [qualification](../qualification/render-history-2026-09-30.md).

Native keyboard browsing and the real save sheet/cancellation passed in a
separate QA app; all 20 database tables stayed unchanged and shutdown released
the writer lock. The user's existing window/desktop remained untouched. Native
AX omitted the custom history overlay while listing disabled editor controls,
despite the harness AccessKit tree containing the actions. Investigate that
native accessibility gap; do not infer VoiceOver acceptance from the replay.
Final verification passes 2,578 locked workspace tests, 344 optional UI-feature
tests, strict workspace/UI Clippy, formatting and native Metal startup/shutdown.

Native relink/checkpoint controls, complete keyboard editing, full mastering,
HDR and the full media/performance/release matrix remain open. Saved-render
browsing completes this entrypoint boundary, not DP-17, DP-18 or any delivery gate.

## Visual slice preview, 2026-09-30

`:splice` previews a linked Original moment at an ordinary Sequence child seam
or inside a direct Source, ordinary Hold or supported transparent fragment,
with exact source endpoint pictures, local In/Out refinement, destination and
provisional timeline, frame inspection, and Before/Proposed audition around both
joins. The service retains the exact prepared request until Enter; Escape
abandons the last issued identity and preserves the copied register and saved
edit. Picture and audio use the same genuine proposed document and exact admitted
base. Endpoint pictures have a separate bounded worker/mailbox. Fast `p/P` stays
available. No core/database schema change was needed.

Interior placement uses one `SpliceSourceAt` command with retained Split IDs,
the explicit Sequence parent and a strict local child boundary. Capture audio
clocks before Split, preserve both fragment contexts, then reanchor the suffix
and transform placed sounds once. A failed transaction publishes no preliminary
split; Undo removes split and insertion together. Do not silently descend an
ordinary child group. Core schema 33 and database schema 42 remain unchanged;
frozen command adapters reject the new tag.

[Interior qualification](../qualification/interior-slice-2026-09-30.md) records
2,609 workspace tests, 362 UI-feature app tests and 267 production slice checks.
Actual proposed and committed picture/PCM agree at both joins, while independent
NTSC phase checks and the saved suffix verify retained audio clocks. Keep ordered
native input batches intact and retain a deliberately chosen empty-group slot
when a frame motion is clamped. Both regressions failed before their fixes.
Native release QA verifies both joins, one 14-frame placement at boundary 30
and one Undo. SQLite backups confirm the complete authored document is restored
with exactly those two new revisions; all 16 unrelated tables are identical and
the writer lock is released. Playback advances and pauses/resumes without a
visible starvation error; a complete loop wrap was not conclusively captured
in this pass. Native AX lists the slice controls. This does not resolve the
earlier Render-overlay AX omission or establish VoiceOver acceptance.

The initial seam `place-slice` replay passed 215 checks, including complete widget
Tab/Shift+Tab focus circuits, synthetic IME ownership, actual text/image paint at
960×640 and 1280×820, stale destination/reply handling, one commit and exact undo.
An actual media test compares decoded pixels and canonical nonzero PCM across
both joins against the committed result. See [the contract](../SLICE_PLACEMENT.md)
and [qualification](../qualification/slice-placement-2026-09-30.md) for authority,
evidence and limits. The replay does not qualify physical IME or acoustics.

Native release QA passes sustained looping, heading and focused-button
pause/resume, and cancellation; all rows in 20 SQLite tables remain unchanged
and shutdown releases the writer lock. The user's existing window is untouched.
Earlier debug audition starved: isolated preparation takes 1,065–1,194 ms per
170.667 ms buffer in debug, versus 21–31 ms in release with identical PCM/gain.
Retain this debug limitation; it does not qualify the broader workload matrix.
That checkpoint passed 2,592 workspace tests, 358 UI-feature app tests,
and strict lint/format checks.

Edited-slice move, replacement, picture/audio-only
policies and Repeat/Retime occurrence targets remain required by §9.7. DP-05,
DP-20 and all release gates remain open or partial.

## Owner preparation, 2026-09-30

CLI retain/relink/register/checkpoint now use the existing native import worker
and exact owning session. Do not synthesize native selection, rebase a late
registration, or release a worker slot before its cancelled reply drains.
Operational receipts survive bounded output and refresh failures independently
of authored revision receipts. Checkpoint publication retains the actual pinned
read revision and reports any post-rename durability failure with the saved path.

[Qualification](../qualification/owner-preparation-2026-09-30.md) records 28 real
CLI invocations and native catalog/picture/keyboard inspection, with 2,572 passing
locked workspace tests, 338 optional UI-feature tests, strict lint, formatting
and native startup/shutdown. Initial failures and scoped corrections remain in the
evidence. Current schema migration remains an independent read-only no-op.
Native relink/checkpoint controls and full preparation performance/failure
acceptance remain open. The saved-render boundary above supplies persisted
recovery using the existing Retry/Reconcile coordinator and bounded store queries.

The cursor-preservation gap found during this native inspection is fixed by
`preview::select_source`: reselecting the same registered video retains its
Original cursor and displayed picture during refresh. A production sound replay
first failed with Original 5 becoming 0 and Edit 2 retained, then passed after
the fix. It also checks return from Your edit and held picture refresh. See the
[focused qualification](../qualification/source-return-2026-09-30.md).
The separate keyboard `:source` view switch already retains the cursor; it still
clears the displayed picture during decode when leaving Your edit. That existing
view-transition behavior was outside this selection fix.

## Open-project routing, 2026-09-30

The native writer now advertises a private authenticated local endpoint for
structural edits, undo/redo, primary geometry and the public Render workflow.
Discovery binds the actual package and writable open; close/reopen revokes the
old owner. A request cannot silently fall back, retarget or replay after delivery.
Replies retain durable edit receipts separately from native UI updates. Remote
Render refuses unresolved Camera, Gain and Room tone drafts, retains its exact
workflow through later edits, and shares native cancellation and teardown.
See [the contract](../LIVE_PROJECT.md) for bounds and supported operations.

[Qualification](../qualification/live-project-2026-09-30.md) retains 36 CLI
invocations against the real native owner, three actual preview refusals,
concurrent editing during Render, exact cancellation and historical recovery.
Initial and retry movies pass all 768 independently decoded planes and complete
authored audio. The original workspace run and scoped corrections cover 2,523
distinct passing tests; 330 optional UI-feature tests, strict Clippy, formatting,
native startup/shutdown, 79 visual Render checks and 2,426 release checks pass.
No native status window appeared for CLI-started work; the short recovery finished
before native progress could be inspected. Preserve this visibility limitation
alongside the retained CLI progress/results and endpoint crash-test gaps.

Original retention, relinking, registration and checkpoints now prepare through
the service's shared import worker and complete through their exact owner.
Registration retains the caller's complete revision and insertion intent.
Checkpoints use a private consistent SQLite backup and retain the actual captured
revision. Operational receipts survive refresh/reply failures; a renamed
checkpoint with failed directory sync retains its receipt and reports the error.
Native persisted-job recovery is described above.
Section 20.5, DP-21 and all product gates remain open.

## Native and public Render, 2026-09-30

Native Render (`Cmd-E`, `:render`, or the visible control) and the public
closed-project headless commands now use the shared automatic SDR workflow.
Camera, Gain and Room tone drafts offer explicit commit/discard/keep choices.
Input is captured after native text processing; picker cancellation preserves the
draft. A typed `CommitAndStart` uses the exact durable preview commit receipt,
including when later refresh or render admission fails. Status and cancellation
remain separate from editor feedback. See [the contract](../RENDER_JOBS.md#native-and-public-render)
and [CLI commands](../HEADLESS.md#automatic-render).

The production `render` replay passed 71 checks, including actual publication,
later Undo during rendering, stale preview rejection and minimum/default layouts.
The public workflow passed start, stored status, checkpoint retry, reconciliation,
stale rejection, destination collision, SIGINT cancellation and JSON re-encoding.
[Qualification](../qualification/public-render-2026-09-30.md) retains 2,463
passing workspace tests, 318 optional UI-feature tests, strict Clippy, formatting,
native startup/shutdown, eight inspected final captures and 2,418 release replay
checks. Independent readers pass all 768 picture planes and complete authored
audio in both public exports. The accepted-generated-picture replay requires its
separate fixture and is explicitly skipped in this ordinary release run.

The open-project boundary above extends these commands to the native owner.
Prepared owner operations and persisted-job recovery are described above.
Full mastering, HDR, expanded output qualification and the complete
product scope remain open.

## Durable automatic encoding, 2026-09-30

Database 42 adds a strict automatic job policy and immutable decisions owned by
the original encoding attempt. Qualification runs while Queued; the writer
commits the exact decision and Encoding transition together before the worker
can consume its live admission. Store closure revokes probe and encode work.
Fresh encoding retries qualify again. Checkpoint retry and reconciliation retain
the original decision, controls and runtime while freshly verifying the movie.
Automatic manifests and publication provenance use schema 2; engineering schema
1 keeps its frozen shape. See [the contract](../RENDER_JOBS.md#automatic-admission-and-recovery).

Migration preserves legacy cells, rejects automatic vocabulary in old job and
nested publication intents, and creates the new decision table without masking
name collisions. Full audits reuse validated job heads and compact revision
summaries; targeted reads still check their own allocation head.

[Qualification](../qualification/automatic-render-jobs-2026-09-30.md) passes two
fresh native-app encodes, four verifications, reopen/retry and reconciliation.
Independent readers pass 768 picture planes and complete authored audio in both
files; every old cell in 19 tables survives. Final coverage is 2,439 workspace
tests plus 310 optional UI-harness app tests. Strict Clippy, formatting and native
smoke pass. The original full-run migration-expectation failure and a native
example's f32 JSON-comparison failure remain retained, with scoped corrections.

The entrypoints above now expose automatic Render with captured preview decisions
and owning-session validation. Keep full mastering/effects, HDR, scheduling and
release qualification open.

## Encoder runtime binding, 2026-09-30

The [automatic consumer](../AUTOMATIC_ENCODER_ADMISSION.md#consuming-a-fresh-admission)
now consumes one live admission for a committed project encode. Probe protocol 2
and report schema 2 carry actual mapped helper and Avcodec/Avformat/Avutil/Swscale
observations. Project protocol 3 requires the exact nullable runtime/control
binding. Matching descriptors stay open through work and are fully hashed and
revalidated before and after. Missing evidence or a runtime change stops the
attempt. This proves backing-object provenance under trusted installed code,
not resident-memory or OS/framework/driver attestation.

`EncodeContract::new_v1` freezes the existing controls, with `new` as its alias.
Historical manifests and database schema 41 are unchanged. The real qualification
consumer retained a verified 128-frame, 205,005-sample project file and fresh
canonical references; all 19 database tables remained unchanged. See
[qualification](../qualification/encoder-runtime-2026-09-30.md) for final checks,
actual hashes and limits. Independent readers pass all 384 planes and the full
authored audio. Final coverage is 2,397 passing tests after one protocol fixture
correction; strict Clippy, formatting and native smoke pass. Tiny probes report
192x96 coded geometry above their 256/256/4096-pixel budgets. Their admission
bounds remain unchanged.

The durable automatic boundary above completes this increment's next storage
step. Native/public headless Render remains the next product entrypoint.

## Automatic encoder probe, 2026-09-30

The [admission boundary](../AUTOMATIC_ENCODER_ADMISSION.md) generates deterministic
moving I420 and independent stereo markers at the requested raster/rate. Fresh
supervised probes advance only after specific admitted native failures. The
selected file passes complete structural/decode verification plus per-frame
pixel and exact event checks. Ordered rejected attempts and owned probe bytes
remain available; serialized reports cannot create a project output capability.

[Final evidence](../qualification/encoder-admission-2026-09-30.md): 2,384 workspace
tests, strict Clippy, formatting and native startup/shutdown pass. Four native
cases cover 190 frames and 24 independently decoded exact audio events. The
14x16, 16x16 and 64x64 probes explicitly fail the geometry guard. Existing bounds
already include macroblock padding; capture rejected dimensions/limits or SPS
before deciding whether those failures justify a decoder change.

Keep legacy final-file verification policy 1 unchanged. An absent-B verification
failure still stops this selector; safe typed absence requires finishing every
other check. Legitimate all-I/P project output needs a separately versioned rule.
That milestone identified helper bytes, kernel and native version observations.
The runtime-binding increment above adds loaded-library evidence and a fresh
project consumer.

The durable automatic boundary above now retains the policy and each fresh
encoding decision. Native Render and public headless access remain open.

## Typed encoder failures, 2026-09-30

[Encode protocol 2](../ENCODED_RENDER.md) preserves the failed boundary and exact
native kind separately from diagnostic text. Missing video encoders and actual
PTS-before-DTS packets have specific kinds. Source, DSP, I/O, capacity, control
and generic driver failures cannot become capability evidence through prose.
Later supervision faults invalidate a typed report; a validated failure terminal
permits exit 1, while other failing exits and malformed tails remain faults.
No automatic fallback is implemented and no stored policy/schema is upgraded.

[Native qualification](../qualification/encoded-failures-2026-09-30.md) reproduces
the real typed hardware-B rejection, then separately verifies a no-B project
encode without authored changes. The synthetic matrix preserves all ten working
paths and three rejected hardware-B attempts, with six deliberate fault cases.
All 2,359 locked workspace tests, strict Clippy, formatting and native smoke pass.

Next, implement bounded automatic admission using deterministic moving pictures
and audio at the actual output raster/rate for at least two GOPs. Arbitrary short
or frequently cut project content cannot establish B-frame support. Bind the
result to the current runtime and record rejected probes. Preserve strict legacy
engineering intents; automatic policy belongs to the job, while the selected
encoder and qualification evidence belong to each new encoding attempt.
Checkpoint verification/reconciliation must retain the original encoding decision.
Then expose the same policy through native Render and the public headless API,
with preview commit/discard decisions and requests bound to the owning session.

## Visual slice placement requirement, 2026-09-30

The user requested an elegant keyboard workflow for putting selected parts of
the video elsewhere while seeing the edit. [Section 9.7](DEADPAN_SPEC.md#97-visual-slice-placement)
now requires visible source endpoints, a provisional destination placement,
frame-accurate adjustment, audition around both joins and one-step commit/undo.
Insert, replacement and move retain exact timing, occurrence scope and owned
attachments. Enter commits; Escape cancels the unsaved proposal. Existing moment
copy/paste does not fulfill this requirement. DP-05/DP-20 acceptance must include
the complete keyboard flow and minimum-size visual inspection. Temporal slices
are the current interpretation.

## Durable destination reconciliation, 2026-09-30

The [publication journal](../RENDER_PUBLICATION.md#durable-publication-journal)
records Intent, Prepared, report authorization/commit and movie authorization.
Opaque exact-stage permits require a direct checked DB/WAL full-sync and namespace
barrier after COMMIT. Barrier failure leaves the row visible, revokes permits and
requires reopen. Media work and full hashing remain off the writer.

Reopen interrupts active operations and never changes destination entries.
Explicit reconciliation requires a newer same-checkpoint verification attempt,
its live candidate, APFS volume/birth/inode/path identities and full byte hashes.
A matching final movie with a missing/damaged report remains committed but
unconfirmed; a replacement with identical bytes stays unresolved. Keep recovered
file locks through the final journal transaction. Schema 41 preserves all prior
schema-40 cells and core 33; earlier migration adapters remain intact.

[Native qualification](../qualification/publication-recovery-2026-09-30.md)
passes 20 SIGKILL cases, 40 fresh verifications and exact authored/history
preservation. Independent FFmpeg and AVFoundation readers pass 138 pictures,
221,021 authored sample frames and 414 image planes. The full workspace passes
2,318 tests, strict all-target Clippy and formatting on the native-qualified source.
Kills occur between completed host calls; physical power loss is not qualified.
Native Render, public headless
rendering, automatic policy, scheduling, full audio/effects and HDR remain open.

The shared `encoded_render::workflow` coordinator now connects start,
cancellation, checkpoint retry and publication reconciliation. Heavy stages use
one bounded worker; the native project service owns journal transactions and
retains its writer through close/switch/shutdown. Explicit process cleanup
evidence gates terminal failure, and render feedback remains separate from
editor feedback. Public Render still needs real automatic policy admission and
product controls; current encoder choices remain explicitly engineering-only.

[Workflow qualification](../qualification/render-workflow-2026-09-30.md) passes
two complete encodes, a real encode cancellation, six fresh verifications,
reopened checkpoint retries and reconciliation. Edits and undo/redo run during
encoding without retargeting its captured revision. Independent readers pass
138 pictures, 221,021 authored sample frames and 414 complete planes. The final
workspace passes 2,347 tests and strict all-target Clippy. A reproduced native
shutdown notification failure is corrected and its regression passes.

## Durable encoded checkpoints, 2026-09-29

The [render job boundary](../RENDER_JOBS.md) persists immutable render intent,
fresh attempts and exact transition sequences outside authored history. An I/O
worker retains complete movie and strict manifest objects in a separate bounded
`Media/RenderCandidates` namespace. The SQLite writer checkpoints opaque session
and attempt tokens using cheap freshness checks.

Writer reopen interrupts nonterminal attempts and preserves their checkpoints.
An explicit new attempt freshly hashes both retained objects, reconstructs the
captured historical contract and invokes the isolated finished-file verifier.
Stored reports cannot create a live verified candidate. Closing the owning store
revokes retained handles; final verifier admission checks session liveness.
Schema 40 adds operational tables while preserving schema-39 authored JSON and
patches byte-for-byte; older schemas retain strict replay.

[Qualification](../qualification/render-jobs-2026-09-29.md) passes two real
restart/reverification cases and independent readers over 138 pictures, 221,021
authored sample frames and 414 complete planes. The workspace has 2,284 passing
tests after correcting and rerunning three failed targets; all other full-run
results are retained. Strict workspace Clippy and formatting pass. Evidence
includes authentic schema-39 databases, final schema-40 backups, actual movies,
readers, source differences and independent reviews. That checkpoint qualified
orderly writer restart; subsequent process-death coverage is described above.
Physical power-loss injection remains open.

The subsequent publication journal is described above. Next implement the complete
native Render and public headless workflow, using the qualified staged APIs. Scheduling,
automatic platform policy, full mastering/effects, HDR and all release gates
remain required. See the contract for ownership, limits and API sequencing.

## Verified destination publication, 2026-09-29

The library [publication host](../RENDER_PUBLICATION.md) now takes a private
verified candidate and an explicit MP4 destination. It pins the destination
parent, stages exclusive sibling partials, checks exact destination bytes and
publishes the local report before atomically renaming the movie without replacing
an existing entry. Before that rename, failure preserves the verified candidate
and diagnostic recovery paths. After it, bounded integrity and durability checks
finish despite late cancellation; a failure returns `PublishedUnconfirmed`.
The report and movie are separate commits, so an orphan report can remain.

Provenance binds the captured historical revision and full document hash. Source
receipts supply original object identities and SHA-256; the catalog is explicitly
a committed superset. Effective Generated intervals follow the indexed picture
resolver through sparse plays, gaps and retiming, carrying complete artifact and
immutable provenance identities. Source labels, URLs and linked paths are omitted.
Capacity limits fail explicitly and do not truncate the report.

[Qualification](../qualification/render-publication-2026-09-29.md) passes seven
fresh project publications and independent final-file decode: 304 pictures,
390,695 authored sample frames, 912 complete planes and 18 exact audio markers.
A separate audit checks report/movie hashes, historical SQLite document hashes,
Original receipt/byte identities and the exact `[0, 30)` Generated interval.
All 2,248 locked workspace tests pass, with zero failed or ignored, as do strict
workspace Clippy and formatting. Independent review corrected cancellation-code
loss and a post-rename content-check gap; both corrections have passing tests.
The evidence retains actual bytes, SQLite backups and exact source/binary identities.

Native Render, public headless render commands,
automatic platform policy, complete mastering/effects, HDR and release
qualification remain open. No DP requirement or Gate A through G is complete.

## Isolated finished-file verification, 2026-09-29

The [finished-file verifier](../FINISHED_FILE_VERIFICATION.md) now inspects a
private encoded candidate in a separate supervised process. It binds the bytes,
document and captured contract, checks actual MP4 tables and edit lists, decodes
every picture, and compares every complete GOP with a fresh decoder starting at
its actual IDR. Manual and ordinary AAC modes must cover the exact authored
sample interval and agree at fixed sample coordinates without PCM realignment.
Clean teardown admits the report; failure retains the candidate for retry.

[Current qualification](../qualification/finished-file-verification-2026-09-29.md)
passes seven fresh project MP4s through normal and instrumented production
verifiers: 304 pictures, 390,695 authored sample frames and 21 GOPs per run.
Independent FFmpeg/AVFoundation readers pass all 912 picture planes and 18
audio marker observations at exact fixed coordinates. The workspace passes
2,222 tests, Clippy and formatting. Native C ASan/UBSan passes 241 selected
tests and reinspection of all seven fresh files. The evidence records exact
source/binary identities and the instrumentation limits.

Source admission remains bounded to 64 GiB, one million packets, a 16 MiB header
and 16 MiB packets, with bounded aggregate tables. This is narrower than the
encoder's capacity and fails explicitly. Structural/decode admission does not
replace content, event-sync or hardware/runtime qualification. See the linked
contract for the supported SDR interpretation and remaining limits.

The library publication and retained-checkpoint boundaries are described above.
Next add durable publication recovery, native Render and public headless render commands. Complete mastering,
remaining audio/picture effects, HDR, release-runtime coverage and the full
specification remain required. DP-17 stays open; every DP requirement and Gate A
through G remains open or partial. No native export workflow is complete.

## Isolated committed SDR encoding, 2026-09-29

The [encoded render child](../ENCODED_RENDER.md) now streams one committed
revision's real pictures and canonical PCM directly into the native encoder.
Both readers bind the complete document hash and exact range before native
allocation. Strict separate messages, the shared process supervisor and contained
snapshot admission return private MP4 candidates after clean teardown.

[Qualification](../qualification/encoded-render-2026-09-29.md) retains seven
actual candidates, 304 pictures and 390,695 authored sample frames. Normal and
sanitized independent readers pass complete picture/PCM comparisons, exact
clocks/edit lists, 21 fresh GOP boundaries and all marker events. Real live
edit/undo/redo, cancellation after progress, byte exhaustion and retry pass.
Sanitizers cover the independent readers here; the native encoder has its
separate preceding qualification. The production structural/decode verifier is
now implemented in the newer boundary above.

The locked workspace passes 2,182 tests, zero failed/ignored; strict all-target
workspace Clippy, formatting and 123 Python tests pass. Independent review fixed
qualification bounds/SAR checks and strengthened progress-fault fixtures, whose
seven integration tests pass separately. Evidence retains the initial failures,
actual bytes, SQLite backups, source/binary bindings and review dispositions.
No app UI changed.

The [isolated verifier](../FINISHED_FILE_VERIFICATION.md) now reuses the
descriptor-only source decoders and bounded MP4/packet observations. Its source
capacity and content/runtime qualification limits remain explicit. Library
publication now exists. Next add durable publication recovery, native Render and public
headless render commands.
Full audio/effects, HDR and every DP requirement and Gate A through G remain in
scope and incomplete.

## Native SDR encoder and offline audio, 2026-09-29

The [native encoding boundary](../NATIVE_ENCODING.md) now writes bounded
descriptor-only H.264/AAC MP4 from composed I420 and finite canonical stereo PCM.
It enforces exact chronological inputs, one deadline, explicit encoder attempts,
poisoned failures, complete codec EOF and restricted same-file fast-start.
`OfflineAudioSession` captures one historical revision and B(end)-B(start),
preserves limiter/source context, and shares the deadline through source index
comparison and provenance hashing. The existing inspection API stays unchanged.

[Qualification](../qualification/native-encoding-2026-09-29.md) retains normal and
ASan/UBSan files, all three audio readers, exact MP4 edit-list observations and
fresh-decoder GOP suffixes. Each matrix passes ten usable cases and six deliberate
failure checks. Hardware B-frame attempts still fail PTS<DTS on the measured
M5 Max. Hardware without B-frames and explicit OS software attempts are separate
measured paths; software requested for two B-frames emits one. No fallback occurs
inside an encoder session.

All 2,159 locked workspace tests pass, with zero failures or ignored tests.
Strict workspace/all-target Clippy, formatting and 114 Python oracle tests pass.
The evidence archive retains and verifies all 836 native result files, including
rejected attempts and partial outputs. No app UI changed in this milestone.

The integration above now feeds committed pictures and canonical PCM directly
into this encoder inside the supervised child, followed by the separate
[finished-file verifier](../FINISHED_FILE_VERIFICATION.md). Library publication now
exists. Add durable publication recovery, native Render and public headless render
commands.
Do not promote a synthetic adapter fixture into product export evidence. Full
audio/effects, HDR, release hardware/OS coverage and all DP-01 through DP-24 and
Gates A through G remain open or partial.

## Isolated committed picture worker, 2026-09-29

The [render worker](../RENDER_WORKER.md) now owns real decode, Metal composition
and bounded raw I420 output in a separate process. Host and child independently
bind the complete authored document hash and every exact output-contract field.
The shared process supervisor retains checked launch, group/reaping ownership,
bounded pipes and clean-exit admission through separate generation/render
protocol adapters. A returned range owns a verified private snapshot.

[Qualification](../qualification/render-worker-2026-09-29.md) passes 2,140 locked
workspace tests, zero failed/ignored, strict workspace/all-target Clippy and
formatting. Actual Metal passes 118 direct and 128 worker checks, including all
82 isolated frames, 31 independent complete-plane comparisons, live writer
edit/undo/redo, cancellation after real progress and a successful new request.
The macOS raw/fixed pipe-inheritance witness also passes. Source inventory
`1c9362b0…` binds all final checks and native artifacts.

Independent review fixed diagnostic overwriting, interrupted-copy error mapping
and interrupted control-drain handling. Retained evidence includes these findings,
the initial compile failure, terminal command journals, exact worker/example
binary hashes and every synthetic actual/direct/reference plane. No app UI or
native interaction changed. No compiler, test or native probe is left running.

The native encoder and bounded offline audio reader now exist as separate
boundaries described above. The separate encoded child now connects them.
Do not spool a full uncompressed product movie through the current 512 MiB raw
qualification range. The existing conversion worker still owns whole-file FFV1
conversion; it is not the H.264/AAC encoder.

The [encoded-file verifier](../FINISHED_FILE_VERIFICATION.md) now provides the
bounded structural/decode checks described above. Library publication is also
described above. Durable publication recovery, full audio/effects, native Render,
HDR and all remaining product requirements stay open. No DP requirement or
Gate A through G is complete.

## Committed encoder pictures, 2026-09-29

The [encoder picture host](../EXPORT_PICTURES.md) owns one captured revision,
range, exact output clock, legal even raster and completed I420 frame. Framing
still uses the unchanged authored canvas. Source PTS and absolute project frames
remain separate from relative output timestamps; both absolute audio boundaries
are retained. It reuses the actual Original/Generated reader and shared Metal
composition/readback, with cancellation/deadline checks and bounded ownership.

[Qualification](../qualification/export-pictures-2026-09-29.md) passes 118 actual
Metal checks with 52 retained frames. All 30 Generated frames and one odd-canvas
Original match independent complete-plane references within one code value;
93,396,906 codes were compared. Exact nonzero-range clocks, held-result rejection,
captured framing and live writer edit/undo/redo/close are covered. Three independent
reviews found no actionable defect. No app UI or native interaction changed.

The full locked workspace passes 2,106 tests, zero failed/ignored. Strict
workspace/all-target Clippy and final formatting pass. One initial harness
SHA-256 formatting compile failure and its correction remain in the evidence.
Corrected checks and Metal use the same source inventory `66cc8615…`; only docs
and archived evidence changed afterward. No compiler, test or native probe is
left running. Keep these results unless a later code change affects them.

The later [encoded worker](../ENCODED_RENDER.md) now isolates this real producer
and canonical PCM behind separate render messages and checked supervision. The
[finished-file verifier](../FINISHED_FILE_VERIFICATION.md) adds independent
structural/decode admission. Library publication is described above. Complete the
shared audio/effects graphs, durable publication recovery and native Render. This
picture boundary alone produces no encoded file or export control. All DP
requirements and Gates A through G remain in scope and incomplete.

## Accepted Generated Hold pictures, 2026-09-29

The [shared picture reader](../PROJECT_PICTURES.md) now admits schema-3 Generated
Holds from retained project media in native preview and captured-revision
preparation. It checks all six objects, strict provenance, both asset records,
canonical decoding, every sampled PTS and measured terminal duration. Its
revocable package handle and one private decoder preserve accepted historical
pictures through request staleness, relocation and prefix resizing. It needs no
model, current candidate selection or worker path. Legacy Accepted without this
evidence, Still and HDR remain explicit failures.

The [qualification](../qualification/generated-pictures-2026-09-29.md) retains
real FFV1 conversion/acceptance, all 30 exact RGBA frames, damaged-object cases,
undo/redo/revert and independent review. The full locked workspace passes 2,094
tests; after app-only layout changes, base app passes 268 and optional harness
304. Strict Clippy and formatting pass. Generated visual replay passes 108 checks
plus audit; the ordinary visual run's room-tone failure is corrected by a
220-check scoped continuation. Final release replays pass all 2,348 ordinary
checks and 367 Generated checks plus its audit. Full source inventories bind
each continuation. No compiler/test/replay process remains running.

The Hold inspector now exposes Picture, Sound and duration action first. A
stronger paint-order check and image review caught hidden mode/focus text:
inactive Gain/Sounds panels retained decoration that let later panes cover the
footer. Empty panels now retain their IDs without decoration. The read-only
clock row uses compact height where needed; the copied-Original minimum picture
is 141 points. Room-tone scroll checks reveal the complete fact/action group.
Failed captures and checks remain in the evidence. The generic Generated
fixture's minimum picture remains small; full editor visual acceptance is open.

Only app layout and harness sources changed after that milestone's backend
gate, with scoped tests and release replay covering them. Command journals bind
terminal status and source inventories. Inspect live PIDs and journals after
interruptions before starting another Cargo process; quiet compilation does not
justify a restart. App model management, real candidate audition/acceptance,
analysis, remaining editing/effects, recovery and signed distribution remain
required. No DP requirement or gate is complete.

## Earlier completed evidence

The [SDR encoder timing experiment](../qualification/encoder-timing-2026-09-28.md)
finds native AAC events and the stream endpoint 1,024 samples late when edit
lists are disabled, exceeding one frame at 60 fps. Default-edit-list references
are sample-aligned. Retain the failed
files and both decoder modes; do not hide priming by shifting/cropping PCM or
widening tolerance. The [independent AVFoundation comparison](../qualification/native-audio-2026-09-28.md)
finds missing opening events and later events 1,088 samples early in the same
disabled-edit-list file; the default reference aligns. The user approved the
§22.3 revision on 2026-09-28, applied on 2026-09-29: edit lists may represent
encoder delay, padding and frame reordering, with explicit stream-start/sync
and full emitted-file verification. Do not reopen this decision. The newer native
encoding and finished-file boundaries above add measured GOP/video evidence and
production structural/decode checks; full product export remains open.

The [shared SDR encoder pixel boundary](../SDR_ENCODER_PIXELS.md) snapshots
the composed linear working target into bounded owned memory and converts it
to explicit Rec.709 limited-range, left-sited I420. Preserve its signed working
values until the output transform and keep output timestamps separate from
source PTS. Cancelled GPU work retains its permit until callbacks drain.
The synthetic video-only encoder experiment does not qualify AAC timing under
the approved mux policy; neither boundary supplies a project export worker.
Its [qualification](../qualification/sdr-encoder-pixels-2026-09-28.md) retains
the full workspace pass, 22 actual Metal checks and normal/sanitized H.264
pixel comparisons. Later boundaries above add final-render isolation and file
verification. Legacy Accepted/Still readers and the full export workflow remain open.

## Product in one paragraph

Build a native macOS, Rust-first, Vim-style instrument for massaging one original video into a weird YTP. A project chooses one local or YouTube video and starts with its full unedited timeline automatically. Cuts, repeats, pauses, reframing and effects remain reversible structures on that original. Reuse moments from the same video, add external audio-only effects, and explicitly accept local AI Hold extensions. Do not offer additional video imports. New native projects live in Documents/Deadpan regardless of launch or source location. The full product still includes analysis, recovery, actual local generation and one-action source-derived YouTube output without external end-user runtimes.

## Read first

[Sound catalog audition](../PLAYBACK.md#sound-catalog-audition) uses the shared
canonical playback service with its own sample clock and leaves the retained
picture and edit selection untouched. Follow the
[sound design board](../design/boards/sound-audition-board-v1.png) for focused
selection, visible Space/Shift+Space keys and explicit state. It does not place
sound events. The [sound integration contract](../SOUND_EVENTS.md) distinguishes
node-owned clocks, continuous per-voice processing and scoped Hold allowances.
Its exact route kernel and CLI LRU PCM cache support the persisted root subset;
nested ownership, the remaining edit transforms, voice effects and full final
mixing remain required.
The [retained sample-route evaluator](../SOUND_EVENTS.md#retained-sample-routes)
now composes each edit's physical-grid cut and new anchor. Keep the old selected
audible mask separate from complete recipe/filter/DSP support. Current Hold
queries retain issuer identity; they grant no allowance and invent no historical
policy. Playback also uses bounded LRU source eviction. Complete per-voice
processing remains required. See
[sound-clock qualification](../qualification/sound-clocks-2026-09-27.md).
The [independent source operand](../SOUND_EVENTS.md#independent-catalog-source-operands)
now feeds qualified catalog audio through the existing tape and PCM engine without
adding a Source node. Its input retains complete DSP context; its output separately
applies current scoped silent-Hold rules. Preserve still requires the checked
owner/descendant relationship. Do not inherit Original bindings or use metadata
as permission to bypass host admission.
The [routed PCM readers](../SOUND_EVENTS.md#routed-pcm-preparation) connect retained
sample routes to complete source or projected providers on checked PointCeil and
RoundEven clocks. They read the old sample labels with full filter/DSP support,
including cold suffixes, and admit dependencies even for entirely masked output.
Captured provider policy stays separate from current consuming Hold gates. See
[routed-voice qualification](../qualification/routed-voices-2026-09-27.md).
Core 29/database 35 added qualified persisted root events and the shared
pre-master bus. Core 30/database 36 introduced chronological root sound routes
through InsertTime, SpliceSource and ordinary Sequence Delete. Non-root Split
is neutral; root Split and temporal occurrence edits remain guarded. Preserve
the complete recipe and old physical sample labels while current Hold gates
stay live. Parameter changes keep routing; explicit ReplaceSound discards it.
Frozen core 29 checks historical contextual admission before modern replay.
See [persisted root ripple edits](../SOUND_EVENTS.md#persisted-root-ripple-edits).
The [native root placement subset](../SOUND_EVENTS.md#native-root-placement) now
places the complete measured catalog sound with `,s` or `:sound-place` at the
retained Edit cursor. A separate Placed sounds pane owns event selection and
`j/k`, durable exact-frame `h/l` nudges, Enter for a whole 48 kHz sample onset,
3 dB `+` / `-` steps, soft/hard endpoints and `dd`. Edits use normal durable
history; picture duration never grows and overflow is rejected. Parameter entry
captures event/session/revision, including rejection when no event was captured.
Routed gain and edge edits retain their journal; native move/nudge rejects it.
Follow the [placement board](../design/boards/sound-placement-board-v2.png) and
the [native placement qualification record](../qualification/native-sound-placement-2026-09-27.md)
for the implemented subset and its verification limits. Nested ownership,
Repeat/Retime sound transforms, send/tail allowances, effects, the remaining
structural transforms and export remain required. No requirement or gate is
complete because these controls exist.

Core 31/database 37 add explicit per-sound, per-concrete-Hold allowances.
`:sound-allow` and `:sound-silence` capture the selected event, Edit frame,
issuer, session and revision. The writer re-resolves that scope and rechecks
source admission. Granting one sound leaves Original audio, other sounds and
other pauses suppressed. Raw preparation precedes current contribution gates;
an allowance cannot create sound in a retained route gap. Split and occurrence
isolation remap exact identities; database-36 histories replay through frozen
core 30 and gain no permission. See [sound allowances](../SOUND_EVENTS.md#persisted-root-sound-allowances).

Core 32/database 38 add [atomic Hold audio authoring](../ROOM_TONE_AUDIO.md).
`SetHoldAudio` and its occurrence form retain picture, duration and sample clocks,
reconcile changed audio lineage, and remove only that Hold's obsolete silence
permissions. Undo restores both policy and permissions. The store requires
revision-bound qualification and measured sample endpoints for new source
choices; unrelated legacy Hold recipes remain unchanged. Database-37 history
uses frozen core 31 with exact allowance comparison. The
[room-tone design board](../design/boards/room-tone-board-v2.png) now guides
`:room-tone` and `:hold-silence` for ordinary selected Holds. Copy Original time,
inspect inward-snapped source samples, audition on a separate audio-only clock,
then explicitly Apply. Native fields preserve IME and button/key ownership.
Reopen the saved range; replacing it from the captured copy is explicit.
Preparation successes and failures carry request/session/revision identities.
Waveforms, occurrence controls and acoustic qualification remain open.

[Gain contracts](../AUDIO_GAIN.md) add persisted node treatments, direct and
occurrence setters, exact owner clocks and canonical post-mapping PCM gain.
Core 33/database 39 replay database 38 through the closed core-32 adapter.
Context schema 4 retains a sparse treatment map separately from timing-only
`FrozenAudioLayout`. CLI `inspect-audio --authored-bus` exposes the pre-limiter
result. Native `+`/`-`, `:gain <dB>` and `:gain-mute` capture a selected ordinary
beat; Placed sounds retain their separate gain target. `:gain` opens a buffered
editor for trim, mute, exact owner-output envelopes and mute ranges, with a
single explicit Apply. Use the [gain board](../design/boards/clip-gain-board-v2.png)
for the panel hierarchy and retained picture.
Keep gain after complete time/pitch mapping and edges, with exact independent
owner clocks and unchanged continuous Preserve history. Root-owned sounds receive
only their own gain and root treatments, never an unrelated Source's gain.
Temporary drafts retain explicit content identity and the same delivered sample
window; a matching base revision alone cannot authorize cache/resume reuse.
Writer previews produce validated, unstored documents while media admission
remains anchored to the committed entry snapshot. Apply rechecks the captured
target and commits once; cancellation retains the accepted picture and restores
entry context only in the same session and revision.
Follow the [native gain integration design](../GAIN_EDITOR_DESIGN.md) for the
proposal/admission boundary and production-router verification.
See the [native-gain qualification](../qualification/native-gain-2026-09-28.md)
for actual verification, including native macOS command/focus/text/cancellation
and the separate 2,156-check release replay. The warm picture and 10,000-beat
CPU measurements retain their small-fixture/offscreen limits.
Waveform editing, physical keyboard/IME, VoiceOver, listening, complete audio processing
and DP-09 acceptance remain open.

The [measured beat overview](../WAVEFORMS.md) is implemented with scoped
[qualification](../qualification/gain-waveform-2026-09-28.md). Preserve the captured
committed owner independently of gain proposals, complete signed min/max bins,
unknown coverage and exact terminal owner clipping. Analysis uses the same
preparation owner as playback and waits for controller-confirmed output
quiescence. Retained peaks keep their memory reservations; a new request admits
source evidence afresh. Stale errors cannot disable a valid gain edit, and Retry
remains in the native modal keyboard circuit.
Wide layouts pair exact fields with the editable curve below the overview;
minimum-size layouts retain one scroller and fixed comparison/commit actions.
The image review caught and corrected fields scrolling away from their curve.
The final full release replay passes 2,348 checks across 18 scenarios; retain
the qualification's small-fixture and offscreen measurement boundaries.

The [compact workspace qualification](../qualification/compact-workspace-2026-09-28.md)
records empty Sounds consolidation into the Beats heading in short
single-Original edit views. Preserve its distinct focus target, measured label
width, scrolling breadcrumbs and empty panel ID. Capture placement once per
render pass and reject changed placement before picture submission. Room tone
is an overlay over the same layout; Gain/Camera and populated sound lists retain
their existing layouts. The final source passes app/painted checks within that
record's scope; release performance is recorded separately.

[Structural speed editing](../RETIME_EDITING.md) exposes `:retime` and
`:wrap-retime` through exact speed resolution and the native inspector. Preserve
the input range when adjusting an ordinary Retime; wrap split Partitions instead.
Only a changed stage's own retained output binding resets. Descendant source and
DSP input bindings remain intact. Core 28/database 34 close the old command
vocabulary through `legacy_v27`; old histories cannot gain new speed operations.

The current [Original/edit audition contract](../PLAYBACK.md) describes the native
Space Play/Pause and Shift+Space selection-loop increment, exact paused sample
retention and the limited edge-faded bus. Whole Original playback includes its
full A/V union; selected moments use measured picture endpoints before adding
audition context. It does not qualify the full mastered preview/export pipeline or
reduce the requirements below.

[Authored framing and Camera](../FRAMING.md) records the current implementation
contract and remaining work, including saved targets and tracking.
[Captured framing](../CAPTURED_FRAMING.md) preserves a pause's input composition
separately from its provider and live Camera operations. Use the dedicated [Camera design board](../design/boards/camera-framing-board-v1.png)
alongside the primary workspace target. A temporary preview must remain distinct
from a committed edit, and opening Camera must preserve existing curves.

[Exact Original moments](../SOURCE_MOMENTS.md) records measured range candidates
and selected audio placements, including physical-grid endpoint behavior and the
core-20/database-26 migration boundary. Core-27/database-33 add atomic
`SpliceSource` at an explicit ordinary Sequence slot. Native `v`/`y` selects and
copies an Original range; `p`/`P` pastes after/before the selected beat. Keep the
copied session/asset/receipt identity and captured revision/scope through
preparation. Admit the prepared existing receipt and Original freshness in the
same history/relevance transaction. Do not use separate Split/Insert commits.
The new Source begins unbound on the canonical project grid while each old
suffix owner retains its sample entry. Frozen core 26 preserves DB32 nested
pause history and rejects the new command. Persistent registers, arbitrary
occurrence/cursor splice and Visual replacement remain required. The
`original-moment` harness exercises the keyboard path when Metal is available.

[Compact audio reanchors](../AUDIO_REANCHORS.md) adds core-21/database-27 ordered
per-occurrence resume steps and retained allocation queries. Distinguish hidden
allocation from meaningful raw support and preserve each step's lexical scope.
Core-22/database-28 [gap bindings](../GAP_AUDIO_BINDINGS.md) extend this ownership
to configured Repeat gaps, including gaps with no current occurrence. General
atomic moment splice remains required.

Core-23/database-29 [editable gap branches](../REPEAT_GAP_BRANCHES.md) retain
independent subtrees after stable plays and materialize current default gaps
without changing their audio clocks. Final-play branches stay dormant until a
following play exists. Use these owned structures in the general splice author;
the primitive commands do not yet resolve an arbitrary cursor insertion.

Core-24/database-30 [pause insertion](../INSERT_TIME.md) admits existing root
Sequence seams before composite suffixes. Capture current placements without
replacing retained lattices; append windowed steps and stop at nonunity Preserve
outputs. Every older replay checks the frozen contextual command boundary before
modern apply. Core-25/database-31 add root Source/ordinary Hold fragment interiors
before composite suffixes. Capture sampling before Split and placements afterward
under separate timing identities; preserve each occurrence's own rounded entry.
Frozen core 24 admits its old seams but refuses these new interiors. Arbitrary
nested splice and general Visual replacement remain required. The native editing
replay includes both pause-before-Repeat and interior-pause/undo paths.

Core-26/database-32 extend that command into unretimed Sequence groups. Use
`insert_time_target` for the exact native capture parent and required Split IDs.
Capture only scopes below that parent; keep every ancestor live. Reanchor later
siblings at each Sequence level, stopping at physical Preserve outputs. Frozen
core 25 refuses this broader context when replaying database 31. Native edit
completion carries the captured cursor. Native [Sequence navigation](../GROUP_NAVIGATION.md)
adds Enter/Backspace, breadcrumbs and direct-child editing at each ordinary
Sequence depth. A deeper Hold selects its visible enclosing group until entered.
The `nested-pause` harness exercises insertion, navigation, duration, history and
Camera when Metal is available. This does
not implement fractional clocks or insertion under Repeat/Retime ancestors.

Use `AnchorIndex::locate_boundary` for exact project-to-content descent. The
[headless query](../HEADLESS.md) exposes all owner clocks, Sequence slots, stable
Repeat identities, distinct play/gap entries and implicit-gap terminals under
shared work bounds. This query performs no mutation.
The [splice design](../STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent) records
the required ownership and resume work that remains. Do not use picture-center
sampling or flatten a Repeat out of its live group to choose a splice target.

The preferred [derived-clock representation](../STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks)
keeps authored integer durations separate from exact effective extents, including
nested Retime output. It is not yet a document capability. The shared framing
evaluator now accepts exact derived extents without rounding or overflowing
intermediate quotients; existing integer callers use the same implementation.
Do not implement subtree duration dilation as an audio-preserving shortcut:
Preserve processing depends on its physical input grid and retained history.

The borrowed [audio input tape](../AUDIO_INPUT_TAPES.md) projects current scoped
signals onto one intrinsic PointCeil grid and reads actual PCM through the shared
`StageAudio` path. Allocation seams never restart phase or crop filter support.
Checked `AudioStageProjection` views retain full intrinsic input/output history;
parent tapes consume child intrinsic output, while a separate PointCeil schedule
can place pauses around it. A request-local identity memo prevents duplicate
preparation and descriptor aliasing.
[Projected root placement](../AUDIO_PROJECTED_ROOT.md) separately allocates one
physical projection on the absolute RoundEven grid, preserving phase, exact
policy and support exhaustion through crops and repeated resumes. These are
borrowed evaluation views, not authored splice routes: exact effective owner
clocks, persistent lifecycle, aggregate scheduling and normal root-plan
integration remain required.

Use the [UI feedback loop](../UI_FEEDBACK.md) for every meaningful interaction
change: replay production keyboard, pointer, wheel and text paths, inspect actual
Metal captures, and run the separate release latency checks. The
[interaction review](../INTERACTION_REVIEW.md) records measured friction and
priorities. Reserve Kestrel's global shortcuts; whole-Original reuse is `,i`,
while Cmd+Return belongs to Kestrel. Update routing, visible hints, help and replay
together. A passing replay does not establish native IME, VoiceOver or physical
display behavior.

Read Sections 1–8 for product/primitive/keyboard semantics, 12–14 for AI contracts and qualification, 17–22 for rendering/runtime/storage/export, and 23–30 for dependencies, tests, requirements, and build gates. Section 31 resolves command targeting and source-browser behavior. Source references are in Section 33.

## Decisions already made

- Working name: Deadpan; `.deadpan` project directory packages.
- One pinned Original per new native project, with a full-source starting timeline and protected undo baseline. SQLite retains this workflow identity independently of reversible presentation state.
- A global system Documents/Deadpan project library; initial source-picker cancellation creates nothing and failed preparation remains explicitly recoverable.
- Original/moments plus a separate sound-effects collection. Existing generic backend and legacy multi-video projects remain valid in an explicit compatibility workspace; never discard their data to fit the new UI.
- Visible keycaps, pending-prefix guidance and a distinct focused-pane cue teach ordinary actions. Searchable contextual help supplements the interface.
- Native Rust UI with egui/eframe/wgpu on Metal; no browser shell.
- New domain core, not a whole-app fork of a general editor.
- Pinned FFmpeg/native media adapter; qualify Cutlass components only if extraction reduces complexity.
- Exact rational frame/source timing and 48 kHz sample coordinates.
- Source/Sequence/Hold/Repeat/Retime nodes with stable IDs, anchors, occurrences, and attachments.
- Same rendering and DSP semantics in preview and export.
- SQLite authoritative storage; no competing mutable JSON document.
- AI worker language/backend chosen for measured usable-output latency, not MLX loyalty or Rust purity.
- Immediate committed freeze fallback; actual generated candidates require explicit acceptance.
- Bundle yt-dlp, its JavaScript/EJS support, and all executable runtimes; model weights install in-app or from an approved offline pack.
- Full V1 scope includes every revised DP-01 through DP-24 requirement. The single-original policy is intentional; ordered gates do not excuse missing required creative operations.

## First concrete work

Create a dependency/architecture decision log and the workspace boundaries in Section 24. Run Gate A technical harnesses for actual macOS media decode/seek/encode, GPU preview, audio DSP/output, model inference, and private-runtime packaging. Record exact revisions, licenses, true output behavior, and measured hardware results. Do not spend the first implementation pass decorating a timeline while leaving timing, generation, and export unqualified.

In parallel, implement the pure core with generated fixture documents and property tests. Establish command resolution, exact duration math, reversible transactions, serialization, and render-plan inspection before connecting widgets to mutable state.

## Completion discipline

Playback tests reserve real-media work before fixture preparation and retain
the permit through the shared engine callback until both workers exit. Use the
existing test helper for new PCM scenarios; `Engine::drop` and `Stopped` alone
do not establish teardown. The
[scheduling record](../qualification/playback-waits-2026-09-27.md) retains the
reproduced preparation timeouts and the scope of this test-only correction.

Maintain a requirement tracker mapping DP IDs to implementation, tests, and evidence. Every operation must be editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A UI button, mock worker, successful model download, or ignored test does not count as implementation.

If a dependency fails qualification, preserve the product contract and replace the implementation behind its interface. In particular, slow or unreliable video generation is a measured engineering issue: do not silently redefine AI holds as freeze frames or require a manually installed external application.

## Non-negotiable correctness points

`3riw` means three total plays. Repeat gaps occur only between plays. A Hold inserts exactly N project frames and resumes untouched original speech. Do not accumulate fractional-rate duration rounding. Jobs cannot overwrite newer edits. Accepted generated media remains usable without the model. Export snapshots cannot mix revisions. Cache cleanup cannot remove referenced originals or accepted artifacts.

Undo cannot erase the original identity or cross its initialization baseline.
Deleting all current beats does not make another video eligible. Importing sound
does not imply placing it, lengthening the edit or replacing original speech;
sound-event overlay requires its actual authored and mixing path. Do not relabel
a generic blank-picture audio beat as a placed effect. Maintain core structural
capability and strict legacy migration while enforcing V1 through the optional
profile and native workflow.

## Delivery

Deliver the complete source, signed/notarized application distribution, approved model-pack manifests, clean-machine online/offline test evidence, benchmark report, keyboard guide, fixture/verification reports, migration policy, and third-party notices/SBOM. Any unfulfilled required behavior remains explicitly open rather than being described as finished.
