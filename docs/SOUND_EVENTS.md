# Sound events and scoped audio mixing

External sound effects must become editable sound events over the existing edit.
Catalog import retains and qualifies audio-only sources. Catalog audition uses
the shared playback engine; it does not place an event. The complete requirement remains in specification Sections 4.2,
5.3–5.4, 8.3, 9.3 and 10. A separate audio player, blank-picture Source insertion
or a sum added after the limiter would not implement that requirement.

## Required ownership and timing

A sound belongs to a host's output clock. A sound inside a Repeat child plays with
each effective child; a sound on the Repeat itself spans the repeated passage.
A sound inside a Retime child enters that stage's input. A sound on the Retime
itself enters after that stage. Enclosing time maps affect each child voice in
its declared clock. Specification Section 10.2 places time/pitch processing on
each voice before the group bus. Keep the existing sequential Original voice's
complete continuous processing history; do not split it into independent
per-beat processors. Distinct attached sounds retain their own complete voice
history and scoped output gates before mixing.

An explicitly declared aggregate processing stage may process a mixed input
once. That is a different operation from independently processing voices and
summing their outputs. It must not become the default merely because the current
structural audio tree previously had only one sequential voice.

Prefer node-owned sound recipes with identities scoped by their authored owner.
Each recipe needs a qualified source span, its complete exact source mapping,
owner-local placement, independent 48 kHz offset, owned edge/effect parameters,
and explicit silence and tail intent. Keep its retained processing recipe and
continuity separate from its audible interval projections. An event's duration
never contributes to structural picture duration. Placement cannot silently trim
a sound, extend picture time or fit its source to the host. Overflow needs an
explicit policy; cross-host tails additionally need a destination and ownership.

An anchor's exact coordinate is usable for placement, but a point mark plus a
length is insufficient for editing intervals. Deleting the start of an event
must not erase its surviving suffix. A source phase, processing history or fade
cannot restart merely because an event is represented by another fragment.

The root-owned subset below has validated commands, migration, rendering and
native controls. The native workspace exposes that subset explicitly; the
complete ownership and edit contract remains required product work.

## Native root placement

Choose an audio-only catalog sound and use `,s` or `:sound-place` to place its
complete measured span at the retained Your edit cursor. The destination uses
the project-origin 48 kHz sample boundary, independently of Original browsing
and the catalog audition cursor. Placement neither adds picture time nor trims
the sound: a selection extending beyond the root is rejected without an edit.

The Placed sounds pane has its own event selection, separate from the catalog
asset and structural beat. `:sounds` or pane navigation focuses it; `j/k` chooses
an event. Its controls use the same revision-bound project service and reversible
store transactions as structural edits:

| Input | Result |
| --- | --- |
| `h/l`, with an optional count | Move an unrouted event by exact project frames. The durable mapping retains fractional phase, so repeated nudges do not accumulate rounded sample durations. |
| Enter | Open fine position entry. Accepting the unchanged prefilled value preserves fractional phase and creates no history. |
| `:sound-at 274000` | Explicitly set the selected onset to a whole, nonnegative 48 kHz sample coordinate. |
| `+` / `-`, with an optional count | Change the selected event's gain by 3 dB per step. |
| `:sound-gain -3.125` | Set exact gain, with at most three decimal places, within -96 through +24 dB. |
| Soft / Hard or `:sound-edges soft\|hard` | Set both event endpoints to Automatic or Hard. |
| `dd` or `:sound-delete` | Remove the selected event. |
| `:sound-allow` / `:sound-silence` | Grant or revoke this sound's permission in the concrete silent pause under the Edit frame. |
| `u` / Ctrl-R | Use ordinary durable undo/redo. |

Parameter command entry captures the event, project session and revision. A
changed or missing target cannot be acquired from a later asynchronous reply.
Native text and IME retain input ownership; placement accepts neither a count
nor held-key repetition. Success is reported only after the transaction commits.
The contextual `dd` key removes a sound, while command entry uses the explicit
`:sound-delete`; `:delete` is rejected in sound context. `:source` and `:sequence`
leave event focus, and the viewer's navigation buttons always move picture time.

Routed events keep their timing journals when gain or edges change. Native move
and frame-nudge controls reject them rather than resetting their cuts. Silent
Holds suppress event output by default. The inspector identifies the exact pause
at the retained Edit cursor; `:sound-allow` permits only the selected effect in
that occurrence and `:sound-silence` revokes that permission. The caption shows
the Edit frame and readable pause/play context, while the command retains the
complete stable issuer. Eligibility checks the pause's samples within
that frame, so an effect wholly between frame boundaries remains selectable.
Missing targets and routing gaps cannot gain
audible support. An existing allowance remains revocable when support moves away.
Nested owners, Repeat/Retime sound transforms, send/tail allowances, effects,
full mastering and export remain open.

The [placement board](design/boards/sound-placement-board-v2.png) is the visual
target for this subset. The [native placement qualification record](qualification/native-sound-placement-2026-09-27.md)
tracks its checks and limits; this increment completes no product requirement or
release gate.

## Persisted root sounds

Core schema 29 and database 35 introduced `SoundEvent` recipes keyed by `SoundId`.
The current schemas are core 31 and database 37. `SetSound` creates an event or
updates it under the retained-route rules below; `DeleteSound` removes it. The
shared headless command entrypoint supports preview and atomic commit, followed
by ordinary durable undo/redo. Migration replays old chronology through closed
adapters and adds no sounds to historical projects. The actual database-34
fixture includes ordinary and occurrence Retime edits, an abandoned branch and
pending redo.

This increment admits at most 64 root-owned events. Each retains a qualified
source span, explicit natural-rate mapping and optional exact audible selection,
a signed 48 kHz offset, milli-decibel gain from -96 dB through +24 dB, independent
Automatic/Hard edge choices and an explicit `Reject` overflow policy. The whole
selected interval must fit inside the root. Sound placement never changes
picture duration. A qualification ID alone is insufficient: storage rechecks
the revision's immutable receipt and original binding during the command
transaction, and playback admits actual source bytes through its existing
provider.

The root reader evaluates source phase, allocation and current silent-Hold
policy directly on the absolute RoundEven grid. It does not relabel PointCeil
PCM. Each sound receives its own edges and gain before f64 summation in SoundId order
with the existing continuous Original voice. The bus converts once to finite
f32, then enters the existing single canonical limiter. Exhaustion applies to
one contribution; bus suppression is an intersection. Even a wholly suppressed
sound remains a dependency of cached PCM. Original and catalog audition use
source-only views and exclude the edit's sound events.

Silent Holds cut that voice's audible intervals with the same sample-centered
2 ms edge envelope, shortened for tiny surviving intervals. Source phase continues
under the gate. Exact event, Hold and owner boundary choices determine Hard
exceptions; unrelated Original cuts create no sound edge. A bounded 192-sample
lookaround discovers short intervals independently of request chunks. Context
limits never introduce a fade or erase a genuine fractional endpoint.

`StageAudio::prepare_edge_faded` returns
`authored_bus_pcm_before_mastering` when events exist. `LimitedAudio` and
`inspect-audio --limited` return `limited_authored_bus_pcm`. The small raw,
time-mapped and `--edge-faded` diagnostic readers retain their Original-only
meaning. With no events, previous PCM and metadata stay unchanged.

Nested ownership, the remaining structural interval transforms, custom Hold
send/tail allowances and creative treatments remain open. The
root ripple subset below preserves sounds through supported edits. Other
temporal commands and unsupported frozen audio captures still fail explicitly.
Renaming, framing, audio mapping,
edge choices and other clock-preserving edits retain their normal transactions.
This root subset is not full sound editing or a completed audio master.
See the [qualification record](qualification/root-sounds-2026-09-27.md) for
review findings, real-PCM comparisons, resource limits and verification evidence.

## Persisted root ripple edits

Core 30/database 36 retain `sound_routes` separately from complete `SoundEvent`
recipes. Each journal stores the original root extent and RoundEven grid, then
each Insert/Delete operation, its grid and genuine cut policies in chronological
order. Never combine adjacent edits into one frame offset. `InsertTime` and
`SpliceSource` introduce sound-free time; Delete through ordinary Sequence
ancestors removes the intersecting interval while preserving a surviving suffix.
Survival follows the retained integral selected support through each physical
Keep, including destination clipping. An initially sampleless selection uses
exact logical support so unrelated edits preserve its accepted intent. Removing
all selected support deletes the event. Internal helper Splits do not apply the
transformation twice. Non-root Split retains the root bus unchanged.

`SetSound` changes label, gain or endpoint policy without discarding the journal.
Changing a routed recipe, mapping, owner or offset requires `ReplaceSound`, which
explicitly clears the previous route in the same reversible transaction. Stored
recipe containment is checked against its original extent, not today's shorter
root. Route-only changes recheck their unchanged source receipts too.

The raw `AudioRootSource` capture binds the full original recipe directly to its
RoundEven grid. Routed reads copy old physical samples using the caller's shared
work, deadline, dependency and PCM budgets. Current silent Holds stay live and
are applied once to the routed output. They never become frozen input silence.
Recipe endpoints retain both exact semantic coordinates and integral physical
labels: move the labels by the retained sample shift, never reround them after
an odd shift. Genuine cuts replace affected envelope boundaries; transparent
Split and query boundaries supply none. Intersect current Hold boundaries with
these retained boundaries and evaluate one envelope. Exact coincidence combines
Hard intent while the current Hold owns its clipping sample label.
Virtual envelope labels use widened integers; clip translated physical support
before narrowing it to an output sample label. A valid clipped allocation can
retain a virtual envelope endpoint outside the output integer range.

The journal has a hard 1,024-edit bound and all document journals share a 1 MiB
serialized bound. Existing compiled arena caps can apply earlier: 819 interior
insertions fit the 4,096-node cap. History traversal is iterative and bounded;
each Ripple map still has structural depth at most 64. Root Split, temporal
occurrence edits, nested sound ownership, Repeat/Retime transformations and
general retained sound-bus captures remain required. Their guards stay closed.

The immutable render plan compiles sampled routes and transported envelope
islands once. Construction has aggregate ceilings of 16,777,216 visits and
16,384 retained islands. A read seeks intersecting islands by index and combines
only their boundaries with current Hold policy. Audio blocks must not rebuild
or forward-project every chronological island on each query.

Database 35 replays through frozen core 29. Its original sound-bearing command
restrictions are checked before modern apply, including a formerly forbidden
Split whose modern snapshots and patches contain no new route field. The actual
old-CLI fixture retains qualified media, two sounds, abandoned sound parameters
and pending redo. Migration adds no journals to old snapshots.
See the [routing qualification](qualification/root-sound-routing-2026-09-27.md)
for review corrections, decoded-PCM comparisons and verification scope.

## Structural edit contract

| Operation | Required result |
| --- | --- |
| Move or Group | Keep owned recipes and phase with their owner. Ancestor-owned sounds stay in that ancestor's output clock; intersecting a moved child does not transfer ownership. |
| Split | Copy complete retained context, then select each output fragment. Do not create new independent fades or preparation histories. |
| Root Split | Put the old root's sound bus in retained contexts; the replacement structural root must not emit it again. Explicit pinned events remain a separate policy. |
| Occurrence isolation | Copy sounds with their owners, remap physical identities and scoped allowances, and preserve continuity independently of new edit identity. |
| WrapRepeat | The existing owner's sounds remain inside the new Repeat and follow its plays. |
| SetRepeat or MovePlays | Child-owned sounds follow stable plays and their overrides. Sounds owned by the Repeat stay in its output clock; changed duration uses their explicit overflow policy. Reordering child plays does not reorder the Repeat's own sound. |
| WrapRetime | Apply the new time map to every child-owned voice, retaining each complete processing context. Ancestor-owned sounds stay in their owner's output clock. An explicit aggregate bus processor is a separate choice. |
| SetRetime | Keep an event authored on that stage in its declared output clock; do not silently reinterpret it as child-owned. Validate any changed extent and explicit overflow policy. |
| Delete | Remove owned events and project surviving parts of intersecting ancestor events without a source restart. |
| Ungroup | Redistribute exact projections of the removed bus, retaining its complete recipe and continuity. Independent shortened source copies are insufficient. |
| InsertTime or source splice | Capture old clocks first, exclude inserted content from default event support, and resume surviving sound on its prior phase. Preserve input clocks and moved opaque processing outputs remain distinct. |

This follows the ownership distinction in specification Sections 5.3 and 5.4.
Moving a group moves its owned attachments; it does not turn every overlapping
ancestor event into a child attachment. Ordinary internal reordering or retiming
therefore requires no implicit cross-clock transport of a parent sound. Explicit
ripple insertion/deletion still transforms intersecting host-local intervals;
sequence-pinned events have a separate, visibly fixed policy. Preserve complete
historical processing domains and select exact fractional windows afterward.
A fractional selection alone does not require a new fractional DSP history.

A physical fragment may share a retained logical event recipe with another
fragment. Editing a logical event must identify that scope; independently changing
one occurrence or fragment needs explicit isolation. This cannot be inferred from
which copy happens to be visible.

## Silence and edges

### Persisted root sound allowances

Core 31/database 37 retain a separate `sound_allowances` relation. Each entry
names one `SoundId` and a concrete `SoundHoldIssuer`: a silent Hold's complete
`InstancePath`, or a default Repeat gap's owner path and stable preceding play.
There are no definition wildcards or permissions inferred from sample ranges.
`SetSoundAllowance { sound, issuer, allowed }` grants or revokes one address in
one normal reversible transaction. Duplicate, stale, incomplete and oversized
addresses fail validation. An allowance-only edit rechecks source admission.

Transparent Split copies the issuer with its retained context; occurrence
isolation remaps only the selected path. InsertTime and source splice carry those
changes once through helper splits. Deleting a sound or issuer prunes its relation;
undo restores it. Parameter changes and explicit recipe replacement retain the
same sound identity and its permissions. Inserting new time grants no permission,
and an explicit allowance cannot recreate sound absent from a retained route gap.

Root sound preparation uses complete raw input. One current per-contribution gate
then combines exact Hold policy, explicit allowances and authored edges on the
root sample grid for both direct and routed sounds. The Original keeps its own
silence, and another sound gets no permission. Source exhaustion, full source
admission and route masks remain independent. Intrinsic definition queries cannot
be promoted into concrete root issuer addresses.

Database 36 replays frozen core 30 with its original sound-bearing command
admission. Older snapshots gain empty allowance state; their closed document,
command and patch grammars reject the new relation even when explicitly empty.
This root contribution policy does not complete nested ownership, effect sends,
hanging tails or the broader voice processing graph.

### Full voice policy

Source exhaustion silences that source voice. It does not silence another voice
at the same time. By contrast, an authored silent Hold suppresses its governed
direct audio and incoming tails by default. These are different policy owners.
A global union of zero ranges cannot represent both meanings. Apply each voice's
Hold/tail gate at its processing output before the default group mix. One scalar
mask after a nonlinear mixed Preserve cannot remove the Original's decay while
preserving a permitted sound at the same samples.

Placing a sound over a silent Hold needs an explicit, visible custom allowance
for that contribution and that Hold. It must not re-enable original speech,
permit unrelated tails or bypass another silent Hold later. Copying and splitting
must retain or remap the allowance with its sound and Hold identities.

Voice edge fades also need their own ownership. An Original cut must not fade an
unrelated continuing effect. Ancestor policy cannot accidentally apply the same
fade twice. The existing single-voice path must retain its samples when no sounds
are added. Creative edge fades, source-support masking, post-Preserve silence and
final limiter processing must remain separately testable.

The authored integration needs a policy view separate from content: a stable
issuer (such as a Hold and its occurrence), a subject (voice or send), default
suppression and explicit allowances. Resolve those rules through the existing
exact clocks and compact Repeat structure onto the consuming output grid.
Materialized `AudioMixGate` ranges and temporary voice indices are not durable
per-occurrence policy identities. The current single-voice output tape can remain
an adapter, retaining its deliberate exclusion of physical source endpoints after
Preserve. Separately processed voice tapes may then enter the borrowed mix.

`audio_hold_policy` now reports current structural silent Holds with their
issuer, preserving definition namespace, occurrence path and a Repeat gap's
stable preceding play. An unplayed gap definition has no invented preceding play.
Root and placed-domain queries use RoundEven; intrinsic signal queries use
PointCeil. Source absence/exhaustion, RoomTone and Tail do not issue silent-Hold
rules. The existing scalar Original policy remains unchanged. These current
rules do not reconstruct historical policy, grant allowances, or flatten a
projected processing operand's independently declared output policy.

## Borrowed mixing boundary

`AudioSignalMix` is a checked, borrowed preparation operand: ordered
voices on one exact intrinsic grid, with explicit gates selecting which voices
they affect. Each voice retains its own source support and policy. The bus is
known silent only where every contribution is explicitly silent or suppressed.
For explicitly aggregate processing, a mixed-input `AudioStageProjection` feeds
the complete sum into one canonical Preserve while retaining an independently
declared output-policy clock. This capability does not establish aggregate
processing as the default for separate authored voices.

This preparation primitive must share existing source admission, cancellation,
deadlines, depth, cumulative work, PCM residency and dependency checks. It must
not materialize Repeat plays, use a second rounded timeline, skip validation of
a gated voice, or reuse a projected result based only on an authored descriptor.
Its raw PCM precedes creative effects and mastering. It does not by itself
establish persisted sound events, the complete voice graph or native placement.

Construction accepts 1 through 64 complete `AudioSignalTape` voices on the same
live plan, exact support and normalized PointCeil grid. At most 4,096 exact gates
may reference at most 65,536 voice indices in total; one gate cannot name the same
voice twice. A gate is converted on that common grid without restarting phase.
Content, policy, gate lookup and interval composition share bounded query work.
Content spans share an aggregate count limit. A fully gated voice still retains
its policy and processing dependencies for admission.

Explicit raw silence also contributes to that intersection, including absent
source audio and time before a placed source begins. It is separate from authored
tail-suppression policy. Do not infer silence by flattening an opaque processing
stage: its output may still contain decay from earlier input.

`StageAudio::read_mix` returns 1 through 256 stereo samples, labeled
`scoped_mix_preparation_pcm_before_effects`. Each contribution passes through
the existing raw preparation reader and its own policy before ordered f64
accumulation and one finite f32 conversion. Values above unity remain above unity;
this operation does not clip, normalize or apply the final limiter. A mixed
`AudioStageProjection` uses the same single canonical stretch, independent
output-policy tape, request-local identity and shared resource limits as a
single-tape projection. Every voice must retain the owner's definition,
occurrence and descendant scope, even when gated.

## Exact route kernel

`SoundRoute` retains one complete recipe extent and selects exact output windows
through checked `SoundRippleMap` edits. Keep selects previous output; Gap allocates
time without inherited sound; Sequence concatenates; Repeat advances a compact
edit pattern through its input with a declared stride. This Repeat is a periodic
ripple map, not an instruction to replay one sound. A deletion that removes an
event's beginning keeps the surviving recipe suffix and its original phase.

Both arenas require a final root, earlier-only references and complete reachability.
Admission memoizes extents, monotone input footprints, depth and Sequence indexes.
Combined routes admit at most 4,096 nodes and 16,384 edges, with a 1 MiB
serialized bound. Ripple maps have structural depth at most 64; chronological
route history uses a bounded iterative traversal independently of that depth.
Use `from_json` to reject oversized wire input before parsing;
embedding callers using serde must bound their containing input too. Queries
default to 65,536 work units and 4,096 result spans, with hard caps of 1,000,000
and 16,384. Short queries seek directly to a Repeat ordinal or Sequence prefix;
an entirely gap-only subtree returns one gap without expanding its plays.

The kernel selects exact unity-rate intervals. It does not choose a sample grid,
process PCM, persist events, or transform project edits. Whole-event removal is
the authored layer's responsibility because stored route extents are positive.
Fractional windows retain the full recipe rather than creating fractional DSP
histories. JSON admission revalidates all derived extents and indexes.

`SoundRippleMap::locate` returns the complete immediate-input and output leaf
interval, with explicit seam bias. It seeks compact Repeat ordinals and Sequence
prefixes directly. A compressed all-gap subtree returns one interval; an outward
endpoint returns no interval. This differs from a flattened final recipe query:
the intermediate physical cut is necessary for sampled editing.

## Retained sample routes

`AudioSoundRoute` binds one captured sample grid to every chronological route
node. Root output uses branded `AudioSample` RoundEven grids; intrinsic output
uses `SignalSample` PointCeil grids. Unity edits can move an origin but cannot
change grid spacing or boundary rule. Allocation endpoints are checked before
querying. These handles do not serialize new event state or admit media.

Each Keep resolves its old cut on the preceding node's grid and its new anchor
on the current grid, then resumes the preceding sampled output. Further edits
compose that retained sample mapping. At 30000/1001 fps and 48 kHz, two one-frame
insertions can resume at original sample 3204 where reconstructing the final
picture offset would incorrectly choose 3203. Window projections preserve the
complete recipe and apply the same physical-clock rule. Changing a rounded
placement can change its allocated count. Keep and Window retain the previous
selection's physical half-open audible mask; any extra allocated sample past
that cut is explicitly silent, even when the complete prior recipe has more
audio. That output mask does not shrink the retained recipe's filter/DSP support.

Queries return exact recipe-frame sample maps or gaps. Their maps retain one
constant step and are independent of query chunks. They do not choose source
filter support, restart fades, create fractional DSP histories, or substitute
for per-voice preparation. Prior gaps remain gaps when another edit resumes
their sampled output. Sampleless exact gaps require no scanning. A shared query
budget covers history traversal, indexed map lookup and emitted spans; the
existing audio caps are 65,536 work units and 4,096 spans.

The CLI and playback source providers each retain up to 16 qualified private PCM sessions under
a 1 GiB aggregate physical-sample budget, including priming and padding, and
1,000,000 aggregate indexed audio frames. Each keeps compact receipt identity
instead of a duplicate full source snapshot. Least
recently used eviction allows inspection of more sources sequentially. Every
hot hit rechecks the captured asset/receipt/original contract; cold opens verify
original bytes before eviction and reserve space before decode. Failed decode
may leave evicted entries absent but never charges an uncommitted reservation.
Playback reuses its immutable snapshot's shared receipt rather than cloning
the full index into a second cache entry. A new revision still rebuilds the
playback cache; this is bounded residency, not cross-revision cache admission.

## Independent catalog source operands

`AudioSignal::source_voice` derives an immutable `AudioSourceVoice` from a
checked structural owner. The plan retains catalog-only asset contracts as well
as structural sources. Admission requires an audio span contained in the asset's
exact measured clock, a source-qualification identity and an explicit natural-rate
mapping. Signed 48 kHz offsets and selected placement remain exact. FitBeat and
implicit rate changes are rejected; processing uses explicit enclosing stages.
The host still admits the exact revision, receipt, layout and original bytes
before reading PCM. Catalog metadata alone is not media admission.

Input and output views share one opaque voice identity and the complete source
recipe. The input retains raw sound through current silent Holds so canonical
processing has the required history. The output adds only current explicit
silent-Hold rules from its structural owner, evaluated directly on the consuming
grid. Original source absence, endpoints, RoomTone, tails and retained sampling
bindings are not this voice's policy. Hold introspection retains the issuer's
definition and stable occurrence identity. No allowance is implied.

Both views use existing signal tapes, dependency admission, source resampling and
`StageAudio` preparation. Physical query chunks and tape seams keep source phase
and complete filter support; an explicit intrinsic input selection still constrains
support. Source endpoints do not erase processed Preserve decay. Checked
`AudioStageProjection` retains its existing descendant, definition and Repeat
scope checks and independent output-policy tape. Constructing a catalog voice
does not make an unrelated owner eligible for that stage.

These operands read real qualified catalog PCM without adding a Source node or
changing history. They do not persist a sound, grant scoped Hold allowances,
add sound effects or populate the final bus.
The continuous Original reader remains separate and unchanged. See
[source-voice qualification](qualification/source-voices-2026-09-27.md) for the
measured fixture, verification results and remaining acceptance limits.

## Routed PCM preparation

`AudioRoutedSignal` binds a retained PointCeil sample route to either a complete
independent source input or a shared intrinsic Preserve projection.
`AudioRoutedRoot` binds a RoundEven route to a complete captured projected root
or an explicitly checked raw `AudioRootSource`.
Construction checks the original Recipe extent, grid origin, spacing, rule and
allocation against that provider. Equal durations or sample counts are not enough.
Cropped, transformed or resumed captures are rejected; select later output with
the route while retaining the complete provider. The independently placed raw root
source has its own checked capture and is never inferred from sampled PointCeil PCM.

Root Recipe frames are relative to the complete output's start, while its sample
labels remain absolute. The grid therefore retains the negative of that start as
its frame origin, including signed and fractional placements. The captured root
keeps its original exact sampling map and policy; a later route never substitutes
the current destination frame coordinate for either.

The `StageAudio` routed readers turn each exact recipe lookup into an integral
old sample label, evaluate that old provider and copy its samples to the new
allocation. They reuse source resampling and canonical preparation, with one
deadline, work budget, dependency set and projection identity across every span.
Cold suffix reads prepare the complete processing history. Even an entirely
masked route admits its source dependencies and complete projected input.

Route gaps and old selected audible masks remain separate from complete source
filter support and processing history. Captured provider output policy is read
on its old grid and copied with those samples. Current consuming Hold gates,
scoped allowances and creative edges still belong after this routed preparation;
moving an old gate does not implement a current allowance. The source case always
uses the input view, preserving the sound beneath current structural Holds.
Raw root routes now feed persisted root events into the shared pre-master bus.
The broader projected APIs remain preparation interfaces. See
[routed-voice qualification](qualification/routed-voices-2026-09-27.md) for
decoded-PCM witnesses, review, verification and acceptance limits.

## Remaining integration

The remaining integration is required, not optional follow-up scope:

The supported root ripple journal and raw RoundEven capture establish the
current subset. Keep complete recipes, sample clocks, selection support and
envelope progress separate from fragments while extending it. Root Split needs
retained owned contexts and shared logical
sound identity so the replacement root never doubles the contribution. Do not
open public frozen-context guards until those captures include their sound bus.

Decisive witnesses include successive NTSC insertions retaining the old physical
sample rather than reconstructing a final offset; deleting an event's beginning
without losing its suffix; sample-identical root/non-root Split without new fades;
nested source splices with sound-free inserted time and unchanged old suffixes;
and bounded compact histories whose masked dependencies remain revocable.

1. Extend root-owned recipes to nested owners and bounded interval projections
   through validated, reversible commands. Implement the structural edit table
   above while preserving strict old history grammars and actual old-CLI fixture
   provenance during migration.
2. Compile all live and retained sound dependencies into root, definition,
   processing-input, bound-domain and projected readers. Stop at each processing
   boundary so descendant voices enter once. Sound catalog assets need qualified
   source admission without creating picture content.
3. Extend the root sound allowance subset to nested voices and sends; resolve
   voice/ancestor edge ownership. Preserve
   continuous per-voice time/pitch processing and its output gates. Apply gain,
   treatments, sends and group processing in the declared order, then feed the
   complete bus into the shared limiter. Extend host scheduling beyond the current
   bounded source caches without weakening source admission.
4. Extend the native root placement controls to the remaining owner, transform,
   treatment and remaining custom-silence policies. Keep Original and edited
   clocks separate and use the common typed command path.
5. Extend CLI inspection, production UI replay and real decoded-PCM tests. Verify
   actual preview/export equivalence, listening, failure recovery and performance;
   unit tests alone cannot qualify those acceptance results.

Required PCM evidence includes different source rates, arbitrary sample onsets,
independent scalar sums, per-voice exhaustion, scoped silence, mixed-before-
Preserve witnesses, full retained history through split/repeat/reanchor, cold and
shuffled reads, changed source fingerprints, aggregate limits, and summation
before one shared limiter. Every source, recipe and processed result remains tied
to its immutable project revision.
