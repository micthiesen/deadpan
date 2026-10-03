# Sound voices in structural occurrences

Beat-owned sounds have independent source recipes and processing histories.
The saved command path now connects those recipes to bounded occurrence
preparation and the canonical authored bus. Temporal editing still needs
independent retained clocks. This does not enable `ib`/`ab` or add a native
placement control.

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
retaining local SoundIds. Whole-child capture can carry those recipes into a
destination with no existing beat attachments. Their placement uses the new
current owner clock; this does not claim retained historical sample phase.

Timing changes in a document containing beat sounds fail explicitly, including
Split, Trim, Move, duration/Repeat/Retime changes, ripple deletion and paste.
Identity-preserving metadata edits and sound Set/Delete remain available.
Deleting an attachment before pasting a historical copy is an explicit change
of authored intent. Existing root-only sound editing keeps its qualified routes.
Partial-copy timing qualification and `ib`/`ab` await the full attachment
transform lifecycle.

Database 56 stores core schema 44. Earlier unused development packages refuse
before writes; see [development formats](DEVELOPMENT_FORMATS.md).

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

The new voice uses current structural maps. The Original's retained source
bindings, source absence, endpoint masks and processing cache do not become
this sound's history. Future authored edits must retain the sound's own clocks
and complete processing recipe explicitly.

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

- Persist independent retained sound clocks across temporal edits.
- Preserve or transform sound intervals through Split, Trim, Move, Group,
  Repeat, Retime, occurrence isolation and deletion.
- Extend whole-owner copying to partial captures and timing-preserving edits;
  distinguish `ib` from `ab` without changing picture bounds.
- Add scoped beat-sound allowances and permitted tails.
- Expose captured native placement/editing through the common command path,
  then verify preview/export equivalence and audible behavior.

Root sound editing and its guards remain described in [sound events](SOUND_EVENTS.md).
The [qualification record](qualification/owned-sound-voices-2026-10-03.md)
retains the initial preparation checks. The
[saved-sound record](qualification/saved-beat-sounds-2026-10-03.md) covers
persisted recipes and the authored bus. No full-product requirement or release
gate is complete.
