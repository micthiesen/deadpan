# Headless editing foundation

The standalone CLI and `deadpan-app --headless` use the same implementation.
Neither headless entrypoint initializes a window. Source registration opens
bounded media sessions to qualify the selected streams. This is the engineering
API, not the finished editor's user interface.

```sh
cargo run --locked -p deadpan-cli -- project create /tmp/example.deadpan --fps 30000/1001 --size 1920x1080
cargo run --locked -p deadpan-cli -- project validate /tmp/example.deadpan
cargo run --locked -p deadpan-app -- --headless project dump /tmp/example.deadpan --json
```

Creation requires a new `.deadpan` directory path. Without `--fps` and `--size`,
the basis is provisional 1920×1080 at 30 fps; a first primary picture insertion
can choose it before timed edits. Supplying both options establishes an explicit
fixed basis. Sources, analysis, and renderers cannot be inferred from this blank
project. The source package's `project.sqlite` is authoritative; `manifest.json`
contains discovery identity only. No loose JSON document is read as current state.

## Commands and dry runs

Copy the actual `project_id`, `revision_id`, and root node ID from the dump into
a request such as this. `new_revision` is optional; the host generates a UUID
when omitted. Caller-supplied revision IDs must never have been committed or
reserved by a compound step before.

```json
{
  "protocol": 1,
  "project_id": "PROJECT_ID_FROM_DUMP",
  "expected_revision": "REVISION_ID_FROM_DUMP",
  "command": {
    "command": "insert",
    "parent": "ROOT_NODE_ID_FROM_DUMP",
    "index": 0,
    "subtree": {
      "root": "pause-1",
      "nodes": {
        "pause-1": {
          "label": "The pause",
          "kind": {
            "type": "hold",
            "recipe": {
              "duration": 45,
              "video": {"type": "background"},
              "audio": {"type": "silence"}
            }
          }
        }
      }
    }
  }
}
```

Save the request outside the project database, then run:

```sh
cargo run --locked -p deadpan-cli -- command /tmp/example.deadpan --json /tmp/request.json --dry-run
cargo run --locked -p deadpan-cli -- command /tmp/example.deadpan --json /tmp/request.json
```

A dry run returns the validated forward/inverse changes, affected node IDs, and
duration delta with `committed: false`. It does not allocate a persistent revision
or change undo history. It checks the same reducer, serialized size limits, and
never-reused revision rule as commit. A stale expected
revision fails with `RevisionConflict` and the current revision, without writing.

Supported commands are `insert`, `insert_time`, `split`, `slip_source`, `trim_source`, `roll_sources`, `apply_source_trim`, `delete`, `delete_ripple`, `delete_range`, `delete_children`, `move`, `move_range`, `group`, `group_selection`,
`ungroup`, `splice_source`, `splice_source_at`, `replace_source`, `splice_slice`, `splice_slice_at`, `replace_slice`, `wrap_repeat`, `set_repeat`, `wrap_retime`, `set_retime`, `insert_plays`, `move_plays`, `set_hold_duration`, `set_hold_provider`, `set_hold_picture_context`, `set_source_audio_mapping`, `set_source_video_mapping`,
`rename`, `set_audio_edge`, `set_audio_treatments`, `set_hold_audio`, `set_framing`, `set_sound`, `replace_sound`, `delete_sound`, `set_sound_allowance`, `add_asset`, `set_canvas`, `set_mark`, `delete_mark`, `set_play_override`, `clear_play_override`, `set_gap_override`, `clear_gap_override`, `isolate_gap`, and `edit_occurrence`. Their exact typed parameters are defined in
[`Command`](../crates/deadpan-core/src/command.rs). `set_repeat` changes an existing
Repeat; `wrap_repeat` deliberately adds nesting. A three-play repeat includes
three total plays and only two gaps. These are structural edits, not rendered
media. [Named grouping](GROUP_EDITING.md) adds `group_selection` for exact child
or half-open range grouping, with preflighted Split identities and retained owner
clocks. [Exact sibling selections](STRUCTURAL_SELECTIONS.md) also retain empty
endpoint children through capture, grouping, repeat and `delete_children`.
The dedicated [Macro commands](SEMANTIC_MACROS.md#headless-inspection-save-and-run)
inspect, save and run bounded motions, frame cuts, beat copies, register pastes
and named calls. Broader
range/text selectors, register management and effects remain required work.

`compound` executes a bounded `ResolvedTransaction` through the same preview
and commit boundary. Its flat Edit/Yank/Cut/Paste steps carry exact resolved
targets and fresh leaf allocation revisions. The transaction captures the bank
version and frozen register inputs; preview checks them without writes. A
successful authored compound saves one history entry and returns its prepared
register bank. Historical copies can refer to retained intermediate snapshots.
See [compound transactions](COMPOUND_TRANSACTIONS.md) for identity, admission,
recovery and size rules. This command does not parse semantic macros. Bank-only
execution uses the dedicated store API; the generic headless edit command
requires an authored leaf.

`macro inspect <package> [--register a]` reads register types and Macro bodies
with one consistent document revision and bank version. `macro <package>
--json <request> [--dry-run]` saves or runs a named Macro. Requests bind both
versions and supply an explicit Sequence parent and Edit cursor for execution.
The selected direct child is separate from that cursor; absence remains explicit.
Dry-run uses complete store admission; a counted authored run saves one Undo.
The same commands use the authenticated native owner when the project is open.
See the [request examples and receipt contract](SEMANTIC_MACROS.md#headless-inspection-save-and-run).

An `apply` operation plans and commits one inline program once, the same path
as a native single recorded action, without saving it as a Macro. Use it for
the GUI edits that resolve through semantic instructions, such as gags
(`gag`, `set_gag`), pauses (`insert_pause`), `bleep`, `lift`, `tail`,
captions, cutaways, framing presets and `set_audio` gain steps:

```json
{"type": "apply", "program": {"instructions": [{"type": "repeat", "selector": {"type": "visual_selection"}, "plays": 3}]},
 "parent": "SEQUENCE_NODE_ID", "cursor": 4, "selected_child": null,
 "visual_selection": {"type": "time", "anchor": 18, "head": 4, "extending": true}}
```

It binds the same revision and bank version, supports `--dry-run`, saves one
Undo step and reports `operation: "apply"`, the trace and the edit summary.
Register writes come only from the program's own yanks and cuts. `macro
inspect` also returns an edited register's `outline`, which `:recipe-inspect`
shows. [Parity](PARITY.md) maps every GUI action to its headless form.

`delete` takes `node` and resolves to `DeleteRipple` before preview or dispatch,
with timing allocation equal to the new revision and ordinal zero. Explicit
`delete_ripple` takes `node` and `timing`. Both retain the old sample entry of
downstream audio through ordinary Sequence ancestors. Empty and terminal
children are supported; Repeat/Retime ancestors require separate occurrence
work. Saved core `Delete` requests retain their historical replay semantics.
See [ripple deletion](AUDIO_REANCHORS.md#ripple-deletion).

`delete_range` takes `parent`, a global nonempty half-open `range`, fresh Split
`identities`, and `timing`. Both endpoint cuts and removal form one transaction.
`ProjectDocument::range_deletion` returns the required Split identity count.
The timing allocation equals the new revision. An interior deletion with a
surviving suffix uses two consecutive ordinals for the original and split suffix
clocks; operations needing only one clock use the supplied ordinal. The same
command supports dry-run, commit and durable Undo/Redo.
See [selected ranges](AUDIO_REANCHORS.md#selected-ranges).

`move_range` takes the current `source_revision`, `source_parent`, a nonempty
global `range`, `destination`, fresh Split `identities`, and `timing` allocated
under the new revision. A destination has `type: "seam"`, `parent` and original
child `index`, or `type: "interior"`, `parent`, direct-child `target` and strict
local frame offset `at`. Both addresses use the same pre-edit revision. The
read-only `ProjectDocument::range_move` query reports the joint Split budget,
exact final interval, removal join and timing slots. A copied historical range
cannot authorize removing current content after edits or Undo. Root-owned sound
recipes/routes remain in their unchanged root clock. The older whole-node
`move` retains its index-after-removal meaning. See [atomic moves](ATOMIC_MOVES.md).

`wrap_retime` takes `node`, a fresh `id`, a positive frame `duration`, and
`pitch` (`preserve` or `follow_speed`). `set_retime` takes `node`, `duration`
and `pitch`, retaining the ordinary Retime's child/input range. Both also have
occurrence forms. The native `:retime` speed grammar resolves to these same
commands. See [speed editing](RETIME_EDITING.md) for output-binding lifecycle.

Core schema 34/database 43 introduced direct sound replacement maps. `set_sound` and `replace_sound`
take an `id` and complete `event`; `delete_sound` takes its `id`.
Events require a qualified source, root owner,
natural-rate mapping, contained selection plus sample offset, explicit gain,
edge choices and `overflow: "reject"`. They feed the shared limited bus without
adding picture time. `insert_time`, `splice_source`, `splice_source_at` and supported Delete edits
through ordinary Sequence ancestors retain chronological root `sound_routes`;
non-root Split leaves the sound bus unchanged. `set_sound` preserves an existing
route when changing label, gain or endpoint policy. Changing a routed recipe,
mapping, owner or offset requires `replace_sound`, the explicit reversible reset
of its retained routing. Root Split, temporal occurrence edits and Repeat/Retime
sound transforms remain guarded. See [root sound events](SOUND_EVENTS.md#persisted-root-ripple-edits).

### Node gain and mute

`set_audio_treatments` replaces one node's complete bounded treatment recipe.
The occurrence form uses `edit: { "type": "set_audio_treatments", "treatments": ... }`
and isolates the selected play atomically. Preserve existing envelopes and mute
ranges when constructing a trim adjustment. This command does not change timing,
raw-audio lineage, source admission or placed sound recipes.

```json
{
  "command": "set_audio_treatments",
  "node": "NODE_ID_FROM_DUMP",
  "treatments": {
    "order": ["clip_gain"],
    "clip_gain": {
      "trim": -6000,
      "muted": false,
      "envelopes": [],
      "mute_ranges": []
    }
  }
}
```

Trim is integer millidecibels. Zero remains an explicitly configured unity stage;
`muted: true` is distinct from finite attenuation and Hold silence policy. Clear
the complete stage with `treatments: { "order": [], "clip_gain": null }`.
See [gain recipes and owner clocks](AUDIO_GAIN.md) for envelopes and bounds.
`inspect-audio <package> --samples <START> <END> --authored-bus` returns at most
256 canonical samples after node/root-sound gain and before limiting. Existing
raw, time-mapped and edge-only inspection stages retain their meanings.

### Hold audio policy

`set_hold_audio` changes an existing Hold's `audio` without changing its picture,
duration or retained timing. The selected source must already have measured
qualification in the expected revision. For example, a 48 kHz original range:

```json
{
  "command": "set_hold_audio",
  "node": "hold-id",
  "audio": {
    "type": "room_tone",
    "source": {
      "asset": "original",
      "span": {
        "start": { "ticks": 96000, "time_base": { "numerator": 1, "denominator": 48000 } },
        "end": { "ticks": 108000, "time_base": { "numerator": 1, "denominator": 48000 } }
      }
    }
  }
}
```

Use `audio: { "type": "silence" }` for digital silence. `edit_occurrence` accepts
the same operation as `edit: { "type": "set_hold_audio", "audio": ... }` with
the normal explicit instance and identity pool. Obsolete permissions belonging
to a Hold changed away from Silence are removed atomically; Undo restores them.
The command's existing Tail vocabulary does not imply implemented tail DSP.
See [room-tone authoring](ROOM_TONE_AUDIO.md#authored-policy-changes).

### Sound permissions

`set_sound_allowance` takes `sound`, a concrete `issuer`, and boolean `allowed`.
It grants or revokes one root contribution's permission through one current
silent Hold or default Repeat gap, in the same reversible command transaction:

```json
{
  "command": "set_sound_allowance",
  "sound": "impact",
  "issuer": {
    "type": "node",
    "instance": { "node": "hold-id", "repeats": [] }
  },
  "allowed": true
}
```

For a default Repeat gap, `issuer.type` is `repeat_gap`, `instance` identifies
the Repeat and its complete enclosing occurrence path, and `gap_after` identifies
the preceding stable play as `{ "allocation": "repeat-revision", "ordinal": 0 }`.
Node issuers likewise require every repeated ancestor; definition wildcards and
stale issuers are rejected. Allowance-only edits recheck source admission.
Direct and routed sounds prepare complete input before applying current
per-contribution Hold gates and authored edges. Allowances preserve the Original's
silence, other sounds' policy, source exhaustion and empty routing gaps.
The native `:sound-allow` and `:sound-silence` commands capture the sound, session,
revision, Edit frame and issuer on entry. Nested owners, send/tail allowances,
the full voice graph and remaining structural sound transforms stay open. See
[root allowances](SOUND_EVENTS.md#persisted-root-sound-allowances).

Sparse [gap branches](REPEAT_GAP_BRANCHES.md) retain an
independent subtree after a stable Repeat play. `isolate_gap` copies the current
rendered default into a Hold while preserving its audio clock; it requires fresh
node and timing identities. `set_gap_override` replaces a gap with a supplied
subtree, while `clear_gap_override` exposes the current default. Each has an
occurrence form. A final play retains its branch without rendering a trailing
gap. These commands do not yet plan insertion at an arbitrary cursor position.

`insert_time` admits existing root Sequence seams before composite suffixes,
including partially selected Repeats and owned gap branches. The new Hold,
current placement reanchors, mark transforms and history commit are atomic.
Root Source/ordinary Hold interiors and supported transparent fragments also
admit composite suffixes. This branch reserves the supplied timing ordinal and
its checked successor for pre-Split sampling and post-Split placement. These
cuts also descend into unretimed Sequence groups and keep their actual parent
live. Repeat/Retime interiors remain unsupported. See [pause insertion](INSERT_TIME.md).

`splice_source` accepts `parent`, `index`, `source`, `id`, `label` and `timing`
(`allocation` equal to the new revision and an ordinal). It inserts the supplied
Source at an explicit ordinary Sequence child slot and preserves each shifted
physical audio entry. The newly inserted Source starts on the canonical unbound
project grid. Repeat/Retime ancestors are rejected. This generic command can
reuse an existing asset offline; the native host instead derives the Source
through prepared-receipt admission. See [Original moments](SOURCE_MOMENTS.md).

`splice_source_at` replaces the slot with an explicit direct `target` child and
strict local `at` frame duration, and also requires `identities` for Split. The
named `parent` must be an ordinary Sequence; supported targets are Source,
ordinary Hold and transparent fragments. It splits and inserts in one reversible
command, retaining the original audio clocks and transforming placed sounds
once. `timing.ordinal` and its checked successor cover pre-Split sampling and
post-Split placement. It never descends a child group or resolves a Repeat/Retime
occurrence. Native [Place slice](SLICE_PLACEMENT.md) previews the complete command
through exact prepared Original evidence before committing.

`replace_source` takes `parent`, a nonempty global Edit `range`, `source`, `id`,
`label`, pooled Split `identities` and `timing`. It replaces the complete range
in one reversible command. Endpoint interiors use the same physical admission
as `splice_source_at`; whole intervening children may be composite structures.
Zero-duration children at either endpoint survive; those strictly inside the
range are removed. Both retained fragments keep their context. Suffix audio
maps directly from its old entry to its final placement, and root sounds gain
one Replace operation without a shorter intermediate clock. Native preview
and fast Visual paste derive the Source from qualified Original evidence.

Optional per-node [framing](FRAMING.md) retains a static
pose or whole-owner envelope; `set_framing` sets it, and `framing: null` removes it.
`inspect-plan --frame N` includes exact evaluated provider-to-root framing scopes.
Hold recipes optionally retain a [captured view](CAPTURED_FRAMING.md) in
`picture_context`. `set_hold_picture_context` takes a Hold `node` and optional
`context`; `null` explicitly clears the retained composition. Ordinary Camera
reset changes only the live framing, preserving that composition. The plan
includes captured context separately from live scopes. Provider and duration
changes preserve it, including generated acceptance and fallback restoration.
`set_source_audio_mapping` also accepts `selected_placement` with full-span
`start`/`frames` and a contained exact `selection: {start, end}`. It crops audible
support without changing the original sample span or mapping phase. See
[Original moments](SOURCE_MOMENTS.md). Native range reuse derives this mapping from the prepared Original receipt.
Retime `purpose` defaults to ordinary `edit` and is
omitted from canonical JSON. `partition` retains child audio context at unity
speed and requires automatic edges; see [the partition contract](AUDIO_PARTITIONS.md).
`split` retains editable child context through transparent partitions.
`insert_time` atomically inserts a positive Background or Freeze Hold into the
supported root sequence and preserves downstream audio clocks. Its current scope,
identity parameters, zero-duration refusal and migration contract are documented
in [Insert Time](INSERT_TIME.md). Arbitrary nested insertion remains open.
Marks may retain multiple physical bindings under one logical ID. Named queries
return every matching binding ordinal, require explicit ambiguous occurrence
scope and reject distinct exact positions with `MarkAmbiguous`. See
[mark fragments](MARK_FRAGMENTS.md) for ownership, limits and query metadata.
[Audio edge choices](AUDIO_EDGES.md) default to automatic
when the JSON object is absent and are editable through direct or isolated
occurrence commands. Explicit policy objects retain all six typed fields.
[Source audio mappings](SOURCE_AUDIO_MAPPING.md) and
[picture mappings](SOURCE_VIDEO_MAPPING.md) independently choose the beat duration
or an exact stream duration and start with `placement`.
[Measured import timing](SOURCE_IMPORT_TIMING.md) describes the candidate policy.
Picture endpoint policy is persisted and enforced
against the chosen trim. Repeat `iterations.runs` store
`allocation`, `first`, and `count`. Commands `wrap_repeat` and `set_repeat` still
take a total `plays` count. `insert_plays` takes `node`, `index`, and `count`;
`move_plays` takes `node`, a half-open `start`/`end` play range, and `destination`
measured after removal. Surviving identities remain stable; inserted/grown plays
use the command's new revision. The run limit is 100,000 per Repeat, checked
without expanding plays. `InstancePath` identifies the target node and its exact
ordered Repeat ancestors. Sparse play overrides are described below.
Inserting a subtree remaps its Repeat plays to the actual insertion revision,
preserving their count. An imported initial document reserves its existing
allocation names, so later revisions cannot resurrect retired identities.

## Edits the app derives from its focus

These commands take, as explicit input, the context the app reads from its
focus, and derive the same request through the same shared code. Each derives
against a read-only snapshot at `expected_revision` and commits through the
ordinary revision-checked path, so a request computed against an older head is
refused. Each works on a closed project and, through the
[live endpoint](LIVE_PROJECT.md), on one the app has open. See
[parity](PARITY.md).

### Placed sounds

```sh
cargo run --locked -p deadpan-cli -- sound /tmp/example.deadpan --json /tmp/sound.json [--dry-run]
cargo run --locked -p deadpan-cli -- sound --write-sting /tmp/sting.wav
```

```json
{"protocol": 1, "expected_revision": "REVISION",
 "edit": {"type": "place", "asset": "ASSET", "at": {"frame": 120}}}
```

`edit` is one of `place` (`asset`, `at`, optional new `id`), `move` (`id`,
`at`), `nudge` (`id`, `frames`), `set` (`id` and any of `gain_millidecibels`,
`gain_step_millidecibels`, `edges`: `automatic` or `hard`), `cut` (`id`, Edit
frame `at`), `allowance` (`id`, Edit frame `at`, `allowed`) and `delete`
(`id`). `at` is `{"frame": N}`, the Edit frame boundary `,s` uses at the
cursor, or `{"sample": N}`, the exact 48 kHz onset of `:sound-at`. The event
comes from `deadpan_cli::sound_events`, the same functions the app's `,s`,
`:sound-*`, `h`/`l` and `:sound-cut` call: the complete measured catalog span
at natural rate, translated or cut in its exact frame clock, refused when it
would leave Your edit or when the sound follows retained timeline cuts
(`SoundEditRefused`). The result is the `command` output plus `sound_id`.
`--write-sting` writes the bytes `:sting` synthesizes (never replacing a
different file); `project retain-original` and `register-source` with
`audio_only` streams then add it to the catalog.

### Original copies and whole-Original reuse

A Macro request with `"operation": {"type": "yank_original", "register": "a",
"ordinals": {"start": S, "end": E}}` copies the half-open picture ordinals
`[S, E)` of the project's ready Original (or an explicit registered `asset`)
into the unnamed register and `a`, exactly as `y` in Original. It names the
expected revision and register bank version, writes only the bank, and returns
`committed_registers`; the store validates the range against the qualified
index. A later `paste`, `replace_selection`, `set_room_tone` or `set_cutaway`
instruction uses it as it would a native copy.

```sh
cargo run --locked -p deadpan-cli -- project insert-original /tmp/example.deadpan --parent NODE --index 1 --expected REVISION [--dry-run]
```

`insert-original` is `,i`: it derives the app's registration of the whole
Original (same content, asset, label and qualified streams, a fresh Source
node at `index` of `parent`) and admits it through `register-source`'s path,
which reuses the existing qualification and asset.

### Gag inspection

`gag-inspect <project.deadpan> --json <recipe.json> [--visual]` lists the step
lines `:gag-inspect` shows for an explicit `gag` instruction recipe at the
project's frame rate, with the expanded instructions. `--visual` expands it
for a Visual range. Nothing is written.

## Source Slip

`slip_source` takes an explicit ordinary Sequence `parent`, its direct child
`node`, and signed `delta_frames`. Positive moves select later Original material
without changing output position or duration. The target must be an admitted
Source or one neutral unity Partition with full qualified context and an exact
editorial window. See [Source Slip](SOURCE_SLIP.md) for its current scope.

On `command --dry-run`, `source_slip` reports requested/applied deltas, exact and
whole-frame handle bounds, and the limiting picture boundary. The sibling `edit`
is null for an applied zero. Preview writes no history or revision reservation;
normal commit rechecks the captured revision and receipt. Raw zero commits return
`InvalidCommand`. This response is shared with the live project writer and
`deadpan-app --headless`.

## Source edge Trim

`trim_source` takes an ordinary Sequence `parent`, its direct child `node`,
`edge: "in" | "out"`, signed `delta_frames`, `mode: "ripple"`, a nullable
`wrapper`, and a `timing` identity allocated in `new_revision`. Positive moves
the named edge later in Original material. A contraction shortens the output;
an extension grows it and shifts the suffix. The admitted Source/Partition scope
and exact bounds are described in [Source Trim](SOURCE_TRIM.md).

`command --dry-run` returns a `source_trim` resolution alongside nullable `edit`.
The resolution reports exact inclusive/exclusive bounds, inward whole-frame
clamps, old/new allocation and selection, physical prefix, duration change and
whether a fresh Partition wrapper is needed. Supply that wrapper exactly when
`needs_wrapper` is true; zero previews require null. A dry run must carry valid
revision, timing and wrapper metadata even when it resolves to zero. It reserves
no identity or history. Commit rechecks the captured request and stored receipt;
a raw zero commit fails `InvalidCommand`. The live writer and
`deadpan-app --headless` share this response and one atomic history change.
Scalar `trim_source` supports Ripple. Use the combined command below for
Overwrite.

## Adjacent Source Roll

`roll_sources` takes an ordinary Sequence `parent`, literally adjacent `left`
and `right` children, signed `delta_frames`, nullable `left_wrapper` and
`right_wrapper`, and a `timing` identity allocated in `new_revision`. A positive
delta moves their shared boundary later without changing their combined duration.
See [Source Roll](SOURCE_ROLL.md) for exact bounds and current target admission.

`command --dry-run` returns `source_roll` plus nullable `edit`. Both sides share
one applied delta after intersecting their exact handle and minimum-duration
limits. The resolution reports the controlling side, exact bound, fixed pair
interval, seam before/after and both Source/allocation transitions. At most one
side needs a fresh wrapper. Supply each wrapper exactly when its side reports
`needs_wrapper`; zero requires both null. Preview validates both receipts and
all revision/timing metadata without reserving identities. Raw zero refuses.
Cold and live writers use the same response and atomic commit path.

## Combined Source Trim

`apply_source_trim` accepts `parent`, `node`, nullable captured `right`, an
`intent` with `in_frames`, `out_frames`, `slip_frames`, `roll_frames` and
`policy` (`ripple` or `overwrite`), and an exact `resources` pool. All four
values refer to the same entry revision. They are accepted values, so this
command does not independently clamp or save intermediate scalar commands.
See [Combined Trim](COMBINED_TRIM.md) for output geometry and current verification.

Resources contain nullable `target_wrapper` and `right_wrapper`,
`split: {"nodes": [...]}`, `fillers: [...]` and nullable `timing`. Resolve their
exact roles through `ProjectDocument::source_trim_edit`; provide only the
required fresh identities. A timing allocation, when required, names
`new_revision`. The resolver's report is descriptive and cannot bypass
command validation or stored Source admission.

Dry runs return `source_trim_edit` and nullable `edit`. Zero intent requires
empty resources, validates A and the unused result revision, and returns
`edit: null` without reserving anything. Raw zero authoring is refused. Both
used Sources are admitted even when overwrite removes B; an unused diagnostic
neighbor does not become a required media source. Standalone and owned-project
dispatch share these checks and save one atomic history entry.

## Sparse play overrides

`set_play_override` replaces one play of an authored Repeat with an ordinary
editable subtree. It takes `node`, the stable `iteration` from the document, and
`subtree` using the same shape as `insert`. For example, the command inside a
current protocol/revision envelope can be:

```json
{
  "command": "set_play_override",
  "node": "repeat-1",
  "iteration": {"allocation": "REVISION_THAT_CREATED_THE_PLAY", "ordinal": 1},
  "subtree": {
    "root": "different-pause",
    "nodes": {
      "different-pause": {
        "label": "Only this play",
        "kind": {
          "type": "hold",
          "recipe": {
            "duration": 60,
            "video": {"type": "background"},
            "audio": {"type": "silence"}
          }
        }
      }
    },
    "overrides": {}
  }
}
```

The document stores `overrides[repeat_id]` as an array of `{iteration, root}`
entries. Each root has one structural owner. Its duration may differ from the
default child; later plays move by the exact difference, with gaps only between
plays. Nested overrides are supported through `subtree.overrides`. Insertion
reallocates nested Repeat identities and remaps those override keys by play
position. Other plays retain the default child and their original identities.

`clear_play_override` takes `node` and `iteration`, removes the owned subtree,
and restores that play's default child. It fails if no override exists. Edit an
existing override's nodes with the ordinary commands; replacing its entire
subtree requires fresh node IDs. Both commands support dry-run and durable
undo/redo. Reordering preserves overrides by identity. Shrinking retires removed
overrides into undo history; newly inserted or grown plays inherit none.

Occurrence paths must target the effective child of their selected play. A path
to the default child is invalid for an overridden play, and an override child
cannot be addressed through another play. Marks attached to replaced or removed
content follow their declared loss policy. Gaps follow their preceding stable
play even when its duration changes.

This command targets an authored Repeat and one of its plays. If that Repeat
itself occurs within another Repeat, the authored change applies in each outer
occurrence using it. Use `edit_occurrence` below to edit through one complete
nested occurrence path. Partial range operators and role-only overlays remain
required work.

## Edits to one nested occurrence

`edit_occurrence` resolves a complete `instance` against the expected revision,
isolates its repeated ancestors, and applies one node operation as a single
undoable edit. For each ancestor without an override for that play, it copies the
default child's authored subtree and installs an override for only that play.
Existing overrides are reused. Other plays keep their authored content. Nested
copies include existing override subtrees and retain compact iteration orders
under fresh authored node IDs, without expanding play counts or copying media.

For example, in a tree `outer -> inner -> hold`, where both Repeats have two
plays, this command changes only the second inner play of the second outer play:

```json
{
  "command": "edit_occurrence",
  "instance": {
    "node": "hold",
    "repeats": [
      {"node": "outer", "iteration": {"allocation": "OUTER_ALLOCATION", "ordinal": 1}},
      {"node": "inner", "iteration": {"allocation": "INNER_ALLOCATION", "ordinal": 1}}
    ]
  },
  "edit": {"type": "set_hold_duration", "duration": 60},
  "identities": {
    "nodes": ["copied-inner", "copied-default-hold", "selected-hold"],
    "marks": []
  }
}
```

Use the real allocation values from the document. The host supplies a bounded
pool of fresh, distinct `nodes` and `marks`; unused identities have no effect.
Node IDs are consumed in structural preorder for each copy, outside inward.
Mark IDs are consumed in current mark-ID order for each copy. The example needs
three node IDs and assumes no owned marks need copying. A subsequent edit through
the resulting override path needs no new IDs when all its ancestors are isolated.
Dry-run returns the same nodes, overrides, and mark changes as a commit with the
same request and revision. An invalid path, stale revision, exhausted identity
pool, invalid operation, or document-limit overflow commits nothing.

Supported `edit.type` values are `insert`, `delete`, `group`, `ungroup`,
`wrap_repeat`, `set_repeat`, `insert_plays`, `move_plays`, `set_hold_duration`,
`set_hold_provider`, `rename`, `set_play_override`, and `clear_play_override`.
They use the selected node as their implicit `node` or `parent`; other parameters
match the corresponding ordinary command. Child-index and play-ID parameters
remain local to that node. The ordinary operation's structural preconditions
still apply. This entrypoint does not yet implement range deletion/splitting,
multi-target moves, text selectors, or role-only operations.

Isolation preserves local time before the edit, including exact fractions under
Retime. Existing ancestor-local marks therefore follow the selected copied
content when the actual edit transforms them. Owned Local/Source marks copy with
fresh mark IDs; their external hosts and source clocks stay explicit. Concrete
occurrence marks follow their selected copies while retaining their mark IDs.
Sequence-pinned marks remain single events at fixed project coordinates.
Unresolved marks remain unresolved and retain their last coordinates.
Inheritance follows the owner: an externally owned Local mark referencing a
copied host remains on its original authored host. It is not implicitly copied.
See [nested occurrence verification](OCCURRENCE_VERIFICATION.md).

## Explode, duplicate and several plays

`explode` converts one Repeat into an ordinary Sequence of independent plays;
`duplicate` copies an ordinary Sequence child, sibling span or range after
itself; `edit_scoped_many` applies one value per explicit scoped target, such
as one node in plays 2 and 3 only. Hosts size the identity pools with the
read-only core queries `explode_requirements`, `duplicate_requirements` and
`scoped_many_requirements`; `timing.allocation` names the new revision.

For three plays of a one-Hold definition with a gap, nodes are consumed per
play: the first gap, the second play's copy and its gap, then the third copy.

```json
{"command": "explode", "node": "repeat",
 "identities": {"nodes": ["gap-1", "copy-2", "gap-2", "copy-3"], "marks": []},
 "timing": {"allocation": "NEW_REVISION", "ordinal": 0}}
```

```json
{"command": "duplicate", "parent": "root",
 "selection": {"type": "child", "node": "word"},
 "identities": {"authored": {"nodes": ["copy", "copy-word"], "marks": []}, "aliases": []},
 "timing": {"allocation": "NEW_REVISION", "ordinal": 0}}
```

Both are ordinary revision-bound edits with dry-run, one history entry and an
exact Undo. See [explode and duplicate](EXPLODE_DUPLICATE.md) for the full
contract, including attachment and retained-clock rules.

## Source-stage audio inspection

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256
```

This read-only command decodes qualified originals and returns 1 through 256
stereo samples under protocol 1's `audio` key. It pins the opened revision and
labels the result `source_pcm_before_effects`. This is source PCM before edge
fades, effects and mastering, not the final mix or an export file. Natural-rate
source placement, silent Holds, structural repeats and explicit tape-speed
retimes are supported. Unsupported pitch/effect policies and ambiguous speaker
layouts fail explicitly. The same command runs through `deadpan-app --headless`.
See [the contract and failure codes](SOURCE_STAGE_AUDIO.md).

Add `--time-mapped` after the sample endpoints to prepare continuous Preserve
stages as well. The result identifies `time_mapped_pcm_before_effects` and
retains silent-Hold suppression ranges. Nested retimes preserve order and full
intrinsic history; admission and shared work limits fail explicitly. This still
precedes fades, effects and mastering. Authored room-tone Holds and repeat gaps
loop their explicit selected source with short exact crossfades. See
[stage preparation](AUDIO_STAGE_PREPARATION.md) and [room tone](ROOM_TONE_AUDIO.md).

Use `--edge-faded` instead to apply authored automatic/hard choices after time
mapping. It returns `edge_faded_pcm_before_voice_effects` with the engine and
fixed implemented stage order. Fades remain inside each full allocation and
shorten for tiny fragments; read chunks cannot create new fades. See
[audio edges](AUDIO_EDGES.md). This inspection stage does not apply limiting.

Use `--limited` for the shared limited audition bus. It returns
`limited_edge_faded_pcm`, its exact implemented processing order, linked stereo
gain and the complete canonical tiles whose reconstruction anchors were verified.
The output remains limited to 256 requested samples, with real adjacent context
prepared internally. Cache hits re-admit all source dependencies. Gain is
informational; this command never changes the project. Voice effects, sends,
the full group mix and encoded-output qualification remain open. See
[mastering](AUDIO_MASTERING.md).

## Physical audio context inspection

```sh
cargo run --locked -p deadpan-cli -- inspect-audio-domain /tmp/example.deadpan --at 0 --samples -1600 -1344
```

`--at` selects an allocated root sample and its physical Source, Hold/gap or
Preserve domain. The signed sample interval can read that domain's hidden context
outside its visible Partition, including before root zero. The request must fit
its meaningful support and contain 1..256 samples. Returned protocol-1 `audio`
identifies `physical_domain_pcm_before_effects`, the captured occurrence and both
meaningful and visible sample ranges. It uses qualified historical media and
does not mutate the project. See [physical domains](AUDIO_PHYSICAL_DOMAINS.md)
for exact phase, policy and transfer semantics. This is context inspection,
not a final mix, export or an authored resume binding.

## Audio definition inspection

Authored definition output is separately available with
`inspect-audio-definition <project.deadpan> --repeat-default <ID> --samples <START> <END>`
or `--node <ID>` or `--repeat-gap <ID>`. It reads 1..256 nonnegative samples on
the selected definition's local-zero point grid, including a Repeat default
that no current play uses or a configured gap in a one-play Repeat.
Protocol 1 returns `definition_output_pcm_before_effects`. This is recipe
inspection, not timeline allocation or a new-play edit. See
[audio definitions](AUDIO_DEFINITIONS.md) for scope, exact grids and admission.
Append `--revision <ID>` to inspect an exact committed historical revision without
moving the cursor. The selected revision supplies aliases and source receipts,
so deleting the current Repeat does not change its historical definition.

`inspect-audio-placement <project.deadpan> --node <ID> --clock <clock.json> --samples <START> <END>`
evaluates a physical definition's current recipe on an explicit signed root grid.
`--repeat-default`, `--repeat-gap` and trailing `--revision <ID>` are also supported. The bounded
clock JSON contains exact origin, scale and local support. The result includes
the selector and placement, and uses current or requested historical source
receipts without changing history. See [owned clocks](OWNED_AUDIO_CLOCKS.md) for
the complete format, supported roots and errors. This is inspection, not an
authored placement command.

## Picture and audio plan inspection

```sh
cargo run --locked -p deadpan-cli -- inspect-plan /tmp/example.deadpan
cargo run --locked -p deadpan-cli -- inspect-plan /tmp/example.deadpan --frame 10
cargo run --locked -p deadpan-cli -- inspect-plan /tmp/example.deadpan --audio-samples 0 48000
```

The audio command resolves an explicit half-open range of 48 kHz mix samples.
It reports exact source placement, structural silence/Hold policies, retained
Retime pitch stages, stable occurrences, and bounded lookup work. It performs
no audio rendering. See [audio planning](AUDIO_PLAN.md) for quantization, query
budgets and the distinction between planned recipes and implemented DSP.

The first command reports the immutable revision, authored node durations, and
index storage. The second resolves one project-frame center through Sequences,
Repeat plays/gaps, and nested Retimes. It returns an original media coordinate
or the authored Hold/blank/still provider, plus stable occurrence identity and
lookup work counters. Fractions use decimal numerator/denominator strings so
wide exact values survive JSON clients. Out-of-range frames fail explicitly.
These commands work read-only alongside a writer and do not decode media,
evaluate effects, mix audio, or render a file.

## Exact boundary selection

`locate-boundary <project.deadpan> --json <boundary.json>` maps an exact project
position into the nested structural scopes occupying that boundary. It reads the
current immutable revision and works while a writer holds the package. For example:

```json
{
  "protocol": 1,
  "request": {
    "project_id": "PROJECT_ID_FROM_DUMP",
    "expected_revision": "REVISION_ID_FROM_DUMP",
    "position": {"numerator": "33", "denominator": "4"},
    "bias": "right"
  }
}
```

The response is `{ "protocol": 1, "location": ... }`. The location repeats the
project/revision identity, exact position and bias, then returns ordered `scopes`
from the project root inward. Each scope contains an `instance`, exact local
`position`, integer authored `duration`, and an `entry` describing the edge from
the preceding scope. Its `type` is `root`, `sequence`, `retime`, `repeat_play`, or
`repeat_gap`. A `sequence` entry includes the original child `index`, counting
empty children even though they contribute no content. `repeat_play` and
`repeat_gap` distinguish the play child from an explicitly owned gap branch;
their stable Repeat identity is retained in `instance.repeats`.
Retime conversion retains fractions. The final `terminal.type` is `node`, `gap`,
`project_start`, or `project_end`. A default `gap` retains its owning Repeat as
the final scope and reports `after` (the preceding stable play ID), gap-local
`position`, and `duration`. It does not create a Hold or invent a node identity.
Explicit gap branches descend through their existing nodes.

Bias chooses the adjoining content at an exact seam. Left at project zero and
right at project duration return the outward project edge; the opposite biases
descend inward. A returned node scope can be supplied as an `occurrence` target
to `resolve-selection` below to map its exact local coordinate back to project
time. The query does not round positions or choose a future edit scope.

An optional top-level `limits` object accepts both `max_scopes` and
`max_comparisons`; omitting it uses 257 scopes and 8192 comparisons. The response
reports `comparisons` used. Exhaustion fails with `BoundaryQueryLimit` and no
partial location. A zero comparison budget can still return an outward project
edge with one permitted root scope. These limits bound the structural query,
not loading/validating the stored revision or constructing its index. The envelope,
request and limits reject unknown fields. A stale revision returns
`RevisionConflict` with the current revision; a foreign project returns
`ProjectConflict`. Queries never change authored state or history.

`resolve-selection <project.deadpan> --json <selection.json>` resolves a point or
nonempty range against one immutable revision without changing the project.
It works while a writer holds the package. For the 45-frame `pause-1` above:

```json
{
  "protocol": 1,
  "request": {
    "project_id": "PROJECT_ID_FROM_DUMP",
    "expected_revision": "REVISION_ID_FROM_DUMP",
    "role": "linked",
    "selector": {
      "type": "point",
      "target": {
        "boundary": {
          "coordinate": {
            "space": "local",
            "node": "pause-1",
            "position": {"numerator": "21", "denominator": "2"}
          },
          "bias": "right"
        }
      }
    }
  }
}
```

This returns the exact project boundary and its one ties-to-even quantization
(10.5 becomes frame 10 if this Hold starts at zero). A `range` selector replaces
`target` with `start` and `end` targets. Reversed, empty, or quantization-collapsed
ranges fail with `InvalidRange`; endpoints are never silently reordered.
`role` is the requested editing role (`linked`, `video`, or `audio`), independent
of which source coordinate identified the boundary. This query does not apply
a role edit or create an attachment.

The strict types are in [`anchor.rs`](../crates/deadpan-core/src/anchor.rs):

- `local` uses a host node and exact frame position. A repeated host requires
  an explicit `occurrence` with that node and its complete ordered Repeat path.
- `occurrence` stores that `instance` directly alongside `position` and accepts
  no additional scope.
- `sequence` uses a pinned integer `frame` and accepts no occurrence scope.
- `source` uses an asset plus an original video/audio timestamp or audio sample
  index with its explicit original sample rate. It requires a Source occurrence.
  Equivalent clocks convert exactly, signed origins remain intact, and positive
  mix-clock audio offsets delay the mapped audio boundary. A held picture has no
  unique reverse source boundary and is rejected here.

All positions must fit their host and every intervening Retime mapping. End
boundaries are legal, including zero in an empty project. Local anchors never
guess a play, even for a one-play Repeat. Missing or retired iterations fail.
The index stores authored parents and sequence prefixes; seeking billions of
plays does not expand them. Repeat identity lookup scans compact runs.

`set_mark` persists one of these boundaries, its owner, label, and explicit loss
policy. Named-mark selectors are described below. Range-editing operators and
temporal attachments remain to be implemented.
[Boundary verification](ANCHOR_VERIFICATION.md) preserves the original query
evidence; [mark verification](MARK_VERIFICATION.md) covers the schema-3 extension.

## Persistent marks

A mark command uses the same protocol/revision envelope as every other edit:

```json
{
  "command": "set_mark",
  "id": "a",
  "owner": "ROOT_NODE_ID_FROM_DUMP",
  "label": "After the answer",
  "boundary": {
    "coordinate": {
      "space": "local",
      "node": "pause-1",
      "position": {"numerator": "12", "denominator": "1"}
    },
    "bias": "right"
  },
  "loss_policy": "keep_unresolved"
}
```

`set_mark` creates or replaces the entire named mark with one binding, including
deliberately rebinding an unresolved mark. `delete_mark` takes `id` and removes
all bindings. The owner controls lifecycle and
is separate from the anchor coordinate. Labels accept Unicode; IDs use the
existing 1–128 byte ASCII identity rule. Documents permit up to 1,024 physical
bindings per mark and 100,000 in total, including primaries, within the shared
64 MiB JSON limit. See [logical mark semantics](MARK_FRAGMENTS.md).

Local and occurrence marks follow retained content through edits, using exact
fractions. At an internal boundary, left bias follows preceding content and right
bias follows following content. At a host's outside leading/trailing edge, the
corresponding left/right bias stays with that edge. A mark inside moved content
follows it within the same host; moving its host carries the mark too. Repeat
marks follow stable play identities, and gaps belong to the preceding play.
Source coordinates stay in the original asset clock, independent of ownership
or whether a timeline occurrence currently uses that moment. Sequence-pinned
coordinates stay fixed through ripple edits.

When an owner, host, play, gap, or targeted content disappears, `delete_owned`
removes that binding or `keep_unresolved` preserves its original coordinate and a
typed reason. The logical mark survives while any binding remains. Unresolved
bindings never attach automatically to a replacement at
the same timestamp or ID. `wrap_repeat` accepts `anchor_policy: "first"` or
`"unresolved"`; omission defaults to `first` for compatibility. A concrete old
occurrence can follow the first new play, while an authored Local child mark
still applies to that child across its plays. New plays do not copy concrete
occurrence marks. Every mark change is part of the same reversible transaction
as its structural edit; dry-run exposes it in `edit.forward.marks`.

Use `resolve-selection` with these selector shapes and the current project,
revision, and media role:

```json
{"type": "mark", "target": {"id": "a"}}
```

```json
{"type": "mark_range", "start": {"id": "a"}, "end": {"id": "b"}}
```

Each named target may supply `occurrence` when its stored Local/Source coordinate
needs explicit scope. Missing marks return `MarkMissing`; marks with no bound
bindings return `MarkUnresolved`. Unresolved coordinates are not fallback targets.
Equal exact results retain all matching ordinals in `mark.bindings`; different
exact results return `MarkAmbiguous`, even when rounded frames coincide.

## Original media ownership

```sh
cargo run --locked -p deadpan-cli -- project retain-original /tmp/example.deadpan /absolute/source.mp4
cargo run --locked -p deadpan-cli -- project retain-original /tmp/example.deadpan /absolute/source.mp4 --linked
cargo run --locked -p deadpan-cli -- project originals /tmp/example.deadpan
cargo run --locked -p deadpan-cli -- project originals /tmp/example.deadpan --after BLAKE3_DIGEST
cargo run --locked -p deadpan-cli -- project verify-original /tmp/example.deadpan BLAKE3_DIGEST
cargo run --locked -p deadpan-cli -- project relink-original /tmp/example.deadpan BLAKE3_DIGEST /absolute/moved.mp4 --expected-version 1
```

Retention defaults to a managed complete original, using APFS cloning or verified
copying. `--linked` records its external location. The response reports the
actual retention method and `authored_asset_registered: false`: stream
qualification and insertion into the edited document remain outstanding.
Neither operation changes the authored revision or undo history.
When the native app owns the package, retention and relinking use its
[authenticated preparation service](LIVE_PROJECT.md#background-preparation).
Hashing and copying run on the import worker; the owner commits the inventory.
The ordinary final JSON shape is unchanged. Read-only inventory and verification
continue to use independent readers.

`originals` returns at most 100 records and a `next_after` content digest. Continue
until an empty page. Digests are 64 lowercase hexadecimal BLAKE3 characters,
without the storage filename prefix. `verify-original` creates a private snapshot
and checks every byte against both recorded checksums. It does not qualify the
container's streams or renderability. Managed records use the package copy;
linked records read the recorded path. Inventory and verification are read-only.

Relinking verifies identical complete content and requires the current location
version, independent of the document revision. Wrong content returns
`OriginalContentMismatch`; stale versions return `OriginalLocationConflict`.
Missing owned or linked bytes return `OriginalOffline`. Paths must be absolute
UTF-8 local regular files; final symlinks and parent traversal are rejected.
Relinking the already recorded path and bookmark is a successful no-op and keeps
the location version.
The CLI uses the host defaults of 64 GiB and 300 cooperative seconds. The Rust
host API also accepts cancellation, tighter limits and opaque bookmark data.
See [original media](ORIGINAL_MEDIA.md) for durability and remaining import work.

## One-Original projects from a file or YouTube URL

```sh
cargo run --locked -p deadpan-cli -- project create-original /tmp/example.deadpan /absolute/source.mp4
cargo run --locked -p deadpan-cli -- downloader install
cargo run --locked -p deadpan-cli -- downloader status --probe
cargo run --locked -p deadpan-cli -- project create-from-url /tmp/example.deadpan 'https://youtu.be/VIDEO_ID' [--cookies /absolute/cookies.txt]
cargo run --locked -p deadpan-cli -- project original-provenance /tmp/example.deadpan BLAKE3_DIGEST
```

`create-original` is the closed-project equivalent of native New: it creates an
Awaiting Source [single-Original](SINGLE_ORIGINAL.md) package at the explicit
path, retains the complete file as a managed original, qualifies its picture and
first audio track and establishes the full-source baseline in one store
transaction. The package is built at a hidden sibling path and renamed into
place only once Ready, so a failure leaves nothing at the requested path.
Headless creation keeps explicit developer paths; only the native
app chooses `~/Documents/Deadpan`.

`create-from-url` uses the pinned helpers that `downloader install` verifies
into `~/Library/Application Support/Deadpan/helpers` (`--root`/`--helpers`
select another absolute directory). It emits one JSON object per stdout line
(`fetching_metadata`, `metadata` with title, author, duration, thumbnail URL and
selected streams, `progress`, `assembling`, `creating_project`), then the
created project and its provenance. Refusals before transfer create nothing.
SIGINT/SIGTERM cancel the helper process group and remove the private files;
every signal only requests cancellation.
Errors carry actionable codes such as `YouTubeVideoUnavailable`,
`YouTubeAgeRestricted` or `YouTubeRateLimited`. `original-provenance` reads the
private provenance of one retained original. See
[YouTube import](YOUTUBE_IMPORT.md) for helpers, selection, security and limits.

## Measured source registration

After retention, explicitly qualify the selected streams and optionally insert
them with one authored edit:

```sh
cargo run --locked -p deadpan-cli -- project register-source /tmp/example.deadpan --request-json /tmp/register.json --dry-run
cargo run --locked -p deadpan-cli -- project register-source /tmp/example.deadpan --request-json /tmp/register.json
```

Use the actual current revision and root from `project dump`, and the original's
content identity from `retain-original`. Registration requires a fresh explicit
`new_revision`; unlike the generic command envelope, it does not generate one.

```json
{
  "protocol": 1,
  "registration": {
    "expected_revision": "CURRENT_REVISION",
    "new_revision": "source-import-1",
    "original": {"algorithm": "blake3", "digest": "64_LOWERCASE_HEX_DIGITS_FROM_RETENTION"},
    "new_asset_id": "camera-1",
    "label": "Camera original",
    "insertion": {"parent": "ROOT_NODE_ID", "index": 0, "node": "clip-1", "label": "Opening", "purpose": "primary"}
  },
  "streams": {"type": "video_and_audio", "audio_stream": 1}
}
```

The required request protocol is `1`; unsupported or malformed protocols fail
before opening the project or source. Select the actual audio stream index. Other explicit choices are
`{"type":"video_only"}` and `{"type":"audio_only","stream":0}`.
Omitting or failing a selected stream is an error. Set `insertion` to `null` to
register without inserting. The host derives exact full-source placements from
measured indexes at the final chosen rate. A first primary picture can establish
a provisional basis; timed or explicit projects preserve it. `purpose` defaults
to `primary`; use `secondary` for supporting/reaction media that cannot choose
the basis. Registration without insertion never chooses or locks the basis.

The response has `protocol: 1`, `committed`, and `preview` or `outcome` containing
the resolved asset and qualification IDs plus the proposed edit or commit.
An identical current registration reuses its asset even if a different
`new_asset_id` was proposed. Without insertion, this is a no-op with
`committed: false` and `outcome.commit: null`. Dry-run performs actual verification
and decoding but writes no receipt or history, and can coexist with a writer.

Stale revisions fail before opening source bytes. Commits with current generation
requests require host relevance context and return `GenerationRelevanceRequired`
through this CLI; preview remains available to a host resolver. The CLI does not
invent unchanged context. [Source registration](SOURCE_REGISTRATION.md) documents
durability, historical lookup, bounded evidence and remaining native workflow work.

An open native owner performs qualification on its import worker. The request
file is parsed once and its complete registration and stream selection remain
captured through preparation. The owner rechecks the expected revision at commit;
an intervening edit can therefore reject an already decoded result. It never
substitutes GUI selection, allocates replacement caller IDs or retries at a newer
revision. `--dry-run` continues to verify and decode through an independent
read-only store without contacting the owner.

## Presentation and canvas

```sh
cargo run --locked -p deadpan-cli -- project create /tmp/automatic.deadpan
```

This starts a provisional 1920×1080, 30 fps canvas. The first primary video
insertion chooses its measured cadence and display geometry before calculating
source timing. Audio/Hold/secondary picture insertion or a project-time mark
locks the rate. Later primary picture records its identity without changing the
clock or canvas automatically. Registration alone leaves provisional state intact.
The dump and validation response expose `basis_state` and the first primary
source's immutable qualification binding. See [presentation policy](PRESENTATION_BASIS.md).

To preview the recorded first primary picture's geometry at the fixed rate, use:

```json
{
  "protocol": 1,
  "adoption": {
    "expected_revision": "CURRENT_REVISION",
    "new_revision": "adopt-primary-geometry-1"
  }
}
```

```sh
cargo run --locked -p deadpan-cli -- project adopt-primary-geometry /tmp/automatic.deadpan --request-json /tmp/geometry.json --dry-run
cargo run --locked -p deadpan-cli -- project adopt-primary-geometry /tmp/automatic.deadpan --request-json /tmp/geometry.json
```

The preview returns `edit.forward.presentation` with the exact before/after basis
and origin state. It changes no authored state and can coexist with a writer.
Commit is undoable and requires the expected revision; current generation
requests require host relevance resolution. Both forms use persisted measured
geometry, not caller-supplied dimensions or a guessed video cadence.

For an intentional creative canvas, the ordinary command envelope accepts
`{"command":"set_canvas","width":1080,"height":1920}` and supports `--dry-run`.
Both geometry paths keep frame rate, nodes, marks and temporal coordinates fixed.
`set_canvas` records explicit geometry; on a provisional blank project it also
fixes the existing rate. Generic commands that claim source-derived adoption
return `SourceBasisAdmissionUnavailable`; they must use the qualified host path.
Committed canvas geometry reevaluates normalized framing through the shared
[picture geometry](FRAMING.md). Native temporary canvas-edit previews remain open.

## Automatic Render

The public Render command exports the complete committed edit through the shared
render coordinator. It captures one revision, qualifies an automatic SDR encoder,
records that decision, encodes, independently verifies the complete file, and
publishes the report and movie through the destination journal. It does not edit
the project or add an undo entry. Unsupported picture or audio behavior fails
explicitly. HDR and the remaining mastering graph are still open.

```sh
deadpan-cli render /path/edit.deadpan --output /path/Exports --name edit.mp4
deadpan-app --headless render /path/edit.deadpan --output /path/Exports --expected REVISION_ID
```

`--output` must be an existing directory. `--name` is one `.mp4` filename;
omitting it chooses `deadpan-UUID.mp4`. Existing destination files are never
replaced. The current output path requires macOS and the qualified APFS
publication boundary. It uses the running executable as its private worker;
there is no public encoder, bitrate, helper-path or environment override.
The pinned selection policy is `AutomaticSdrV1`. See
[automatic admission](AUTOMATIC_ENCODER_ADMISSION.md),
[encoded output](ENCODED_RENDER.md), and [publication](RENDER_PUBLICATION.md).

For a closed project, this entrypoint owns the writer for the operation. When
the native app already owns it, Render uses its authenticated local endpoint and
observes that owner's exact workflow. A writer without an endpoint remains
unavailable. See [open-project routing](LIVE_PROJECT.md) for ownership and limits.
Opening an older writable package uses the backed-up migration below before
admission. `--expected` rejects a changed current revision. Rendering never
implicitly commits temporary Camera, gain, or room-tone previews in another app.

### Structured requests and events

`render PROJECT --json REQUEST.json` accepts a regular, nonsymlink file of at
most 16 KiB. The request has a strict schema and rejects unknown fields. For
example:

```json
{
  "schema_version": 1,
  "request_id": "request-unique-to-this-invocation",
  "context": {
    "project_id": "PROJECT_ID_FROM_DUMP",
    "revision_id": "REVISION_ID_FROM_DUMP"
  },
  "operation": {
    "operation": "start",
    "destination": "/absolute/path/Exports/edit.mp4"
  }
}
```

The host generates new job, attempt, cancellation, and publication identities.
The supplied `request_id` correlates this invocation's events; it does not yet
provide cross-process deduplication. Convenience commands generate that ID too.
Recovery operations accept exact existing job, checkpoint or publication IDs;
they cannot supply an encoder policy or restore a serialized live capability.

Stdout is JSON Lines: `admitted`, bounded `progress`, then `finished`, each with
`schema_version: 1`, `request_id`, and a compact status. Status includes the
captured revision, exact workflow target, stage, cancellation state, diagnostics,
retained paths, cleanup evidence and final receipt when available. A separate
`recovery_required` event reports unconfirmed worker cleanup. Errors use a
schema-1 JSON object on stderr with `error.code`, `error.message`, and optional
`error.current_revision`. Success exits zero only for confirmed publication;
failure, cancellation and `PublishedUnconfirmed` exit nonzero after safe release.

Each JSON record is limited to 256 KiB. Stdout uses nonblocking writes and a
one-second backpressure budget; a failed output stream requests cancellation
while the owner continues processing worker replies. Terminal and recovery events
make one bounded stderr fallback attempt if stdout is unusable, preserving the
same request, workflow, publication and cleanup observations. SIGINT or SIGTERM also
requests cancellation, persists it before signaling work, and drains owned work
before releasing the writer. If cleanup remains unconfirmed, the process retains
the writer and recovery diagnostic. It cannot truthfully report cancellation or
safe completion. A movie already committed before cancellation remains a
published or `PublishedUnconfirmed` outcome.

The public execution budget is 24 hours. A movie is bounded to 64 GiB, its retained
manifest to 256 KiB, and the project's retained render namespace to 128 GiB and
4,096 entries. Encoding and independent verification each admit at most 1,000,000
packets; an encoded packet is bounded to 32 MiB. These are refusal limits, not
disk reservations or promises that every input within them is supported. Existing
native geometry, memory, media and teardown bounds still apply.

### Status and explicit recovery

```sh
deadpan-cli render status /path/edit.deadpan
deadpan-cli render status /path/edit.deadpan --job JOB_ID
deadpan-cli render status /path/edit.deadpan --publications
deadpan-cli render status /path/edit.deadpan --publication PUBLICATION_ID
deadpan-cli render retry /path/edit.deadpan --job JOB_ID --checkpoint ENCODING_ATTEMPT_ID --output /path/Exports
deadpan-cli render reencode /path/edit.deadpan --job JOB_ID --output /path/Exports
deadpan-cli render reconcile /path/edit.deadpan --publication PUBLICATION_ID
```

Status is read-only and may run while another writer owns the package. It emits
`stored_status` with `live_progress: false`; it neither recovers unfinished jobs
nor treats saved verification as current authority. Collections contain at most
eight items. Follow `page.next_after` with `--after ID` for jobs or publications,
and `page.next_after_attempt` with `--after-attempt ORDINAL` for a job's attempts.
A nonnull cursor may lead to an empty final page.

`retry` freshly hashes and verifies the selected retained checkpoint and resolves
the decision belonging to its original encoding attempt. `reencode` retains the
job's original committed revision but performs fresh automatic qualification and
encoding. `reconcile` re-verifies retained media and checks the exact recorded
destination identities before completing its journal. It does not choose a new
destination or adopt a same-byte replacement. Each operation allocates fresh
attempt identities. Public recovery rejects historical engineering-policy jobs;
their existing engineering APIs and evidence remain available.

`render cancel PROJECT --json REQUEST.json` routes to the native owner and
requires a `cancel` operation with the exact job, attempt and cancellation token.
It never opens a writer or performs recovery. A running closed-project headless
invocation is cancelled through SIGINT or SIGTERM; it does not advertise a native
service endpoint. Stale requests cannot cancel a later workflow.

### Preview-versus-export verification

```sh
deadpan-cli verify-export /path/edit.deadpan --movie /path/Exports/edit.mp4 --revision REVISION_ID
```

This read-only diagnostic renders reference pictures for the committed
revision through the shared picture path and SDR encoder pixel boundary, reads
the limited audition bus, decodes the movie with the qualified decoders and
reports per-frame PSNR/structure and per-window level, SNR and exact measured
audio offset as JSON. `--frames`, `--every`, `--samples`, `--no-audio` and
`--report` select coordinates and a new report file. It exits nonzero with
`ExportVerificationMismatch` after printing the full report when any check
fails. macOS only. See [preview/export verification](PREVIEW_EXPORT_VERIFICATION.md).

## Storage, cleanup and portable copies

```sh
cargo run --locked -p deadpan-cli -- project storage /tmp/example.deadpan
cargo run --locked -p deadpan-cli -- project storage /tmp/example.deadpan --clean --dry-run --grace-hours 0
cargo run --locked -p deadpan-cli -- project storage /tmp/example.deadpan --clean --files-only --dry-run > /tmp/plan.json
cargo run --locked -p deadpan-cli -- project storage /tmp/example.deadpan --clean --files-only --plan /tmp/plan.json
cargo run --locked -p deadpan-cli -- project storage /tmp/example.deadpan --clean [--files-only]
cargo run --locked -p deadpan-cli -- project storage /tmp/example.deadpan --confirm-clock [--dry-run]
cargo run --locked -p deadpan-cli -- project copy-portable /tmp/example.deadpan /tmp/elsewhere/Example.deadpan
cargo run --locked -p deadpan-cli -- cache status
cargo run --locked -p deadpan-cli -- cache clean --dry-run
```

`project storage` reports every media namespace entry as referenced (with the
kinds of row that name it), unreferenced, unfinished or kept aside, plus the
database, checkpoints and per-user caches. It opens read-only and works while
the app holds the project. `--clean` removes unreferenced objects and
unfinished writes unchanged for the grace period (default 24 hours) that no
reader holds. Storage P then R is a dry run with `--files-only`, whose output
carries a `plan` (project, head revision, grace and each file's name, device and
inode) named by `plan_hash`, then `--clean --files-only --plan <that output>`:
it removes exactly the planned files that a fresh reference scan still finds
removable and reports the others, and refuses a plan for another head
(`StoragePlanStale`) or one that does not match its hash (`StoragePlanInvalid`).
Without a plan, `--clean` is a convenience with no native counterpart: it
expires due AI variants under the current clock (unless `--files-only`) and
removes what a fresh scan finds unreferenced at that moment. `--confirm-clock`
only expires due variants and confirms the clock so the app's automatic checks
resume. A clock earlier than the project's records is refused there
(`RetentionClockBehind`, dry runs included); cleanup then expires nothing but
still removes files. `variant_expiry.status` is `not_requested`, `skipped`
(with `reason` and `clock_anomaly`), `previewed` or `applied` (with the expiry
counts). Dry runs read on their own connection and write nothing. While the app
has the project open, these run through its live endpoint: plans and reference
scans on read-only opens off its service thread, which keeps serving edits,
and only the rechecked writes on its writer; the reply follows when the job
finishes. One such job runs at a time; it is refused with `StorageBusy` while
an import, render, AI pause, tracking, backup or the automatic retention check
could be publishing media.
`copy-portable` writes a verified
self-contained copy with managed originals and only referenced media, and
refuses when a referenced object is missing. `cache` covers proxies and
abandoned downloader staging (grace at least one hour) and only reports model
packs and AI runtimes. See [storage](STORAGE.md).

## History and checkpoints

```sh
cargo run --locked -p deadpan-cli -- project undo /tmp/example.deadpan --expected CURRENT_REVISION
cargo run --locked -p deadpan-cli -- project undo /tmp/example.deadpan --expected CURRENT_REVISION --dry-run
cargo run --locked -p deadpan-cli -- project redo /tmp/example.deadpan --expected CURRENT_REVISION
cargo run --locked -p deadpan-cli -- project checkpoint /tmp/example.deadpan
```

Undo/redo require the current revision and commit a new, never-reused revision
identity. Undo restores authored content; it cannot revive a stale command's
authorization. Editing after undo clears the redo path while retaining older
immutable revisions and their history. Named branch selection is not implemented.

Both history commands accept `--dry-run`. They return `committed: false` with the
proposed `outcome`, using the same navigation and patch validation as the write.
The proposed revision is not reserved; the document, history, and redo stack stay
unchanged. Dry runs may run while another process owns the writable session.

`project validate` checks SQLite integrity and foreign keys, then validates all
retained revisions, including edits abandoned after undo: command and patch
correspondence, inverse patches, revision identities, current cursor, and redo
order, recomputing every command. `project validate PATH --quick` performs only
what opening does: it hashes every stored history row into a chain and
recomputes only the revisions after the
[history receipt](TIMING_STORAGE.md#verified-history-receipts) that this exact
build last proved. Stored document, request, and patch JSON is
limited to 64 MiB each, measured in bytes before SQLite returns the text. Stored
identity fields are bounded to 128 bytes and revision kinds to the defined values
before extraction. Validation reads a few documents at a time plus numeric redo
IDs. It is an integrity check, not an authenticity signature or a repair
operation.

Only one writable `ProjectStore` may own a package. Read-only dumps, validation,
and dry runs can coexist. Structural commands, history and primary-geometry
adoption route to the native writer through its authenticated endpoint. They
retain explicit project and revision targeting and do not use GUI focus.
Original retention, relinking, source registration and checkpoints use that
owner's asynchronous preparation path after a real writer-lock conflict. A writer
without an endpoint remains unavailable. See
[the open-project contract](LIVE_PROJECT.md) for exact targets and limits.

A checkpoint is a consistent SQLite backup including committed WAL data, stored
under `Snapshots/`. It is not a portable project copy: the media directories are
not duplicated. With an open native owner, its worker pins a read transaction
and copies at most 32 pages per step, with a default 1 GiB/five-minute limit. The
owner publishes the finished file after rechecking its session and namespace.
The internal receipt records the revision actually captured by the backup,
which may differ from the admission or current revision; ordinary stdout keeps
the existing `database_checkpoint` path field. Full recovery UI and portable
project-copy workflow remain open.
Unsupported database schema versions are refused without rewriting them;
future-schema read-only inspection still needs a compatibility implementation.

Open-owner preparation commands emit one final JSON result. SIGINT and SIGTERM
request cancellation of the captured operation, followed by observation until
the worker stops. The owner and client also request cancellation after 15 minutes;
the client allows up to five further minutes for drain. A lost reply or unconfirmed
drain reports an unknown outcome and never retries or switches owners. Inspect
the project before repeating a mutation.

An error after checkpoint publication retains `preparation_receipt` and
`completion_error` alongside the output path. For
`CheckpointPublishedUnconfirmed`, the file was renamed into `Snapshots/` but
directory durability could not be confirmed. Stdout retains that result and the
process exits nonzero; inspect the file before repeating the command. Workspace
refresh failures use `host_refresh_error` without discarding an operational or
authored receipt. If final stdout delivery fails, the owner retains its completed
observation until release or its ten-minute expiry. Detailed-output limits retain
the compact receipt with `host_reply_detail_omitted: true`.

## Schema migration

Database schema 68 is current. Schemas 66 and 67 return `MigrationRequired` and
`project migrate` upgrades it (backup, copy, validate, promote). Earlier
schemas return `SchemaUnsupported` before writer locks, backups, recovery,
authored JSON parsing or database writes, and their packages remain intact
([development formats](DEVELOPMENT_FORMATS.md)).

```sh
cargo run --locked -p deadpan-cli -- project migrate /tmp/example.deadpan
```

On a current package this validates read-only and returns equal schemas with
`backup: null`, including alongside a native writer. A package from a newer
build returns `SchemaNewer`; `project view` shows it read-only. Release
migrations will run through the implemented runner: back up, migrate a copy,
validate, promote atomically; a failure after the backup returns
`MigrationFailed` naming the retained backup. See the
[release migration policy](BACKUPS.md#release-migration-policy).

## Backups

```sh
cargo run --locked -p deadpan-cli -- project backups /tmp/example.deadpan [--verify]
cargo run --locked -p deadpan-cli -- project backup /tmp/example.deadpan
cargo run --locked -p deadpan-cli -- project restore /tmp/example.deadpan BACKUP_ID [--expected REVISION] [--dry-run]
cargo run --locked -p deadpan-cli -- project view /tmp/example.deadpan
cargo run --locked -p deadpan-cli -- project relink-moved /tmp/example.deadpan
```

`backups` lists verified backups newest first with the revision, beats, length
and edit count each holds; `backup` makes one beside an open app; `restore`
backs up the current state first, and with `--expected` refuses unless that is
the head revision; `--dry-run` verifies without writing. While the app has the
project open, the restore runs on its writer through the live endpoint
(`BackupRestoreFailed` carries the app's refusal), starts the app's new
session at the restored state and replies before the replaced owner's endpoint
stops; later commands discover the new owner.
`relink-moved` follows the bookmarks of missing linked originals and relinks
only identical content. Error codes: `BackupNotFound`, `BackupOtherProject`,
`BackupInvalid`, `BackupCancelled`, `BackupDeadline`, `DiskFull`. See
[backups](BACKUPS.md).

Generated Hold acceptance/reversion semantics were introduced in core schema 5, but generic
project commands and initial import reject newly introduced generated artifacts
with `GeneratedAcceptanceUnavailable`. The dedicated [store acceptance API](GENERATION_ACCEPTANCE.md)
requires a current selected Ready receipt and all retained objects. It is a Rust
host API, not a CLI generation or audition command.

This is a database migration, not portable media copying or a recovery UI. The
immutable source specification and media references are unaffected.

Generation request storage currently has a Rust host API, described in
[generation request storage](GENERATION_REQUESTS.md). There is no CLI generation
command yet. A plain CLI edit/undo/redo fails with `GenerationRelevanceRequired`
when current requests require host context reconciliation; it cannot silently
bypass that transaction boundary. Dry-run remains read-only.

Machine-readable failures go to stderr with a schema version, stable error code,
and explanation; the process exits nonzero. A failed storage write never reports
a committed revision. Tests cover SQLite page exhaustion (`DiskFull`), rollback
after an interrupted history write, process exit inside an uncommitted database
transaction, and cross-process writer rejection.

## Transcription

`transcribe <project.deadpan> --model <ggml.bin> --sha256 <hex> [--language
<auto|xx>] [--asset <id>]` transcribes the single-Original project's Original, or
an explicit registered source, in the isolated whisper.cpp worker and stores the
transcript as an annotation outside history. It needs the project's writer.
`transcript <project.deadpan> [--search <words>] [--asset <id>]` prints stored
transcripts or phrase matches with exact source sample bounds. Errors use
`TranscriptionUnavailable`, `TranscriptionCancelled` and `TranscriptionFailed`.
See [local transcription](TRANSCRIPTION.md).

## Analysis corrections

`corrections <project.deadpan> [--asset <id>]` reads the Original's stored
corrections: the version, Undo/Redo labels, unreadable values, regions that no
longer apply, and the corrected words and pauses with their indexes.
`corrections <project.deadpan> --json <request.json> [--dry-run]` makes one
`:correct` change through the same `deadpan_analysis::Corrections` operations
and `change_analysis_corrections` store call as the sheet:

```json
{"protocol": 1, "expected_version": 0,
 "change": {"type": "edit_word", "word": 0, "expected_text": "helo", "text": "hello"}}
```

Change types are `edit_word`, `remove_word`, `join_words`,
`set_word_bounds` (`start_cs`, `end_cs`), `add_pause_after`, `remove_pause`,
`set_pause_bounds` (analysis samples `start`, `end`), `drop_inapplicable`,
`discard_unreadable`, `undo` and `redo`. Word changes repeat the word's current
`expected_text`; pause changes repeat `expected_start` and `expected_end`. A
different stored version returns `AnalysisCorrectionsConflict`; a different
target returns `CorrectionTargetChanged`. Neither writes. A dry run returns the
label and proposed corrections without writing. Corrections never create a
document revision. Inspection and dry runs only read, so they work while the
app has the project open. While the app holds the writer, a committing change
runs on it through the [live endpoint](LIVE_PROJECT.md) with the same version
and target checks, and the app republishes its corrected transcript and pauses;
an open correction sheet then sees the newer version and refuses its stale
change.

## AI pauses

`generate-hold <project.deadpan> --hold <node-id> [--seed N] [--variants 1-4]
[--motion still|subtle|moderate] [--target ID|none] [--instructions TEXT]
[--another]` fills a Hold with pictures from the local LTX MLX runtime: it
records a bridge request (or, with `--another`, joins the Hold's current one),
runs one attempt per variant with its own seed, qualifies and publishes each
bundle and records it Ready, without editing the project. `accept-hold
<project.deadpan> --request <request-id> [--attempt <attempt-id>]` accepts the
request's selected (or the given) Ready variant as one undoable edit. When the
app has the project open, both route through its
[live endpoint](LIVE_PROJECT.md#ai-pause-jobs): generation runs as the app's
own AI job. Errors use `GenerationUnavailable`, `GenerationInputsUnavailable`,
`GenerationRefused`, `GenerationCancelled`, `GenerationFailed` and
`GenerationUnknown`. The direct (not live-routed) `generate-hold` report adds
`colour`: each side's model-input conversion from the conditioning manifest
(`srgb_codes_unchanged`, `rec709_to_srgb` or `authored_black`). Older retained
inputs may record `rec709_codes_as_srgb` with `approximate: true`; newly
prepared BT.709 inputs are converted before fitting. See [AI Holds](AI_HOLDS.md).

Motion, the saved region target and optional guidance are captured in the request and reported under
`options`; `--another` retains them and refuses control or seed flags. See
[Generation controls](AI_HOLDS.md#generation-controls) for bounds and the
open-project behavior.

### Choosing, keeping and discarding variants

```sh
cargo run --locked -p deadpan-cli -- ai-variants /tmp/example.deadpan [--hold NODE] [--joins]
cargo run --locked -p deadpan-cli -- select-hold /tmp/example.deadpan --request REQUEST --attempt ATTEMPT
cargo run --locked -p deadpan-cli -- keep-hold /tmp/example.deadpan --request REQUEST --attempt ATTEMPT [--off]
cargo run --locked -p deadpan-cli -- discard-hold /tmp/example.deadpan --request REQUEST --attempt ATTEMPT
cargo run --locked -p deadpan-cli -- dismiss-attempt /tmp/example.deadpan --request REQUEST --attempt ATTEMPT
```

`ai-variants` lists each pause's offered variants exactly as the inspector
does (`deadpan_cli::generation::variants::offered`, shared with the app): the
request, the selected attempt, and per variant its number, seed, sampled size,
Ready time, `kept`, `picked`, `selected` and `expires_unix_seconds`, plus the
attempts a crash interrupted. `--joins` adds the advisory join readings
`:compare-ai` shows, decoded read-only from the request's origin revision; a
reading that cannot be made reports its `error` in place. It writes nothing and
works beside an open app.

`select-hold`, `keep-hold [--off]` and `discard-hold` make the `:pick-ai`,
`:keep-ai` and `:discard-ai` changes; `dismiss-attempt` is the Jobs panel's
discard of an interrupted attempt. Each first checks that the variant is
offered (`GenerationVariantUnavailable` otherwise), applies one durable
operational store change, and returns `changed` and the pause's offered
variants afterwards. None is an edit or undoable. With the project open they
run on the app's writer and refresh its inspector; a preview of a discarded
variant, or of another variant than a newly selected one, closes.
