# Sound voices in structural occurrences

Beat-owned sound needs the same structural time maps as its host, with an
independent source recipe and processing history. This work adds the borrowed
preparation boundary needed for that behavior. Persisted sounds remain limited
to the root bus until their ownership, editing and copy lifecycle are implemented.
It does not enable `ib`/`ab` or add a native placement control.

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
raw time-mapped preparation; event gain, creative edges, scoped allowances and
mixing into the authored bus remain separate work.

## Remaining authored integration

The final product still requires all of the following:

- Persist beat-owned recipes and their independent retained clocks.
- Preserve or transform sound intervals through Split, Trim, Move, Group,
  Repeat, Retime, occurrence isolation and deletion.
- Copy owned sounds with fresh identities and retained media/processing
  provenance; distinguish `ib` from `ab` without changing picture bounds.
- Schedule all active sound occurrences with bounded work, apply owned
  treatments and allowances, and mix once before the shared limiter.
- Expose captured native placement/editing through the common command path,
  then verify preview/export equivalence and audible behavior.

Root sound editing and its guards remain described in [sound events](SOUND_EVENTS.md).
The [qualification record](qualification/owned-sound-voices-2026-10-03.md)
retains the measured checks and limits. No full-product requirement or release
gate is complete.
