# Sound events and scoped audio mixing

The sampled route supports chronological occurrence windows with independent
rounding anchors while retaining complete processing input. Intermediate crops
cannot be undone by a later move or wider output allocation.
Partial attached-sound capture and matching gate transport remain to be integrated.

Independent event EQ and returns share bounded ordered-prefix tiles within each
preparation request. This prevents overlapping histories from repeatedly
evaluating the same input and exhausting plan work. Source admission, exact
clocks and resource limits remain unchanged.

`StageAudio::read_retained_sound_input` reads a checked sound route's complete
input on its original sample clock, before saved cuts/moves and current Hold
gates. Its explicit allocation can extend beyond the shortened live project;
source selection, resampling phase and nested Preserve history remain intact.
Ordinary routed reads share this preparation boundary. Independent event
returns now consume it without granting extra audible time.

Root and attached sounds carry one `AudioTreatments` recipe in `treatments`,
including trim, gain/mute envelopes, EQ, saturation, balance and independent
wet/Tail sends. Core 53 and store 81 replace the former separate event gain and
send fields. Existing development packages are refused unchanged.

Envelope frames start at the complete natural-rate source recipe's exact onset,
before its selected interval. Root and occurrence handles expose that checked
clock; enclosing time maps transport it and saved delivery routes retain it.
Moving a sound carries its envelopes. Cutting its beginning does not restart
them. The default order is input edges, gain, EQ, saturation and balance, then
sends and the group bus. The serialized recipe preserves explicit reordering.
Neutral EQ preserves ordinary delivery exactly.

Processed root dry output has a separate post-treatment envelope. Authored input
endpoints have already fed the processor, so they receive no second fade,
including when a current Hold ends exactly at the same endpoint. New cuts and
Holds at other boundaries keep their own policies. Direct and retained root
routes carry the same distinction through exact frames and transported physical
sample labels. Preparation skips the second envelope only when the canonical
event-return predicate is false; the first gate query and source admission still
run.

The sound inspector exposes `:sound-eq`, `:sound-saturate`, `:sound-balance`,
`:sound-mute on|off` and `:sound-envelope 0f 30f -12dB 0dB smoothstep`.
An envelope with the same exact range is replaced; other overlapping curves add
their dB values. `sound-envelope clear` removes curves while preserving trim,
mute and other stages. Headless `SetSound`/`SetBeatSound` accepts the complete
bounded recipe, including cubic curves, mute ranges and explicit effect order.
All native commands capture the selected event, session and revision and commit
one reversible transaction.

Root and attached sounds carry independent `treatments.wet_sends`. Native
`:sound-wet-send reverb|delay -6dB|off` changes the captured placed sound through
`SetSound`; attached recipes use `SetBeatSound`. Complete time/pitch-mapped
input, its edge fades and its complete treatments feed the finite effect on the retained recipe clock.
Saved delivery routes then transport the processed samples, preserving history
through cuts, inserted gaps and repeated copies. The dry sound and its return
obey current selection/cut fades and silence permissions before
entering group treatments. Returns stop at the existing audible allocation.
The event's own allocated-Tail tap excludes its continuous return; parent sends
can include it. See [finite effects](FINITE_WET_EFFECTS.md#independent-sound-returns).

Selected root and attached sounds carry independent reverb/delay `treatments.tail_sends`.
Native `:sound-tail-send reverb|delay -6dB|off` edits the captured placed sound
through `SetSound`, retaining routes, placement, gain, edges and Hold allowances.
The tap follows the complete event treatment chain; a later `:tail input=sends` pause admits
only matching sends. This does not grant direct audio under silent Holds or
create an ordinary continuous wet return. See
[selected tails](REVERSE_AND_TAILS.md#selected-sends).

External sound effects must become editable sound events over the existing edit.
Catalog import retains and qualifies audio-only sources. Catalog audition uses
the shared playback engine; it does not place an event. A separate audio player, blank-picture Source insertion
or a sum added after the limiter would not implement external sound effects.

## Required ownership and timing

A sound belongs to a host's output clock. A sound inside a Repeat child plays with
each effective child; a sound on the Repeat itself spans the repeated passage.
A sound inside a Retime child enters that stage's input. A sound on the Retime
itself enters after that stage. Enclosing time maps affect each child voice in
its declared clock. Time/pitch processing is placed on
each voice before the group bus. Keep the existing sequential Original voice's
complete continuous processing history; do not split it into independent
per-beat processors. Distinct attached sounds retain their own complete voice
history and scoped output gates before mixing.

Ordinary interior Hold resizing preserves an independent event on an unchanged
fixed Edit Retime output, including a retained root-ripple journal. Its clock
proof may stop at that exact owner after checking the output contract and
placement ancestry. It never supplies aliases for omitted input children.
Changed child-owned processing dependencies still reject atomically. Current
source admission, suppression and owner/ancestor treatments apply to the
current mixed bus. A later
fractional owner-support change
repairs retained overlap at the following Hold's allocation without changing
the independent event's phase or applying that Hold's gain to it. Its complete
nonzero Original/event master and release export pass; genuine physical
exhaustion remains silent and explicitly inactive.

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

A catalog sound whose file declares no speaker layout, such as a plain WAV,
needs an explicit **Unlabelled channels** choice when it is added; see
[speaker interpretation](SOURCE_REGISTRATION.md#explicit-speaker-interpretation).
Audition, placement and export then use that persisted reading.

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
| `:sound-cut` | Bed drop: end the selected event abruptly at the Edit cursor. Its mapping becomes a selected placement whose exact local selection ends at that frame boundary, with a Hard end edge; onset, source phase and gain stay. The cursor must lie strictly inside the audible sound. A routed event (one that follows timeline cuts) ends where the cursor falls in its retained route: the cursor maps back to the recipe's own clock, only the selection narrows and the route is unchanged. Inside an inserted pause the bed ends where that pause begins. |
| `dd` or `:sound-delete` | Remove the selected event. |
| `:sound-attach` | Attach the selected catalog sound to the selected beat, starting at the Edit cursor inside it (a sting). See [beds and stings](#beds-and-stings-through-edits). |
| `:sound-allow` / `:sound-silence` | Grant or revoke this sound's permission in the concrete silent pause under the Edit frame. |
| `u` / Ctrl-R | Use ordinary durable undo/redo. |

Parameter command entry captures the event, project session and revision. A
changed or missing target cannot be acquired from a later asynchronous reply.
Native text and IME retain input ownership; placement accepts neither a count
nor held-key repetition. Success is reported only after the transaction commits.
The contextual `dd` key removes a sound, while command entry uses the explicit
`:sound-delete`; `:delete` is rejected in sound context. `:source` and `:sequence`
leave event focus, and the viewer's navigation buttons always move picture time.

Routed events keep their timing journals when gain or edges change, and when
`:sound-cut` narrows their selection. Native move and frame-nudge controls
reject them rather than resetting their cuts: a new onset has no single
meaning across retained cuts. Silent
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
target for this subset.

### Bundled sting

`:sting` adds Deadpan's own "wrongly triumphant" sting to
the sound catalog: an original brass-like fanfare (three G4 pickups, then a
held C major chord with a slow vibrato and release, 1.8 s, peaking at -3 dBFS)
synthesized deterministically by `deadpan_audio::triumphant_sting_wav`, so no
third-party sound is distributed. The app writes it as a 48 kHz stereo 16-bit
WAV under `Application Support/Deadpan/Sounds/sting-v1/` (a private directory in
replays) and imports it through the ordinary managed `ImportSound` path with an
explicit stereo left/right reading; placement with `,s`, `:sound-at`, gain,
edges and export are then those of any catalog sound. The version directory
names the synthesis recipe, so a changed recipe never replaces an imported
sting. Evidence: `the_sting_is_a_bounded_fanfare_ending_in_silence`, the
`creative-dot` replay (import, catalog row, placement at the Edit cursor, Undo)
and the release `triumphant-sting` preview/export fixture (the chord window's
peak predicted from the synthesized samples, silence before and after).

While a macro records, `,s` commits through the semantic `PlaceSound { asset }`
instruction, which the planner resolves to the same root event (natural-rate
span from the asset record at the cursor's audio boundary, refused when it would
end past the edit); store admission still rechecks the receipt. The
`sound-placement` replay records a placement and runs it at a new cursor.

## Beds and stings through edits

A bed is a sound owned by the edit itself (a root `SoundEvent`); a sting is a
sound attached to a beat (`BeatSound`, `:sound-attach`). They follow different
anchors:

| Edit | Root bed | Beat-attached sting |
| --- | --- | --- |
| Ripple delete (`dd`, Visual `d`, `:delete`) inside or across the bed / of the beat | The deleted part of the bed goes with the picture; the bed continues across the join on its own clock without restarting its source. Deleting all of its support deletes it. | Deleting the beat deletes its sting; Undo restores both. Deleting a range across the beat keeps the surviving part's sting behind a delivery crop. |
| Pause or splice inserted inside | Inserted time is silent in the bed; the rest resumes on its prior phase. | Inserted before or after the beat: the sting moves with the beat. Inside the beat: the beat is split and both fragments keep the sting behind delivery crops. |
| `:splice` move of a range | The bed stays on the edit clock; it does not follow the moved picture. | The sting moves with its beat, inside or out of its group, on its retained clock. |
| `s` Split | Non-root Split leaves the bed unchanged. | Both fragments keep the complete recipe and phase; the mixed PCM is unchanged. This includes a fragment (a Partition left by an earlier pause or Split) that carries the sting itself: see [splitting a sting fragment](#splitting-a-sting-fragment). |
| Group / Ungroup | Unchanged. | Unchanged. Ungroup of a Sequence that owns a sound itself refuses, as do Group and Ungroup inside a sting's retained clock scope. |
| `rr`, `r` + motion, `:repeat N` wrap | The bed is routed through the repeated plays. | The sting sounds in every play; `rib` keeps it on the first play only. |
| `:sound-cut` | Ends at the cursor through the retained route (above). | Not applicable. |
| Trim, Slip, Roll, speed change or Hold length change inside the owner | Trim and Roll before or around the bed transform it. A speed change on a beat in ordinary groups records its added or removed time at the beat's end (`sound_routing::speed_change_operation`): sounds before the beat stay, sounds after it move by the difference, a sound playing over the beat's end is gapped or cut there; inside a Repeat or another speed change it refuses. Hold length changes with root sounds refuse. | Edits before or after the beat transport it; changing timing inside the beat refuses with "this edit changes the timing inside a beat that carries an attached sound". |

Moving a beat that carries a sting proves only the sting's own processing
subtree (the owner, or its enclosing speed change), not the whole top-level
group it started in. Other beats in that group may therefore move around it,
and the sting may leave the group. Each move appends the pre-edit placement to
the sting's clock journal; the retained PCM labels move by the exact
difference of the old and new absolute sample boundaries.

Still refused, readably and atomically: legacy whole-node `WrapRepeat`,
`Move` and `Insert` (historical replay grammar only), a sting owned by the
project root, Ungroup of a sound-owning Sequence, timing changes inside the
sting's beat (Hold length, Trim of that Source, speed change), and a timing edit inside another
beat of the sting's top-level group before the sting has a retained clock
(its first capture covers the whole top-level group; a whole-beat move
narrows it to the sting's own beat). Moving or re-onsetting a routed bed
(`:sound-at`, `h`/`l`) refuses because a new onset has no single meaning
across retained cuts; speed changes and Hold length changes refuse while
root sounds exist.

Evidence (2026-11-10, second pass): the fragment Split and attached-sound
display above; the `sound-placement` replay adds the badge and inspector
entry, `:sound-gain` on the selected attached sound, `s` on the sting's
fragment, `:sound-detach` and three Undos through real keys and pointer;
`attached_sounds_list_change_detach_and_undo_through_the_service` and the
headless `attached_sting_derives_the_native_beat_sound` cover the list and
detach.

Evidence (2026-11-10): core `sting_matrix` tests at 30000/1001 (move inside
and out of a group with exact retained layouts, Split, Group/Ungroup, Repeat
in every play and `rib`, delete with exact Undo, fragment edits, readable
refusals) and `routed_bed_cut_maps_the_cursor_back_through_its_retained_route`;
decoded PCM in `a_moved_sting_beat_plays_its_complete_sound_at_the_new_boundary`
(untreated sample-exact and EQ-treated); release preview/export fixtures
`routed-bed-drop` and `sting-follows-beat`; the headless
`attached_sting_derives_the_native_beat_sound`; and the `sound-placement`
replay's routed `:sound-cut` with Undo plus `:sound-attach`, `rr`, `dd` and
Undo through real keys.

### Splitting a sting fragment

Splitting a fragment normally refines its Partition: the left piece keeps the
fragment's identity with a narrower window and the right piece gets a new
Partition over a copy of the same complete child. A fragment that owns an
attached sound itself is split like any other beat instead (`split::refines`
excludes it): the fragment becomes the complete owner behind two new delivery
Partitions, the right piece's owner is a copy of the fragment and its child,
and both copies keep the complete recipe. Each sounds only inside its own
piece's window, so a sting before the cut plays from the left piece, one after
it from the right, and one spanning the cut plays on across it on its own
clock, half from each piece. The cost is one extra Partition level on that
beat; later Splits of either piece refine those new Partitions as usual.

Each copy's journal gains one retained clock: the pre-split layout, with the
exact Split forest (historical fragment to the two new Partitions, each with
its complete owner). The correspondence proof treats a Partition whose first
piece keeps it as the complete owner as a whole-node Split, not a refinement.
Undo restores the fragment, its single owner and journal exactly. Core
`fragment_split_keeps_the_sting_complete_on_its_retained_clock` checks the
three positions at 30000/1001, the exact journals and a further refining Split;
decoded PCM in `splitting_a_sting_fragment_leaves_its_samples_unchanged`
(untreated sample-exact and EQ-treated) shows the split changes no sample.

### Seeing and detaching attached sounds

A card whose beat carries an audible attached sound shows a `♫ N` badge in its
top-right corner (below an AI badge), spoken as "N attached sounds on beat".
The beat inspector's Attached sounds section lists each sound owned by the
beat or by the complete owner behind its fragment crops, Repeat definition or
speed change (shared `deadpan_cli::sound_events::attached_sounds`): its label,
start within the beat in frames, length in seconds, and "in every play" under
a Repeat, "plays a-b here" when a Split crops it, or "cut away in this beat"
for a copy the window hides. Each row is a button spoken with its position in
the list. Choosing one selects it: `:sound-gain`, `:sound-edges`, treatments
and `:sound-delete` then apply to it (`SetBeatSound`, keeping its source and
placement), while moving, cutting, sends and pause permissions refuse because
its place belongs to the beat. Detach (the row's button or `:sound-detach`)
removes the selected attached sound, or the beat's only one, with
`DeleteBeatSound`; one Undo restores it. Headless: `sound` `detach`.
`:timeline detail` draws each sting in its own colour at its exact Edit frames,
in every Repeat play that uses its owner, scaled by speed changes and clipped
to fragment windows; the lane's spoken summary names each sting and its frames.

## Persisted root sounds

A catalog sound can be repaired or relinked independently of its placed events:
select it in the Original / sounds pane and use `:sound-relink`, then choose
the identical original file. Its registered identity, placements and history
stay intact. See [media recovery](RECOVERY.md#moved-linked-files).

Core schema 29 and database 35 introduced `SoundEvent` recipes keyed by `SoundId`.
The current schemas are core 58 and database 87. `SetSound` creates an event or
updates it under the retained-route rules below; `DeleteSound` removes it. The
shared headless command entrypoint supports preview and atomic commit, followed
by ordinary durable undo/redo. Obsolete unused development packages are refused
without writes under the owner's format-breaking authorization. Frozen test
fixtures retain their original schema and measured source receipts; test-only
adapters preserve their old trims in the complete treatment recipe.

This increment admits at most 64 root-owned events. Each retains a qualified
source span, explicit natural-rate mapping and optional exact audible selection,
a signed 48 kHz offset, the complete bounded treatment recipe (trim from -96 dB
through +24 dB), independent
Automatic/Hard edge choices and an explicit `Reject` overflow policy. The whole
selected interval must fit inside the root. Sound placement never changes
picture duration. A qualification ID alone is insufficient: storage rechecks
the revision's immutable receipt and original binding during the command
transaction, and playback admits actual source bytes through its existing
provider.

The root reader evaluates source phase, allocation and current silent-Hold
policy directly on the absolute RoundEven grid. It does not relabel PointCeil
PCM. Each sound receives its own edges and treatments before f64 summation in SoundId order
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
`MoveRange` supports linked relocation between ordinary Sequence scopes while
preserving the root owner clock. Root sound recipes and existing routes remain
unchanged; final live Hold gates and retained allowances still apply. It adds
no ripple journal. See [atomic moves](ATOMIC_MOVES.md).

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

`DeleteRipple` uses this same one-step root-bus transform; structural
Source/Hold/Repeat/Preserve suffix owners are sampled from their new positions.
It is the native and public CLI deletion path; saved core `Delete` keeps its
historical reduction for exact history replay. No route vocabulary changes.

Core 34/database 43 add one direct `Replace { range, duration }` operation.
`ReplaceSource` maps the old prefix and suffix directly to the final document,
with sound-free inserted time between them. It does not persist Delete followed
by Insert: a temporarily shortened sample extent can drop a rounded terminal
sample that the final replacement should retain. Whole-range replacement also
avoids an empty intermediate map. Logical support, rounded sample support and
cut edges all use the same direct map.

`SetSound` changes label, gain or endpoint policy without discarding the journal.
It may also narrow a routed recipe's selection in place, keeping the same
owner, source, offset and placement (the routed bed drop): the route maps the
unchanged recipe clock, and validation still requires surviving support.
Any other change to a routed recipe, mapping, owner or offset requires `ReplaceSound`, which
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
occurrence edits, nested sound ownership, owner-local Repeat/Retime transformations and
general retained sound-bus captures remain required. Their guards stay closed.

### Descendant sound clocks through Repeat

Sound owners beneath a Repeat now retain their processing clocks through
wrapping, count and gap changes, first-play isolation, copying and recopying.
Each retained clock maps explicit live and historical Repeat identities.
An introduced wrapper records the stable plays that existed at capture;
later plays begin on their own exact sample phase. Dormant final gaps skip
clocks where their sound had no occurrence. Removing and regrowing plays
does not reuse retired identities.

`rib` retains the selected beat's sounds on its first play; `rab` retains
them on every play. Independent sample offsets remain independent of frame
placement. Audio selects the first applicable retained clock for each
concrete occurrence, preserving complete nested Preserve processing context.
The retained-plan cache replaces the previous query's birth cohort and has
an explicit 64-context bound. A suffix-only edit needs no new clock.

This does not yet route a sound owned by the changed Repeat itself, an
ancestor-owned sound spanning the changed subtree, changes inside an
enclosing Retime processing scope, or partial sound-owner selections. Those
cases still refuse atomically. They need an exact owner-local route before
their existing guards can be removed.

Focused verification on 2026-10-06 passed 29 core clock tests, 10 semantic
beat-object tests, 3 cache tests, 11 PCM integration tests and one store
reopen/register/Undo/Redo test. The PCM witnesses use 30000/1001 timing,
an independent 37-sample offset, copied and later-born plays, changed gaps,
cold seeks and nested Preserve context. Logs are under
`/tmp/deadpan-resume-20261006/repeat-*.log`; independent review and the next
full gate remain pending.

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

This follows the ownership distinction between root beds and beat-attached sounds.
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
grid. Original source absence, endpoints, RoomTone and tails are not this voice's policy. Hold introspection retains the issuer's
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
The continuous Original reader remains separate and unchanged.

The [occurrence preparation boundary](OWNED_SOUND_VOICES.md) now resolves one
explicit current owner through its concrete Sequence, Repeat and Retime ancestry.
It builds independent Preserve inputs, retains full history and applies current
Hold gates only at root output. The native authored lifecycle and root-only
sound guards remain unchanged; nested persistence and edit/copy transforms
still need implementation.
The bounded window query also resolves current occurrences automatically and
the PCM adapter sums their independent processing histories. Exact visible
geometry is separate from Preserve influence, which can survive an outer crop.
This remains preparation work; it does not attach sounds to beats or place them
in the authored bus.

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
The broader projected APIs remain preparation interfaces.

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
Preserve witnesses, full retained history through split/repeat, cold and
shuffled reads, changed source fingerprints, aggregate limits, and summation
before one shared limiter. Every source, recipe and processed result remains tied
to its immutable project revision.
