# Sound voices in structural occurrences

Beat-owned sounds have independent source recipes and processing histories.
The saved command path now connects those recipes to bounded occurrence
preparation and the canonical authored bus. Independent retained clocks now
support moving and copying unchanged sound-bearing subtrees through ordinary
Sequence edits. This does not enable `ib`/`ab` or add a native placement control.

## Saved attachment contract

`ProjectDocument::beat_sounds` maps an owner NodeId to local SoundIds and
`BeatSound` recipes. The pair identifies an attachment. Root-bus `SoundEvent`s
remain separate, even when an attachment names the root as its owner.
`SetBeatSound` explicitly authors or replaces a recipe; `DeleteBeatSound`
removes it. Both use normal revision checks and reversible transactions.
Root and beat maps share the 64-event limit. Empty owner maps are invalid.

Recipes require a qualified source, an exact natural-rate mapping and a
selected interval that fits the owner's output clock. Store admission rechecks
changed owner/ID pairs against the selected or validated captured revision,
the source receipt and its retained original. Copies rename owner IDs while
retaining local SoundIds. Whole-child capture carries recipes and their complete
sample history into an eligible ordinary Sequence destination. Even a sound
without previous moves retains its source placement before the copy is pasted.

Explicit-timing insertion, deletion, replacement, slice paste and MoveRange can
transport surviving attachments when their complete scoped processing
subtree is unchanged. Whole-owner deletion removes its events and clocks.
Changing that subtree, partially splitting an owner, or transporting root-owned
attachments fails explicitly.
Identity-preserving metadata edits and sound Set/Delete remain available.
Deleting an attachment before pasting a historical copy is an explicit change
of authored intent. Existing root-only sound editing keeps its qualified routes.
Partial-copy timing qualification and `ib`/`ab` await the full attachment
transform lifecycle.

Database 58 stores core schema 46. Earlier unused development packages refuse
before writes; see [development formats](DEVELOPMENT_FORMATS.md).

## Saved independent clocks

`AudioBindingState::sound_clocks` maps each owner/local-ID pair to a nonempty
chronological `SoundClockJournal`. Its live `scope` identifies a complete
processing subtree. Each `SoundClockReference` names a frozen timing record,
historical scope and historical owner. They describe each pre-edit placement;
the live final placement is implicit. Independent journals never become physical
Source bindings. A move away and back keeps both steps, including any intermediate
sample clipping.
An unchanged origin adds no step. Commands use their explicit fresh timing ID
and share an exactly equal capture with existing physical binding helpers.

The supported transport compares the entire sound-bearing scope: durations,
child order, stable Repeat identities and overrides, gap
geometry and Retime mappings/pitch. Source placement, current Hold policy,
labels, framing, treatments and edge policies do not define the independent
processing clock. Unsupported structural changes reject the transaction.
Changing a sound's source, mapping or offset explicitly replaces its recipe
and clears that sound's journal. Label, gain and edge changes retain it.

## Copying retained sound clocks

Both live and historical scopes have only ordinary Sequence ancestors. A
bounded paired traversal proves their complete processing structure matches
while allowing fresh node IDs. It maps each live owner and concrete Repeat path
to its historical counterpart; missing or ineligible override paths fail.
The planner binds a copied occurrence's alias to its exact current and historical
plans before accepting different instance identities for retained gate envelopes.

Capture appends the source's current placement to each selected sound journal.
That placement was previously implicit and can contain clipped sample support.
A previously unclocked sound starts with this capture-time placement. Selecting
an inner ordinary Sequence child narrows an enclosing scope to the corresponding
historical subtree; selecting an outer group preserves any smaller existing scope.
Partial sound-bearing owners still refuse until their interval lifecycle exists.

Paste allocates fresh live nodes, historical aliases and timing IDs. Corresponding
Repeat families are joined by the scope proof before renaming their stable plays;
equal old names alone never establish a relationship. Every retained clock stays
in chronological order. Recopies append their own capture-time placement, and
later supported moves append new live pre-edit references without rewriting old
aliases. Register and paste admission recapture the immutable source revision;
preview and commit recheck the qualified media separately.

The planner compiles each first retained structure with only the current
qualified assets used by sounds sharing that clock. It preserves the complete
processing graph without introducing primary Source audio, Hold carriers or
historical gain. The audio bus retains the first complete occurrence and
transports its old integral PCM labels through every placement. Intermediate
layouts supply an exact translation of that processed extent; their nominal
owner geometry never replaces the audio allocation.
At most 64 first plans are retained, and their combined selected asset count
is bounded by the same 64-event limit. The bus then applies current Hold gates,
edge choices and owner/event gain. The virtual edge envelope keeps its historical
progress even when a later allocation clips physical samples. At an exact shared
boundary, a current Hold owns its newly rounded endpoint.

All plans share one preparation controller, deadline, work budget, source
admission and PCM residency cap. Intrinsic prepared-stage cache entries include
their immutable plan identity. Retained source reads use the current revision
and exact expected asset record; clock-only store changes recheck receipts and
original ownership. A warm cache cannot conceal source revocation.

Remaining work includes temporal edits inside the surviving processing branch,
retained-clock copy/import aliases, occurrence isolation, root-owned attachment
transport, allowances, tails and native placement. Full beat objects still
require their complete attachment lifecycle.

## Ownership and clocks

`RenderPlan::source_voice_occurrence` resolves one explicit `InstancePath` and
an `AudioSourceVoiceRecipe` against one immutable plan. The path retains every
enclosing stable Repeat play. An override must name the effective owned branch;
a default child cannot stand in for a different play's override. The constructor
does not enumerate other plays or infer an owner from a cursor.

The source recipe has a complete qualified span, exact natural-rate mapping,
owner-local selection and an independent signed 48 kHz offset. Its selected
interval must fit the owner's output duration. Placement changes no picture
duration. A qualification identity still requires live host media admission.

Ordinary Sequences position the voice without changing its rate. A concrete
Repeat occurrence chooses the correct placement, including preceding gaps.
FollowSpeed changes the sample map. Each enclosing nonunity Preserve stage
processes this voice independently using the existing canonical DSP. A sound
owned by a Retime enters at that node's output and bypasses its processor; a
sound on its child passes through it.

The borrowed voice uses its plan's structural maps. The Original's retained source
bindings, source absence, endpoint masks and processing cache do not become
this sound's history. Saved journals select the appropriate frozen plan without
rebinding its borrowed handles to a different plan.

## Preparation and policy

`AudioSourceOccurrence` is a checked handle borrowed from its exact plan. It
retains the independent source identity and the prepared projection graph.
Intermediate tapes use their intrinsic PointCeil grids. The final allocation
and lookup use absolute RoundEven root samples; intermediate storage counts
never determine the authored rate or replace the final clock.

Every Preserve input contains the complete selected occurrence in its parent
context, with explicit silence where that occurrence does not exist. Queries
near the end still prepare the complete required history. Current silent Holds
remain an output policy, applied after processing on the final consuming grid.
They do not punch holes in the source input or restart a processor. Primary
picture/audio absence does not suppress an independent sound.

`StageAudio::read_source_voice_occurrence` uses the existing source, resampling
and projected-stage readers. It retains their cancellation, deadline, work,
dependency and PCM residency limits. A masked read still admits the sound's
source. A foreign plan or invalid range refuses before media access. This is
raw time-mapped preparation. The saved bus path below adds gain and edges;
scoped beat-sound allowances and tails remain unimplemented.

## Bounded occurrence windows

`RenderPlan::audio_owner_occurrences` finds the current concrete occurrences of
one owner that can contribute to a root sample window. It follows compact
Repeat identities and active overrides without expanding unrelated plays.
Its geometric allocation and exact owner-to-root map are separate from its
processing influence: a voice below Preserve may continue across that stage's
whole output. The query ignores the Original's retained bindings and audio
availability. Work or count exhaustion fails the entire query.

`RenderPlan::source_voice_occurrences` validates an independent source recipe
and retains the relevant occurrence handles for that window. Its aggregate work
and retained-run bounds cover the whole batch. A window with no occurrences
still validates the recipe. The handles retain processing graph identities
across smaller reads; they do not establish a persistent processed-PCM cache.

`StageAudio::read_source_voice_occurrences` reads a subwindow, checks static
processing histories before opening media, and shares one deadline, work budget
and PCM residency cap. Runtime reservations remain enforced throughout the read.
Each occurrence keeps its own full Preserve input and current Hold gates.
Contributions sum in f64 before checked f32 conversion.
No normalization or source-endpoint mask is applied to the combined output.
Only intervals suppressed in every contribution are reported as suppressed.
A window with no contributing occurrences still revalidates the source.
This remains raw preparation, without event gain, creative edges, allowances
or authored bus placement.
See the [batch qualification record](qualification/occurrence-batches-2026-10-03.md).

## Retained occurrence sample routing

`AudioRoutedRoot::occurrence` captures a complete checked independent occurrence,
including its nested Preserve graph. Its route recipe uses frames relative to
the occurrence's extent start, with a RoundEven grid origin at the negative of
that start. Admission requires the exact extent, sample spacing, grid origin and
complete old sample allocation. Equal sample counts alone do not prove a match.

The routed reader copies old integral sample labels through each chronological
edit. It does not reround an edit duration or derive fresh source phase from
destination frame coordinates. Intermediate clipping remains part of the route;
a later expansion cannot revive a sample removed by an earlier allocation.
All queried spans share one preparation budget and complete processing history.
Even an entirely silent route must pass depth, work, residency and source
admission. Current consuming Hold gates, edges and gain apply separately after
this raw retained input.

This borrowed route does not itself persist a sound clock or lift a temporal
command guard. The saved-clock integration above supplies the sound's own
processing definition, stable occurrence correspondence and chronological
placements. Original audio bindings cannot substitute for those records.
See the [routed occurrence qualification](qualification/routed-occurrence-pcm-2026-10-03.md)
for sample comparisons, exact rounding checks, review and limits.

## Neutral grouping

`Group`, `GroupSelection` and `Ungroup` preserve absolute time, so they keep
saved sound clocks, timing layouts and owner recipes unchanged when the change
lies outside every retained journal scope. A wrapper may stay in place through
later timing edits; the journal's scope still names the same processing subtree.
Grouping or ungrouping inside a retained scope, removing a Sequence that owns
sounds, and selected groups that need endpoint Splits refuse atomically.
Core tests cover direct, child, sibling and range groups plus refusals; the audio
definition test compares rendered occurrence PCM before grouping, after a later
Hold insertion and deletion, and after Ungroup.

## Authored bus and remaining work

The canonical authored bus prepares every saved event's bounded occurrences
before media work. Each retains its full independent Preserve history. Current
Hold gates and whole-island edges apply after processing, followed by the event
gain and owner-to-root treatments evaluated in their declared current clocks.
Exactly coincident ancestor Hard boundaries override automatic edges. Query
cuts do not create fades. Original audio, root sounds and beat occurrences sum
in f64 before a checked f32 conversion and the existing common limiter.
Even an event with no audible occurrence must admit its source; cached limited
PCM cannot hide a revoked dependency.

The final product still requires all of the following:

- Extend independent sound clocks to edits inside a surviving processing branch.
- Preserve or transform sound intervals through Split, Trim, grouping inside
  a retained scope or with endpoint Splits, Repeat, Retime and occurrence
  isolation.
- Extend whole-owner copying to partial captures and timing-preserving edits;
  distinguish `ib` from `ab` without changing picture bounds.
- Add scoped beat-sound allowances and permitted tails.
- Expose captured native placement/editing through the common command path,
  then verify preview/export equivalence and audible behavior.

Root sound editing and its guards remain described in [sound events](SOUND_EVENTS.md).
The [qualification record](qualification/owned-sound-voices-2026-10-03.md)
retains the initial preparation checks. The
[saved-sound record](qualification/saved-beat-sounds-2026-10-03.md) covers
persisted recipes and the authored bus. The
[saved-clock record](qualification/sound-clocks-2026-10-03.md) covers supported
temporal transport. The
[copied-clock record](qualification/sound-slice-clocks-2026-10-03.md) records
whole-owner copy and register verification. No full-product requirement or release
gate is complete.
