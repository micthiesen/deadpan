# Exact Original moments

An Original moment is a nonempty half-open range of measured presentation-frame
ordinals. Its start is the first selected frame's original PTS; its end is the
next frame's PTS or the measured final-frame endpoint. Ordinal count is not an
elapsed duration for variable-frame-rate media.

`derive_source_moment` derives candidate intent from the same measured indexes
used for full-source import. It retains the selected picture's exact interval
and natural playback rate. The enclosing beat rounds the exact elapsed
duration upward once at the current project rate. Picture holds its selected
endpoint during any rounding slack. The helper grants no media admission,
registration or edit authority.

## Picture selection and handles

New moments retain the entire measured `SourceVideo.span` and its natural affine
mapping. `SourceVideoMapping::SelectedPlacement` records the visible exact
interval separately. The full span provides source context for later edits;
endpoint holding still selects only frames intersecting the selected window.
Fractional source ticks remain exact, and source-anchor queries reject hidden
context outside the selected boundaries. See [picture timing](SOURCE_VIDEO_MAPPING.md).

Core schema 36/database 45 store this representation and dormant linked audio.
Development databases 39 through 44 are refused without migration or writes.
Retaining handles does not yet implement the Trim, Slip or Roll workflow.

## Audio selection and phase

Video boundaries can fall between original audio samples. Narrowing the stored
integer audio span or fitting it to the rounded beat would change either the
selection or its playback rate. `SourceAudioMapping::SelectedPlacement` therefore
keeps the complete measured integer-sample `SourceAudio.span` and its affine
`start` and `frames`, while adding an exact `selection` interval in that mapping's
local project-frame clock. The independent mix-sample `audio_offset` shifts both
mapping and selection by the same exact amount.

The interval must be ordered and contained between `start` and `start + frames`.
Equal endpoints explicitly retain dormant audio with no audible sample support.
Mapping endpoints and offset-shifted endpoints must fit signed frame bounds.
Validation applies to typed callers as well as JSON. The selected interval need
not lie inside the integer Source host, which remains an independent crop.

For overlapping video seconds `[v0,v1)`, audio seconds `[a0,a1)` and project rate `r`:

```text
mapping start = (a0 - v0) * r
mapping frames = (a1 - a0) * r
selection = [(max(a0, v0) - v0) * r, (min(a1, v1) - v0) * r)
beat duration = ceil((v1 - v0) * r)
```

No A/V overlap retains the full audio span and linked intent with an empty
selection at the nearest audio boundary. Audio before the picture selects its
mapped end; audio after the picture selects its mapped start. This includes
touching boundaries and prevents audio in rounded picture slack from leaking
into the slice. The Original's immutable asset metadata stays unchanged.
A positive selection containing no discrete original
sample remains valid and renders zero PCM. Missing terminal-duration evidence,
unavailable interior audio, mismatched originals/streams, invalid ordinal ranges
and arithmetic overflow fail explicitly. No priming trim is inferred.

The picture and audio selections keep original timestamps. Audio source anchors
outside the selected interval are unavailable even if their samples remain in
the retained full-span recipe. Exact selected endpoints remain valid boundary
anchors for nonempty selections. Dormant audio has no source-coordinate anchor.
Normal revision checks, occurrence isolation, inverse patches and
durable history apply to `set_source_audio_mapping`.

For example, with a retained one-second audio span beginning at source zero in
a 30 fps project, this command selects source milliseconds `[1,2)` at local
frame zero. It leaves the existing Source's picture and beat duration unchanged:

```json
{
  "command": "set_source_audio_mapping",
  "node": "moment",
  "mapping": {
    "type": "selected_placement",
    "start": {"numerator": "-3", "denominator": "100"},
    "frames": {"numerator": "30", "denominator": "1"},
    "selection": {
      "start": {"numerator": "0", "denominator": "1"},
      "end": {"numerator": "3", "denominator": "100"}
    }
  },
  "offset": 0
}
```

Use the normal revision-checked [headless envelope](HEADLESS.md) and dry run.

## Rendering and retained context

The shared plan keeps sampling origin/rate separate from audible extent,
resampling support and meaningful fade edges. Root and signal readers use the
exact selection. Discrete original filter taps remain
`[ceil(selected_start), ceil(selected_end))`; rounding that support never changes
the affine source phase. A one-millisecond selection at 44.1 kHz can therefore
use taps `0..45` while its 48 kHz output ends at sample 48.

Selection endpoints belong to the physical Source clock. Transfers and owned
timing bindings preserve their exhaustion on the retained grid. Transparent
Partitions crop allocation without turning their seams into new selections.
Ordinary source-placement gaps retain their existing processed-decay policy;
selection exhaustion is distinct from an authored silent Hold and does not
redefine a downstream Preserve stage's output.

Frozen timing layouts retain the effective audible interval in their existing
Source placement field. `FrozenAudioContext` schema 2 also retains the complete
phase mapping. Its schema-1 reader uses the closed historical mapping vocabulary;
old contexts remain usable against matching retained project history. Matching
ignores only the context wire version and compares every authored/media fact.
Context schema 5 admits empty Source support. Earlier context and supported
document/history readers retain their positive-support grammar, including nested
frozen layouts. Empty Source support produces digital silence without requesting
source PCM through both ordinary and retained audio clocks.

## Native selection and paste

In Original, `v` starts a temporal selection at the current boundary. Move with
`h/l`, counted motions or `gg/G`; `v` finishes the range and `y` copies it.
`Esc` clears the selection without clearing the copy. The Out frame is excluded.
The lavender range bar represents measured elapsed source time, including VFR;
its pointer snaps to measured presentation boundaries. The inspector shows both
ordinal boundaries and measured elapsed seconds. Original selection and copying
create no document revision.

Return to Your edit with `:sequence`. Select time with `v`, motion and `v` to
make `p/P` replace that range atomically. Without an Edit range, select a beat
and use `p` after or `P` before. An empty current group accepts either at its start. The explicit
Sequence owner and child slot keep a paste inside the intended group even at
its first or last boundary. A copied moment is session-local and binds its
asset to the exact source qualification. Session or receipt changes invalidate
it; ordinary edits and undo preserve it. The independent Edit range is bound to
its revision and ordinary Sequence scope. `:splice` previews an explicit
Replace selection choice with fixed removed bounds and locally refined Original
endpoints. See [visual placement](SLICE_PLACEMENT.md). Named/persistent registers,
the general Normal-mode yank operator, multiple-node registers and replacement
inside Repeat/Retime occurrences are
still required.

The native service captures session, revision, qualification, ordinal range,
Sequence scope and insertion slot or replacement range before preparation. Cached preparation can
admit a paste while another import works; an uncached paste retains its captured
intent and rejects stale revisions. `commit_prepared_source_moment` verifies the
existing receipt and live prepared Original token, derives exact Source timing
at the current project rate and commits once. It publishes no new asset or
receipt. History, relevance and the final Original freshness check share the
same SQLite transaction. Completion explicitly names the new Source and its
absolute cursor and group.

## Persistence and remaining workflow

Core schema 20/database schema 26 introduce selected audio placements. Database 25
replays through frozen core 19, including captured Hold framing. Older snapshot,
subtree, command, occurrence and patch vocabularies cannot acquire the new mapping.
Migration preserves the complete history and pre-migration backup.

Core schema 27/database 33 add `splice_source` for a supplied Source leaf at an
explicit ordinary Sequence slot. The frozen core-26 reader preserves genuine
DB32 nested pause histories while rejecting the new command. Migration retains
all snapshots, forward/inverse patches, undo/redo, pending redo and operational
rows, with an untouched pre-migration backup.

Generic child-index Insert does not provide the sample-resume semantics of a
timeline splice. Native selected-moment paste uses the new command. The
[moment-reuse board](design/boards/original-moment-reuse-v1.png) and
[interaction contract](design/README.md) remain its visual target. Actual GUI
and performance evidence must be recorded separately from state-only tests.
The [qualification record](qualification/moment-paste-2026-09-27.md) retains the
checks, review and unresolved native acceptance.

### Required splice boundary

The splice command preserves each shifted physical audio entry independently.
One offset for the whole suffix is insufficient. At 30000/1001 fps, inserting a
frame inside the first play of a repeated two-frame Source must resume that
play at its cut and later plays at their own start. A fixed local resume of one
frame for every play can start a later play at old sample 3204 instead of 3203.
Pre-existing transparent fragments retain their own visible entry as well.

Repeat gaps now have an explicit [definition reader](AUDIO_DEFINITIONS.md) and
root/PointCeil placement, including a configured gap with no current occurrence.
[Authored gap bindings](GAP_AUDIO_BINDINGS.md) now retain a typed gap owner,
the stable preceding play identity and a canonical clock for genuinely new
gaps. A surviving former final play gaining its first gap has no prior gap
placement to inherit. The general splice author must compose these compact
bindings across nested repeats and sparse overrides without allocating one
record per rendered play. Preserve outputs are physical domains for an outer
move; their intrinsic preparation inputs must not also receive that move.

Use the current owned recipes, existing timing layouts and shared DSP rather
than retaining a second historical recipe graph. Build the boundary partitions,
entry changes, inserted Source and mark transforms together before the single
reversible patch is created. Current recipe edits must keep working on retained
clocks. Explicit Sequence-slot Source splicing has core, actual PCM, compactness,
picture, history and migration regressions. The inserted Source begins unbound
on the canonical project-origin grid; it must not force its first sample to
local phase zero. Later insertions capture its actual sampling lattice.

General cursor splicing inside Repeat/Retime occurrences, rational owner clocks,
Visual replacement, named moments and preview trims remain required. These
limits concern insertion targets; shifted suffixes can retain compact Repeats,
gap branches, generated Holds and Preserve outputs.
