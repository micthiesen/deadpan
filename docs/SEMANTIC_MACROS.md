# Semantic macros

`q` plus a letter records a Macro in a project register. While recording, `q`
saves. On Your edit, Escape clears an existing Visual selection and records that
action; without a selection it cancels the draft. On Original it cancels the
draft and retains the independent Edit selection. `@` plus a letter runs a saved Macro;
`3@a` runs `a` three times in one transaction. The command equivalents are
`:record a`, `:record-stop`, `:record-cancel` and `:macro a 3`.
Both binding families are configurable through `macro.record` and
`macro.execute`. Names are case-insensitive a–z.

The current vocabulary includes relative frame and beat motion, group start/end,
Visual selection begin/finish/clear, named-mark range selection, frame, beat or Visual cut, selected-beat or
Visual yank, typed operator motions and group objects, Repeat wraps (optionally
escalating) and count setters, named Group, neutral Ungroup, register paste or
Visual replacement, pauses (`InsertPause`, exact frames or milliseconds with a
host-resolved freeze), framing of the selected beat and invocation-relative
ranged zooms and creeps, explicitly scoped one-play freeze/black pauses, existing
Hold lengths and framing, built-in
[gag recipes](GAGS.md), gain trims and steps and saturation of the selected
beat (`SetAudio`, from `+`/`-`, `:gain` and `:saturate`), J- and L-cuts
(`SplitEdit`), role-only deletes (`DeleteRole`) and audio-only or video-only repeats (`RoleRepeat`, see
[role edits](ROLE_EDITS.md)), `Split` of the selected beat at the cursor (`s`,
`:split`; the right fragment becomes selected, and `.` splits again at the new
cursor), `Trim` of the selected Source with exact accepted In/Out/Slip/Roll
values (Apply in the Trim draft while recording; a replay that cannot apply
every value exactly refuses instead of clamping), `Slip` of the selected Source
by exact whole frames (Apply in the `:slip` preview while recording, with the
same refusal), Edit letter marks (`SetMark`, owned by the child at the cursor
exactly as `m{letter}` saves it) and jumps (`JumpMark`, recorded once the native
jump lands; crossing the active clock or Edit group cancels recording, and a
run refuses a jump outside that context), `PlaceSound` of the selected catalog sound
at the Edit cursor (`,s`, the same root event the native key places; it must
end inside the edit), `SetHoldAudio` with silence (`:hold-silence`) or the
room-tone sheet's exact Original sample span (its Apply while recording), local
recipe save and insert (a whole-group yank
and a register paste), and named Macro call. Word, sentence, pause and shot
motions and objects resolve against analyses projected through each staged
document.
Motions and cuts retain their requested counts, including when they
clamp at a group boundary. Copy, cut and paste retain the selected register name.
The planner resolves each instruction against the preceding staged edit in
its checked ordinary Sequence context, including the scope returned by `ig/ag`.
General temporal occurrence scopes, beat text objects, additional edits and broader
semantic dot-repeat remain required.
This is partial DP-06 implementation, not full macro acceptance.
See [Repeat selections](REPEAT_SELECTION.md) for total-play versus motion counts,
captured targets and structural range wrapping. `:repeat N` records a count setter
for an existing Repeat and a selected-beat wrap for another kind. Setters require
an explicit selected direct-child Repeat and no Visual selection on replay.
`:reverse`/`:ping-pong` record `InsertReverse`, `:tail` and the command `,t`
opens record `Tail`, `:cutaway` records `SetCutaway` and `:caption` records
`SetCaption`, over the selected beat or over the Edit range inside one beat
(the recorded Visual motions rebuild that range on replay); each resolves the
staged document when replayed ([reversed pauses and tails](REVERSE_AND_TAILS.md),
[captions](CAPTIONS.md)). `:retime`/`:wrap-retime` record `Retime { speed,
pitch, wrap }`, `:pitch` records `Pitch { semitones }` and `:hold-duration`
records `SetHoldDuration`; they author the same `WrapRetime`, `SetRetime` and
`SetHoldDuration` commands as the native edits, resolved against the staged
beat's own input ([speed editing](RETIME_EDITING.md)). `+`/`-` over an Edit range
records a ranged `SetAudio` step ([gain](AUDIO_GAIN.md#range-steps)); `,m`
records the explicit whole-beat mute state (`SetAudio { change: mute }`) or, over
a range, the role-only sound delete; `:audio-lag` records `SetAudioLag`;
`:edge` records `SetAudioEdges` ([audio edges](AUDIO_EDGES.md#native-edge-command));
and `:gag-set` records `SetGag` ([gags](GAGS.md#changing-parameters-after-insertion)).
Eligible tape-speed stages retaining exact pause clocks use their exact
selected child interval on every staged invocation, including named calls and
successive speed instructions. They preserve the existing pause structure and
refuse unsupported pitch processing atomically. An unchanged effective speed
still refuses without a revision or bank change. See
[speed changes after a pause](RETIME_EDITING.md#speed-changes-after-an-exact-pause).
[Named grouping](GROUP_EDITING.md) retains the exact name and selector. Ungroup
requires a selected neutral Sequence with no Visual selection. Both preserve the
copy bank and pending register choice, and use one atomic history entry per run.
`Explode` converts the selected direct-child Repeat and refuses every Visual
selection; `Duplicate { selector }` copies the selected beat or Visual range
after itself and selects the copy. Both leave the register bank unchanged
([explode and duplicate](EXPLODE_DUPLICATE.md)).
See [operator qualification](qualification/operator-motions-2026-10-02.md) for
typed selectors, pending input, exact capture provenance and receipt ownership.
See [Visual qualification](qualification/visual-macros-2026-10-02.md) for the
selection, replacement, persistence and native receipt checks.
See [copy/paste qualification](qualification/macro-reuse-2026-10-02.md) for
checks, rendered captures, retained failures and remaining limits.

## Recording and input ownership

### Named-mark ranges

`:select marks start=<exactMarkId> end=<exactMarkId>` selects a finished,
half-open Edit range in the active ordinary Sequence. It requires the Linked
role. Native letter marks use the IDs `native-mark-a` through `native-mark-z`;
the command takes exact IDs, not labels or guessed letters. For example:

```text
:select marks start=native-mark-a end=native-mark-b
:wrap-repeat 3
```

Recording stores `SelectMarkRange { start, end }`. Its targets are
`NamedMarkTarget` addresses, each containing a mark ID and an optional explicit
occurrence. The resulting `SemanticVisualSelection::MarkRange` retains those
addresses. It contains no cached frame endpoints. Native copy, cut, Group,
Duplicate and Repeat operations retain the effective
`SemanticSelector::MarkRange { start, end }`, so a named call or dot-repeat
resolves the marks again after preceding staged edits. A saved direct operator
can reuse that selector without an active Visual selection; a new explicit
Visual selection overrides it.

Both exact and quantized endpoints must remain ordered and inside the captured
scope. Missing, unresolved, ambiguous, wrong-clock, reversed or collapsed marks
refuse atomically. Repeated hosts require the existing explicit occurrence
rules. The planner never swaps, clamps or substitutes endpoints. This admission
does not add temporal occurrence scopes or duration-edit authority. Role-only
consumers retain their existing one-direct-child constraint; this does not
grant cross-owner role authority. Each consuming instruction keeps its own
existing range constraints.

The selection survives ordinary cursor movement and displays the current
resolved range with its exact IDs. If an edit makes it unresolved, consuming it
refuses; Escape clears it before a different selection is made. Plain Visual
selection replaces the named range. Command entry captures session, revision,
scope, cursor, selection and role, and refuses a stale completion.

Selecting alone writes neither authored history nor the register bank. Copy
changes only its register; authored runs keep their existing atomic Undo and
bank behavior. `:wrap-repeat` wraps the range; `:repeat` remains the existing
selected-beat/count command and refuses an active Visual range.
See the [named-mark qualification](qualification/named-mark-selectors-2026-10-22.md)
for the current verification state and retained failures.

### Explicit authored nodes

`:select node=<exactNodeId>` selects one whole authored node beneath the
current ordinary Edit Sequence. The command requires the Linked role and
uses the exact ID, independently of the cursor, selected beat or a time range.
The status line shows its label, ID, scope and current range. An empty Sequence
remains a distinct structural selection even when another empty node occupies
the same time.

Recording stores `SelectNode { node }`; the finished Visual selection and
consuming copy, cut, Group, Duplicate and Repeat retain `Node { node }` intent.
Named calls and dot-repeat resolve the ID against each staged revision.
Renaming or moving it within the captured scope preserves identity. Deleting
it or moving it outside the scope refuses without choosing another beat.
Plain Visual selection overrides the saved selector through the existing rule.

Capture and mutation use the node's actual immediate Sequence parent. Ordinary
navigation retains the invocation's scope, including when the target is nested
several groups deep. Paths through Repeat, Retime or partition owners refuse;
selecting a whole composite child does not grant access to its interior. The
root, the current scope itself, missing IDs and historical audio-only owners
are unavailable. Original, occurrence and duration-only contexts cannot use
this selection.

Empty nodes can be copied and cut by identity. Repeat still needs positive
duration. Native `p`/`P` replaces the whole selected child through the semantic
replacement path; `:splice` currently refuses this selection with guidance to
use `p`/`P`. Role-only consumers retain their range-only contract and refuse a
whole-child Node selection. Selecting alone changes neither history nor
registers. Command entry captures the existing session, revision, scope,
cursor, selection and role guards.

### Discontiguous whole-node sets

`:select nodes=["id-a","id-b"]` selects 1 through 64 distinct whole authored
nodes in ordinary Your edit with Linked role. The suffix is a strict JSON
array of exact IDs. Members may have different immediate Sequence parents;
root, scope, missing, outside-scope, duplicate and ancestor-overlapping members
refuse. Repeat, Retime and partition interiors retain the single-Node limit.
Structural order determines member order, including equal-time empty siblings.

Recording saves `SelectNodes { nodes }`; copy, cut, named calls and dot retain
`Nodes { nodes }` addresses and resolve them on each staged revision. A finished
set survives cursor motion. Plain Visual selection replaces it. The inspector
lists each member's identity, actual parent and range; the footer shows count
and summed duration. Cards highlight individual ranges and empty identities.
There is no gap-spanning range for this selection.
The inspector is read-only while a set is selected. Loop selection refuses the
set instead of playing its navigation beat or unselected gaps; clear the set
before choosing a playback range. AI generation and saved framing also refuse
set authority, including a previously captured single-Hold AI target. Canceling
an existing AI job remains available.

Yank stores one `EditedSet` captured from one entry document. Cut first captures
that same union, then deletes members in reverse structural order through one
atomic Compound. Empty-only cuts change structure with zero duration delta.
Undo restores the complete edit while retaining the union register. Captures,
historical checkpoints, redo and reopening preserve every component and one
logical copied mark when its Source-clock bindings span several components.
Relationships that cannot close within the supported capsules refuse explicitly.

Clear the set selection before choosing a paste seam. `p`/`P` inserts the union
under one editable neutral Sequence, omitting the unselected gaps and allocating
shared marks once for the union. Active-set replacement, Place slice, Group,
Duplicate, Repeat, role-only and parameter consumers currently refuse sets.
Whole-node sets retain structural identities. For temporal intervals, use the
separate range selection below. Transcript-search-result selectors remain open.

### Discontiguous interval sets

`:select ranges=[{"start":2,"end":6},{"start":10,"end":14}]` selects
half-open ranges in project frames relative to the captured Edit cursor.
Negative offsets reach before that cursor. Each of the 1 through 64 supplied
pairs must have positive duration and fit completely inside the current
ordinary Sequence in Your edit with Linked role. Invalid members refuse the
complete request. Checked arithmetic rejects overflow; endpoints never clamp.
Duplicates, overlaps and touching inputs merge into a canonical maximal union.
Partial endpoints retain the ordinary Source, Hold and supported-fragment
contract. Enter a nested group before selecting its interior; a whole composite
child can be included at its existing seams.

Recording saves the original signed offsets in `SelectRanges`. It resolves
all pairs from one entry cursor, then moves to the final canonical end. The
finished selection retains its absolute geometry through later cursor motion.
The inspector lists every interval, and the footer shows count and summed
duration. There is no single enclosing range, even for a union that normalizes
to one interval. Plain Visual selection replaces the set.

Yank captures every component from one immutable entry snapshot. Cut captures
once, then performs the exact partial deletions from right to left in one
atomic transaction. Paste at an ordinary seam concatenates only the selected
intervals under an editable group. Multiple windows of one Source retain its
complete recipe independently; shared logical marks preserve every renamed
physical binding. Historical capture validation reconstructs the complete set
against its retained source revision.

Native `y` and `d` use the finished Visual selection. Repeating that Cut with
dot requires a new selection, as for ordinary Visual cuts. A typed
`Ranges` selector instead retains its original offsets and resolves them anew
at invocation. Active-set replacement, role-only editing, scalar parameter
operations, Group, Duplicate, Repeat, Place slice, AI, saved framing and loop
selection refuse range sets. Cursor playback retains its ordinary behavior.

### Transcript-search sets

`:select search="hello world"` selects complete corrected transcript phrase
occurrences in the current ordinary Sequence of Your edit with Linked role.
The query is one strict JSON string, including spaces or escaped quotes.
Matching uses the existing lowercase/punctuation normalization and treats the
last word as a prefix. Empty normalized queries and queries beyond 32 terms
or 4,096 bytes refuse.

`SelectTranscriptSearch { query }` retains that raw query. Its finished Visual
selection carries the query and separate canonical ranges. Native `y` and `d`
retain the semantic search selector, so dot and named calls search their staged
document afresh. A newly explicit Visual selection can override a repeated
cut. No previous absolute hit timestamps become a macro's target.

Each occurrence must retain its entire forward Original interval, including
pauses between matching words. Continuous split Source seams and positive
slope changes are supported. Deleted words or pauses, jumps, reverse mappings,
Holds, Repeat gaps and play wraps break phrase coverage. Cutaways do not change
the provider speech clock. Exact inverse mappings are enclosed in whole Edit
frames, then overlaps and touching enclosures merge. No partial phrase or hull
is returned. Source support comes from the authored selection and the measured
picture interval; held endpoint slack cannot manufacture a hit.

Yank and Cut use the same immutable union capture, atomic descending deletion
and durable seam paste as explicit intervals. Scalar consumers and unsupported
temporal interiors retain their existing refusals. Search is bounded to 4,096
raw matches, 512 provider spans, one aggregate projection work budget and 64
final components. Exceeding a bound refuses the whole operation.

The host freezes one exact proposal and correction snapshot for the invocation.
Publication validates it inside the same Store transaction for authored edits
and register-only copies. Selection-only success validates the snapshot with
the head and bank in a read transaction. Native command entry and completion
also retain the displayed analysis identity, including absence. Guards do not
enter history or copied content. See the
[qualification record](qualification/transcript-search-selectors-2026-10-22.md)
for verification status and limits. This proves logical Original speech carried
by Source leaves, not audible PCM after independent audio treatments.

### Ranged zoom and creep

On an ordinary selected direct child, `:zoom` and `:creep` over an Edit
selection record `Zoom { kind, center }`. The instruction retains the scale,
optional starting scale, curve and either `Keep` or `OriginalCenter` intent.
It never stores the recording's range fractions, selected owner, displayed
pose or sampled target point. Each invocation resolves the selected owner and
its exact duration from the preceding staged document, then converts the
invocation's selection to that owner's local frames. Time selections retain
their oriented state; Object selections resolve their exact owned range.
An empty or cross-owner range refuses. A range spanning the complete owner
uses the existing whole-owner behavior.

No target keeps the retained framing center. Explicit `target=center` projects
the Original picture's center through the framing entering the selected owner;
it refuses if that point is clipped. Recording a ranged command that would
sample a selected, current, named or face target refuses with an explanation.
An implicit selected-target choice is treated as Keep only when no saved target
or target follow is involved. Ordinary interactive target sampling remains
available. Existing ordinary whole-beat presets keep their `SetFraming` behavior.

Native commands and semantic replay share the pure `framing_zoom` builder.
A ranged smash restores the preceding static pose after the range; a creep
holds its arrival pose. Existing dynamic paths and follows refuse ranged
replacement when their outside behavior cannot be preserved. The shared host
resolver uses the staged render plan's evaluated pose and clock, qualified
provider metadata and composed picture geometry. It opens no source media,
creates no decoder or picture raster and reads no proxy. The invocation cursor
must show the selected owner, as the native framing command requires.

A successful ranged Apply establishes the same Zoom intent for `.`. Repeating
it resolves the new invocation selection; recording `.` stores that instruction.
Named runs retain ordinary Compound atomicity and one Undo, including when an
earlier instruction changes the owner's duration. An unchanged framing adds no
authored step. One-play framing uses the separate invocation authority below;
an ordinary macro cannot infer permission to edit one Repeat play.

The pending stopped-picture request retains the full macro capture, including
session, revision, bank version, ordinary scope, cursor, selected child, Visual
state and pane. Context changes cannot supply a new target to a delayed reply.
While recording, another action waits for that request; Escape or
`:record-cancel` cancels it and the recording. Cancellation before submission
cannot later commit or append the pending framing. Already queued authored
work retains the ordinary durable receipt rules.
See [ranged framing macro qualification](qualification/ranged-framing-macros-2026-10-18.md).

### One-play zoom and creep

Choose This play at every enclosing Repeat before recording or running a
framing macro. `:zoom`, `:creep`, `,z` and `,c` use `Zoom` intent with Keep or
explicit OriginalCenter. A successful single Apply also supplies that intent
for dot. Choose another play before `@a` or `.` to frame that invocation's play.
The saved program contains no node, play, presentation or recording-time pose.

The invocation captures the authoring target and its separate concrete
presentation, plus the ordinary Sequence context. Each staged instruction
resolves that exact plan layer and its intrinsic owner clock, isolates only
changed selected branches, and maps both addresses for the next instruction.
Existing overrides need no additional isolation; unchanged framing creates no
revision. One run commits once, and one Undo restores framing and isolation.
The native receipt retains the mapped inspector. Any scope navigation revokes
its cursor ownership permanently, including moving away and back in one event
batch before a delayed receipt arrives.

Zoom, whole-owner SetAudio Step, freeze/black InsertPause, SetHoldDuration,
SetHoldAudio, SetRoomTone and named Call instructions are
admitted under one-play authority,
including every expanded nested call. An unsupported later instruction refuses
the complete run without history or bank writes. All plays, mixed choices,
several plays, absent or mismatched presentations, Original invocation and
Visual ranges cannot supply this authority. Scope navigation is not recorded;
save or cancel before another editing action. Sampled, named and face-target
framing refuses recording before analysis or authored work. Their ordinary
interactive paths remain available. A Retime scope without a Repeat ancestor
retains its existing direct framing path.

Headless Run and Apply accept optional `scoped` invocation context with `root`,
`target` (`ScopedNodeTarget`) and `presentation` (`InstancePath`). Dry-run and
commit validate the same address. Output context and each trace row retain the
mapped authority. Request bytes have a depth/identity-derived allowance while
saved programs retain their independent 128 KiB limit.
See [one-play qualification](qualification/one-play-framing-macros-2026-10-18.md)
for real-media, durable-history, native replay, review and timing evidence.

### One-play pause intent

With This play explicit at every enclosing Repeat, `,h` and freeze/black
`:hold` commands use the same `InsertPause { length, black }` intent as ordinary
editing. A successful Apply supplies dot-repeat and enters an active recording
only after its durable receipt. The saved program contains no owner, play,
picture, PTS or invocation boundary. Run and dot resolve the newly chosen
play's exact current boundary against the staged document.

Each insertion retains the actual new Hold's authoring address and concrete
presentation for the next instruction. A following pause, Zoom or named call
therefore acts on that Hold, including after isolation or splitting. The
ordinary cursor and visible enclosing selection stay at the insertion boundary.
Freeze capture uses the qualified Original picture and captured framing; black
samples no picture. Both insert silent time and preserve subsequent source
clocks. Unsupported provider content refuses before allocation.
Zoom still requires qualified picture geometry. A black pause has no such
picture; a later Zoom on it refuses the complete run without writes.

Mixed Default/Play choices and selected-play batches keep their direct editing
path but cannot supply macro or dot authority. AI pauses, reverse/tail intent,
Retime ancestors and Visual ranges remain unsupported on this semantic path.
An unsupported later instruction rolls back the entire run. Navigation during
a non-recording queued scoped action may continue; it revokes the delayed
inspector continuation without retargeting or discarding the saved edit.
Recording still waits for each action, and scope navigation cancels it.

### One-play Hold length

With This play explicit at every Repeat level, `:hold-duration Nf` records
`SetHoldDuration { length }` and supplies dot intent when the target path uses
only Repeat and Sequence ancestry. Enter on the selected Hold or its Change
duration control opens the same captured command, prefilled with the current
length. Invocation captures that Hold and concrete play; the saved instruction
contains only length intent.

The planner executes `EditOccurrence::SetHoldDuration` with exact node and mark
pools, then carries its actual isolation mapping forward. The final cursor is
the resized Hold's exact start, while the ordinary enclosing selection remains
unchanged. Another duration, Zoom, pause or named call uses that mapped Hold.
No-op lengths and late unsupported instructions leave history and registers
unchanged. Multiple plays and attached or root sound events retain explicit
refusals. Retime ancestry remains refused except for the explicit-body
fixed-Retime duration path below. Generated-picture shortening retains
qualified accepted media; extension beyond its coverage commits fallback and
durable preparation rather than stretching the accepted provider silently.
See the [Hold duration qualification](qualification/scoped-hold-duration-2026-10-20.md).

### This-play Hold length inside fixed Retime

Explicit This play bodies support `SetHoldDuration` through exactly one
fixed, unclocked Edit Retime below the innermost Repeat. The target path may
otherwise contain Sequence nodes; any other Retime, Partition or owned gap
refuses. The runtime context retains every explicit play
and concrete Hold, while saved programs still carry only the requested length.

Each setter isolates the selected branch when needed and preserves the fixed
Retime's source crop and output extent. It composes the Hold's visible interval
through that crop and scale, then continues at the first sampled visible Hold
picture. That frame may differ from the Hold's intrinsic start when the crop
cuts it away. The target must remain visible in at least one sampled project
picture after every staged setter. Hidden or unsampled Holds, derived pause
clocks, root or attached sound events and selected-multiple-play
contexts refuse atomically. Mixed choices use the separate authority below.
Explicit Repeat ancestry follows the existing
document depth bound. Already-owned branches can be edited without another
isolation copy; partly owned paths copy only the remaining shared branches.
Recording, named duration calls and dot use the same invocation authority.
See the [nested This play fixed-Retime qualification](qualification/nested-fixed-retime-hold-duration-2026-10-22.md)
for retained scope and verification evidence.

### All plays Hold length

With All plays explicit at every Repeat level, Enter, Change duration and
`:hold-duration` capture the literal shared Hold plus its optional concrete
presentation. Saved `SetHoldDuration { length }` intent contains neither address.
The duration-only runtime authority accepts length setters and named calls;
it grants no framing, audio-policy or ordinary navigation authority.

Each staged setter revalidates the shared target, then executes ordinary
`SetHoldDuration` with a parameter revision and no cloned node or mark IDs.
Independent play overrides remain intact, while existing physical audio clocks
use the reducer's old-origin capture. A visible continuation follows the same
stable representative to its new start. No presentation is accepted only when
validated body overrides cover every play of at least one enclosing Repeat.
That dormant change preserves the cursor and rendered duration. Gap overrides
alone do not establish dormancy.

Fixed unclocked Edit Retimes below the direct Repeat inspector root retain their
input windows and output lengths. Exact visibility composes each crop and scale
with current stable Repeat offsets, then selects project-picture centers. A
live Hold needs at least one sampled visible picture after every staged setter;
a later disappearance rolls back the complete run. A crop cannot establish
dormancy. See [fixed-Retime shared duration qualification](qualification/shared-fixed-retime-hold-duration-2026-10-22.md).

No-op lengths, forged or stale captures, mixed or several-play authority,
Partition or derived-clock ancestry and unsupported later instructions refuse atomically. Direct
Apply supplies dot intent; named Run and Undo retain their existing dot policy.
Scope navigation revokes delayed inspector continuation even after away/back
navigation. This path does not broaden one-play semantic framing.

### Mixed All plays and This play Hold length

A single Hold under mixed Default/Play Repeat bodies uses
`SemanticMixedHoldDurationContext`, retaining the inspected root, complete
authoring target and optional presentation. It admits duration setters and
named calls only. Saved `SetHoldDuration { length }` intent has no captured
node, play or presentation identities.

The planner requests exact temporal isolation resources, including empty pools
for already owned branches, and executes `SetScopedHoldDuration`. Only explicit
Play branches that still use a default are cloned. The mapped Default branches
remain shared; the established ordinary duration reducer then captures physical
old-origin clocks and resizes the mapped Hold once. Actual execution proofs
carry both addresses through staged calls and the final native receipt.

A live continuation follows the same stable play to its mapped Hold's new
start. Positively proven dormant definitions have no invented presentation:
isolation preserves their authored Play meaning, while cursor and output stay
unchanged. Direct Apply supplies dot intent; named Run and Undo retain their
existing dot policies. Navigation permanently revokes delayed continuation.

Explicit owned-gap subtrees follow the exact preceding stable Play at any
enclosing level. A final owned gap remains a dormant definition; it does not
invent a project occurrence or move the retained cursor. Default choices still
follow literal bodies only.

Mixed body targets can cross one fixed unclocked Edit Retime strictly below
every enclosing Repeat, with Sequence ancestry otherwise. The fixed crop and
output extent survive each setter. Visibility composes the crop and scale with
the captured stable presentation, and continuation uses its first sampled
visible Hold picture rather than its potentially cropped intrinsic start.
The raw command independently checks a live use of the literal definition
before and after resizing; this geometry check never replaces Default authority
with a Play target. An unused definition retains no presentation, and a live
Hold disappearing from the sampled output refuses the entire staged run.

Several targets use the selected-duration authority below. Retime-owned gaps,
additional Retimes, Retimes above or between Repeats, Partition, derived clocks,
root or attached sounds and unrelated instructions remain unavailable. Other duration families
keep their existing paths and guards. See the
[mixed duration qualification](qualification/mixed-hold-duration-semantics-2026-10-21.md)
and [fixed mixed duration qualification](qualification/mixed-fixed-retime-hold-duration-2026-10-22.md)
for implementation and verification status.

### Selected plays Hold length

`SemanticSelectedHoldDurationContext` captures the inspected root, primary
Hold, optional primary presentation and ordered further targets. It admits
2..=1024 targets with distinct explicit Plays at one common Repeat axis and an
identical earlier authoring prefix. Every target must follow ordinary Sequence
and Repeat bodies or explicit owned gaps. Bodies can also cross one fixed,
unclocked Edit Retime strictly below every Repeat. A shared node ID may represent several distinct selected
families and is never used to deduplicate them.

Saved programs still contain only `SetHoldDuration { length }` and named calls.
The planner validates every member, including no-op members, then requests one
fresh revision and exact isolation pool per changed target. Mixed addresses
use `SetScopedHoldDuration`; explicitly all-Play addresses use the existing
occurrence duration path. Neither route reconstructs mixed authoring from a
representative. Every actual leaf proof maps all targets and the primary
presentation before resolving the next member.

The fixed-window body path validates sampled visibility for every member before
allocation, including no-op counterparts. It preserves crop, pitch and output
extent, and resolves final primary continuation from the cropped visible
interval. An intrinsic start outside the crop is not a continuation frame.
See the [selected fixed-window qualification](qualification/selected-fixed-retime-hold-duration-2026-10-22.md).

The final Compound retains aggregate limits and commits once. The primary
cursor is resolved after all leaves, including when a later earlier-in-time
resize moves it again. A zero root-duration delta remains an edit when the
recipes change. No-op members skip isolation; an all-no-op request refuses
without history. Dormant primary definitions require positive liveness proof
from complete body-override coverage or the owned gap's current final position.
They preserve their cursor without inventing a presentation, even when another
selected live gap changes output duration. Stable iteration identity alone does
not determine dormancy after reordering.

Native Apply, recording, named calls and direct-edit dot use this invocation
authority. The receipt independently checks the complete executed proof and
returns the mapped counterpart set. Navigation permanently revokes delayed
continuation and clears the mapped multi-target selection. Choose a new play
set explicitly before applying dot to another pair. An owned gap follows its
preceding stable Play, including a final dormant gap selected alongside a live
gap. Implicit default gaps have no editable node target. A single all-Play
dormant gap still has no live-boundary native action. Generic framing,
other Retime ancestry, Partition, placed sounds and unrelated instructions retain
their guards.

### Ordinary Hold length inside fixed Retime

An inspected Hold without Repeat ancestry also supports `:hold-duration`, Enter,
Change duration, recording, named calls and dot inside fixed Edit Retime windows.
The captured runtime context grants duration edits and named calls only. Saved
programs retain `SetHoldDuration { length }`, without node or presentation IDs.
Other recording actions refuse before opening a draft or changing state.

Every staged setter revalidates the selected Hold and its exact visibility
through all enclosing crops. The continuation is its first sampled visible
project frame, including when the Hold's intrinsic start lies before a crop.
Input and output half-frame ties follow the half-open picture-center rule.
No visible sampled picture, exact derived pause clocks, Repeat ancestry, Visual
selection, no-op lengths and stale captures refuse atomically. The fixed outer
window and independently supported output sounds retain their clocks. This does
not grant general semantic authority to Retime contents. See the
[native fixed-window qualification](qualification/native-fixed-retime-hold-2026-10-21.md).

### One-play Hold sound

With This play explicit at every enclosing Repeat, `:hold-silence` and the
RoomTone sheet's Apply record `SetHoldAudio`. A prepared RoomTone instruction
keeps the exact approved Original sample span. Preparing or cancelling the
sheet records nothing. Headless `SetRoomTone { register }` instead resolves
the current Original moment in that register against each staged document.
Both forms use the same source qualification at preview and commit.

An invocation isolates only the selected branches and retains the mapped Hold
and concrete presentation for later instructions or named calls. Its absolute
Edit cursor, ordinary enclosing selection, picture provider and duration stay
unchanged. A successful direct one-play edit also supplies dot; choose another
explicit play to apply the same sound intent there. Existing All/mixed and
ordinary nonrecording controls keep their direct workflow.

These are value changes, so they retain the existing Retime and sound-event
admission instead of the Hold-duration path's timing restrictions. Choosing
RoomTone retires only the selected Hold's obsolete silent-issuer allowance;
returning to Silence does not grant it again. No-op policies allocate no
isolation. Unsupported authority, stale targets and late failures refuse the
whole run without history or register writes. A delayed saved result cannot
reclaim newer navigation or append to an abandoned recording.
See the [one-play Hold sound qualification](qualification/scoped-hold-audio-2026-10-20.md).

### One-play gain steps

With This play explicit at every enclosing Repeat and no Visual range, `+`,
`-` and relative `:gain` steps record `SetAudio { change: Step { millidecibels } }`.
A successful Apply also supplies dot. The instruction retains only the relative
amount; each invocation applies it to the newly selected play's current gain.
Counted runs and named calls use each preceding instruction's staged result.

The planner uses the existing scoped audio-treatment edit and exact isolation
pools, then carries the actual mapped owner and presentation forward. Gain steps
preserve other treatments, picture timing, the absolute Edit cursor and ordinary
enclosing selection. One Undo restores the whole run, including isolation.
Zero steps allocate nothing; out-of-range gain, stale authority, mismatched
captures or a later failing instruction leave document, history and registers
unchanged. All/mixed/several-play nonrecording gain retains its direct workflow.
Visual ranges cannot supply one-play gain macro authority. Explicit mute and
literal audio setters have the same authority requirements, described below.
See the [one-play gain qualification](qualification/scoped-gain-macros-2026-10-20.md).

### One-play mute state

With This play explicit at every enclosing Repeat and no Visual range, the mute
key and `:gain-mute` capture the inverse of the selected owner's current mute
state as `SetAudio { change: Mute { muted } }`. Recording retains that Boolean.
Saved calls and dot apply the retained state to the newly chosen play; they do
not toggle it. Gain, envelopes, mute ranges, saturation and stage order stay
intact, as do picture timing and both editor cursors.

The planner uses the same exact isolation and mapped continuation as gain steps.
Counted runs and nested calls use the staged target. Applying an already chosen
state refuses before allocating another isolation; a late refusal rolls back the
whole run. Native same-state preflight adds no instruction or history. Stale or
mixed authority, Visual selection and mismatched captures cannot fall back to a
shared edit. Ordinary nonrecording All/several-play controls retain their direct
workflow. See the [one-play mute qualification](qualification/scoped-mute-macros-2026-10-21.md).

### One-play literal audio setters

Explicit This play at every enclosing Repeat and no Visual range also admit
whole-owner absolute `:gain`, `:saturate`, `:balance` and typed `:eq` commands.
Their existing `SetAudio` instructions retain the complete literal value or
stage removal. Saved calls and dot apply that intent to the newly selected
owner; absolute trim never becomes a relative step. Other treatments, authored
order, gain automation, mute ranges and sends stay intact.

Each instruction reads the preceding staged result and uses exact scoped
isolation. The actual mapped owner and presentation continue through calls and
later pause/framing edits. One Undo restores the entire run. A setter that
already matches refuses before allocating; a later refusal rolls back the whole
run. Native preflight appends no same-state instruction. All/mixed/several-play
nonrecording controls keep their direct workflow.

Visual-range steps, interactive Gain/EQ drafts and wet/tail sends do not gain
scoped semantic admission here. Stale or mismatched captures, incomplete Play
authority and delayed receipts cannot fall back to a shared edit or reclaim
newer navigation. See the
[literal-setter qualification](qualification/scoped-audio-setters-2026-10-21.md).

### Place slice Apply

While recording in Your edit, `:splice` keeps its unsaved placement preview.
Apply records the existing `Paste` or `ReplaceSelection` instruction only when
that instruction resolves to the displayed placement. Copy must use the entry
register's exact source, including its endpoints. An insertion must use the
literal child slot immediately before or after the captured selected beat;
slot zero also works in an empty Sequence. Replacement must use the captured
nonempty time range, exact Object selection or named-mark range resolved to
that exact captured interval. Empty structural Objects retain their identity
and can be replaced.

Move, refined source endpoints, interior or nonadjacent insertion, and insertion
with an Object selection refuse recording with a visible explanation. The draft,
registers and saved edit stay intact, so restoring an eligible proposal permits
Apply. Escape cancels the placement and restores its entry context.

Successful recorded Apply abandons the preview and submits one semantic
transaction. The instruction enters the recording only after its successful
receipt. It stores the register and before/after or replacement intent; replay
resolves the current selected child or selection instead of retaining the old
preview destination. The committed placement has fresh identities, with the
same picture and canonical PCM semantics as the preview. The footer explains
the current recording restriction or the one-action Apply behavior.
See [Place slice macro qualification](qualification/place-slice-macros-2026-10-18.md).

### Original frame navigation and marks

Recording and execution use the active invocation clock. On Original, the
supported instructions are relative frame motion, start/end motion, letter
mark saves and jumps, and named calls containing those instructions. The same
saved intent resolves against Original when invoked there and against Your
edit when invoked there. A program never stores the recording's cursor or PTS.

The host admits the Original's committed qualification and measured picture
index. Navigation uses presentation ordinals, including the terminal video
boundary. Mark saves store the exact measured video PTS, or terminal video end,
as a root-owned Source anchor. Jumps require a bound mark on that same Original
and an exact measured boundary. Wrong-clock, unresolved, ambiguous or conflicting
named marks refuse atomically. The native recorded jump retains the ordinary
Ctrl O/Ctrl I trail.

Original actions preserve the separate Edit cursor, ordinary Sequence scope,
selected beat and oriented Visual selection. An active Original moment selection
refuses recording or execution; other Original instructions and retained Edit
occurrence scopes also refuse explicitly. Escape cancels Original recording
without clearing the inactive Edit range. The recording hint names Original.

Mark-only commits publish separate revision proofs so retained Edit selection,
jump metadata and the accepted picture survive consecutive native or headless
saves, including coalesced updates. These proofs never authorize a remote
cursor continuation. A delayed native receipt cannot reclaim navigation after
the captured Original cursor, source, qualification or selection changed.
See [Original mark qualification](qualification/original-mark-macros-2026-10-18.md).

### Whole-Original reuse

Whole-Original reuse (`InsertOriginal`, `,i` or `:insert`) inserts the complete
measured A/V union after the selected direct child, or appends to the current
ordinary Sequence when no child is selected. Visual selection never turns this
action into replacement. Each invocation receives fresh identities, selects its
inserted beat and leaves all copy registers unchanged. The host verifies the
project's pinned Original and qualification; this instruction refuses legacy
multi-video projects. Legacy interactive insertion keeps its separate path.
The full source uses its measured audio lead/tail and picture endpoint holds,
not the picture-ordinal bounds of an Original moment register.
Like other semantic transactions, reuse requires the project's register-bank
version to be loaded so it can prove that the unchanged bank is still current.
See [whole-Original qualification](qualification/whole-original-macros-2026-10-16.md).

Recording performs normal edits. A motion enters the body when dispatched;
a copy, cut, paste or call enters only after its matching successful receipt. A pending
operation blocks the next recorded action until that result arrives. Failed
operations do not enter the body. Saving writes a named Macro without adding
an Undo entry or replacing the default copy. Cancelling leaves completed
edits undoable and preserves the previously saved Macro.

While recording, `v` begins or finishes a Visual selection. `h`/`l`, `j`/`k`
and `gg`/`G` retain their frame, beat and group-boundary intent. `y` copies a
nonempty Visual range and finishes its extension while retaining both endpoints;
`d` cuts it, and `p`/`P` replaces it. Cuts and replacements clear the selection.
Without Visual selection, `yy` copies the selected beat, `dd` cuts it, and
`p`/`P` paste after/before it. `y` or `d` followed by `h`/`l`, `j`/`k`, `gg`/`G`
copies or cuts the exact half-open range from the entry cursor to that motion's
destination. A yank leaves the cursor and selected child in place; a cut selects
the join. Use one positive distance count before the operator or its frame/beat
motion, such as `5dl` or `d5l`. Supplying both counts refuses. Whole-beat operators
accept only no count or one; group-boundary motions do not accept counts.
An empty motion interval refuses without falling back to a whole beat.
These actions use the same staged planner and receipts as replay. Both measured
Original ranges and edited slices can be pasted. An absent selection is retained
as absent; it never silently becomes the beat under the cursor. An empty group
may be copied, and pasted copies receive fresh identities and become selected
even when they add no picture time. An empty destination Sequence admits slot
zero without a selected child.

Recording and execution may start with an existing Visual selection. Such a
program can depend on the invocation range, or begin a new range relative to
the invocation cursor. Empty, absent, forward, backward, extending and finished
selections remain distinct. An empty range refuses copy, cut or replacement;
it never falls back to the selected beat. `:record-cancel` always discards the
draft. Escape during pending work cancels the draft and relinquishes cursor
ownership; already queued authored work still finishes.

The footer shows the register, instruction count and save/cancel controls.
Unsupported actions refuse explicitly. Changing the project, register bank,
ordinary group, pane or cursor outside the supported recording path cancels
the draft. A refresh failure after durable success retains that success and
stops recording with reopening guidance.

Prefix and command entry capture the project session, revision, bank version,
group, cursor, selected child and oriented Visual selection, including an absent
eligible target. A late letter or command
cannot replace that capture with a newly available context. Text, IME,
focused native controls and Kestrel reservations retain priority. Since egui
has no `Key::At`, logical `@` requires the pressed key's immediate native
`Text("@")` companion. Physical mode explicitly uses Shift+2. Physical input
across real keyboard layouts still requires qualification.

## Planning and atomic execution

`SemanticProgram` contains typed instructions, never raw keys, process hooks,
stored cursor timestamps or preselected node identities. `plan_semantic`
incrementally executes its body against a private document and staged register
bank. `SemanticRegisterBank` keeps the entry map and its observed version
together at that boundary. The planner applies each ordinary leaf once and
returns a resolved `Compound`,
final context, register writes and an entry trace. Navigation/selection-only programs return
no authored request and add no history. Headless Macro runs also return sorted
`mark_changes`: full `before`/`after` logical mark records that differ between the
entry document and final plan. Fragments and unresolved reasons are retained;
intermediate instruction changes do not masquerade as final losses. Dry-run and
commit share this comparison.

`SemanticInstruction::Yank` and `Cut` carry a `SemanticSelector`: explicit
selected beat, explicit Visual selection, or `SemanticMotion` (frames, beats,
group boundary). Motion resolution is shared with standalone semantic movement.
The trace retains the exact `SliceCaptureSelection` and historical revision for
media admission. A selected empty child stays a structural capture, and its cut
uses `DeleteRipple` with no split IDs. Range cuts use one `DeleteRange` with its
exact allocation requirements. A child cut chooses its literal following sibling
even when adjacent empty children share the same time coordinate.

Native operator prefixes capture session, revision, bank version, ordinary scope,
cursor, selected child, Visual state, pane and chosen register. A changed context
invalidates the capture permanently, including after returning to the same
position. Single operations and recorded operations share `Operation::Apply` and
the same saved-receipt handling. Original range `y` stays immediate and immutable.

Context tracks the selected direct child and Visual endpoints separately from
the absolute cursor. An extending selection's head must equal the cursor;
a finished selection remains independent of later navigation. Both endpoints
must lie within the ordinary Sequence's absolute bounds.
Frame motion selects the right-hand child, or the final child at the scope end,
as native navigation does. Yank preserves both coordinates. Paste selects its
new root and puts the cursor at the insertion boundary. Each later instruction
uses that staged selection. Visual replacement uses one ordinary `ReplaceSlice`
or `ReplaceSource` leaf, with exact disjoint split and import identity pools.
Its trace retains both the removed and inserted intervals. Original source mappings come from the saved
qualification's measured index and are checked again at store admission.

A call resolves a named Macro at call entry and freezes that body for all of
its counted repetitions. Later calls see earlier staged register writes.
Overwriting a running Macro's register with a cut does not change its active
body. Calling a copy or pasting a Macro is a type error. Recursive calls fail;
call depth is at most 16, a body at most 1024 instructions and 128 KiB, and an
expanded run at most 4096 instructions. Calls, motions and clamped no-ops all
consume fuel. Existing Compound step, document and capture limits also apply.

The service binds the actual saved head and bank version. The store prepares
the complete Compound with ordinary media admission inside its Immediate
transaction, then invokes fallible host-only runtime-bank preparation before
any write. It returns that private bank only after the same prepared Compound
and bank durably commit. Preparation cannot publish state or perform I/O.
Head, bank, allocation, capture and consumed-analysis checks remain in the
transaction; an independent dry-run still previews without writing. Every
leaf and capture allocation is fresh. A run with edits
saves one reversible history entry and one final bank result. A yank-only run
saves its copies without changing the document, Undo or Redo. A later failure
saves neither. Undo/Redo replays frozen resolved instructions and never rereads a
mutable Macro body. Register contents survive Undo.

## Persistence and feedback

Database schema 55 stores `RegisterValue::Macro` with neither capture revision
nor capture-step provenance. Original and Edited copies require exactly one
such reference. Macro-only banks may omit the default copy. Payload type,
canonical bytes, size and digest are validated on reopen and checkpoint.
Unused schema-54 packages require recreation under the development policy;
existing older supported adapters remain.

Each native request carries a session-scoped identity. Exact retries of a
successful request return its retained receipt; changed metadata cannot reuse
that identity. The last successful receipt remains independent of later failed
feedback. A committed macro remains successful if workspace refresh fails,
while the old view cannot consume its cursor. A cursor moved while execution
was pending is not retargeted by the delayed result.

Native and headless execution use the shared core planner and store Compound
entrypoint. An authored named Run clears the current dot-repeat candidate.
A supported direct Apply cut, Repeat wrap or count setter establishes its effective selector and
parameters after commit, including during recording. Recording `.` stores that
effective instruction; later playback does not consult session repeat state. Bank-only
operations preserve the candidate. See [semantic repeat](SEMANTIC_REPEAT.md).

Original mark-only runs and Applies also preserve the candidate, as native
standalone mark saves do.

## Headless inspection, save and run

The standalone CLI and `deadpan-app --headless` share these commands:

```text
macro inspect <project.deadpan> [--register a]
macro <project.deadpan> --json <request.json> [--dry-run]
```

Inspection reads the project revision and bank version in one SQLite snapshot.
It reports register types and saved Macro bodies without returning copied media
payloads. JSON register names use lowercase a-z. Request files are limited to
132 KiB, including the existing 128 KiB program bound. Use the inspected
identities in a version-1 request:

```json
{
  "protocol": 1,
  "project_id": "PROJECT_ID_FROM_INSPECTION",
  "expected_revision": "REVISION_ID_FROM_INSPECTION",
  "expected_bank_version": 0,
  "dry_run": false,
  "operation": {
    "type": "save",
    "register": "a",
    "program": {
      "instructions": [
        { "type": "move_frames", "forward": true, "count": 2 },
        { "type": "cut_frames", "operation": { "count": 1 }, "register": "b" }
      ]
    }
  }
}
```

To run, replace `operation` with the following object and update the expected
bank version after saving. `parent` is an ordinary Sequence node ID; `cursor`
is an absolute Edit-frame boundary within it. A positive count repeats the
whole Macro in one transaction. The optional `new_revision` names a fresh outer
revision; omission allocates it on the host.

```json
{
  "type": "run",
  "register": "a",
  "parent": "SEQUENCE_NODE_ID",
  "cursor": 20,
  "selected_child": "SELECTED_DIRECT_CHILD_ID",
  "visual_selection": null,
  "count": 3
}
```

To invoke on Original, `run` and `apply` accept an optional `original` object:

```json
{
  "asset": "ORIGINAL_ASSET_ID",
  "qualification": "COMMITTED_QUALIFICATION_ID",
  "ordinal": 40
}
```

The host loads its committed measured index; the request cannot supply one.
`parent`, `cursor`, `selected_child` and `visual_selection` still describe the
independent retained Edit context. Results return `original`, and each trace
entry returns `before_original` and `after_original`. Omission or `null` uses
the Edit clock.

`selected_child` is independent of `cursor`. Omission or `null` means no selected
beat. A motion may establish one. Whole-beat yank requires one; seam paste also
requires one unless the destination Sequence is empty. Group objects can use
the containing nonroot Sequence. A stale or non-direct child rejects
the request. Optional `visual_selection` is tagged `time` or `object`;
omission or `null` means no Visual selection. For example,
`{"type":"time","anchor":30,"head":20,"extending":false}` is a finished backward range
independent of the cursor. The trace includes before/after child, Visual
selection and Edit positions, plus `removed_range` for replacements.
Object Visual holds `selection: {kind: {type: "inner_group" | "around_group"},
group: "NODE_ID"}` and `extending`. The group identity must belong to the current
ordinary navigation context. Membership and geometry are derived from that
revision. Capture traces also expose the effective `resolved_parent`, exact
timing identity, staged scope labels and bounds.

For example, this body copies the selected beat to `b` and pastes it after that
beat. Set `before` to `true` for a paste before the selected beat:

```json
{
  "instructions": [
    { "type": "yank_beat", "register": "b" },
    { "type": "paste", "register": "b", "before": false }
  ]
}
```

A paste-only body may use a previously saved Original or Edited register.
Each counted repetition selects its newly pasted root before the next begins.

This body selects the next four frames and replaces them with register `b`:

```json
{
  "instructions": [
    { "type": "begin_selection" },
    { "type": "move_frames", "forward": true, "count": 4 },
    { "type": "replace_selection", "register": "b" }
  ]
}
```

`yank_selection` and `cut_selection` also take a register. `finish_selection`
retains the endpoints and stops extending; `clear_selection` removes them.
`move_beats` takes `forward` and a positive `count`; `move_scope` takes `end`.
These instructions keep the same bounded work and atomic failure rules.

`select_object` takes `object: {type: "inner_group" | "around_group"}`. It
resolves a fresh group and installs Object Visual state. Typed operators use
`selector: {type: "text_object", object: {type: "inner_group" | "around_group"}}`.
Each counted instruction resolves that intent from the preceding staged context.
See [group objects](STRUCTURAL_SELECTIONS.md#group-object-workflow) for selection,
empty contents and scope continuation. Native `p/P` records `replace_selection`
for either Visual kind. Explicit semantic `paste` replaces Object Visual; with
Time Visual it retains its existing seam-insertion behavior.

The existing ordinary Sequence reducers still refuse range endpoints inside
a composite child. Enter that group or select its complete boundaries. Visual
macro support does not yet add partial Repeat/Retime occurrence editing or
the full semantic text-object grammar.

Save dry-run checks the same program, bank capacity, version and storage rules
as save. It does not execute the body. Run dry-run resolves the whole program
and admits its Compound without saving history, captures or registers.
Motion-only runs return the resolved position without writing or moving the
native cursor. The JSON request's `dry_run: true` remains effective when the
command-line flag is absent.

An open native project executes through its authenticated writer. The host
prepares runtime copies before committing and publishes the saved bank even
when workspace refresh fails. Successful bank writes retain
`committed_registers` with project ID, revision ID and bank version, independently
of detailed output; authored runs also retain `committed_revision`. A compact
reply preserves both receipts if full detail exceeds transport capacity.
Socket uncertainty never authorizes automatic replay. Remote operations wait
for unread native edit, copy and Macro continuations and do not create their
own native cursor continuation. See [live project access](LIVE_PROJECT.md).
