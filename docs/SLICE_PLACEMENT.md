# Visual slice placement

`:splice` opens an unsaved linked Original or edited-slice placement at the retained
Edit cursor. Copy a half-open range with `v`, motion and `y` in Original or
Your edit. To replace time, select an Edit range with `v`, motion and `v`,
open `:splice`, then choose **Replace selection · r**. Fast `p/P` replaces a
selected Edit range immediately; without one it inserts at a Sequence slot.

Without an Edit range, `y` copies the selected whole beat. A saved `d`/`dd` or
`:delete` cut also supplies the register. Empty groups retain their complete
structure and historical label, and insert only at explicit Sequence slots.
Their source card has no picture or audio endpoints; In/Out, Replace and Move
are unavailable. `j/k` distinguishes neighboring slots at the same Edit frame.
Placement adds one structural revision, selects the inserted group and adds no
time. The retained destination picture remains visible.

For a nonempty range or whole beat freshly captured from Your edit, **Move slice · m** relocates the
linked range in one transaction. The source and destination must still refer
to that same committed revision. An older register remains valid for Copy;
after another edit or Undo, yank again before moving. Original is immutable
and only supports Copy. Move and Replace are mutually exclusive. Toggling
either operation retains the separately captured insertion destination.
Whole-beat Move preserves adjacent empty siblings at either endpoint. Empty
groups use cut/paste to move.

The current implementation accepts child seams and strict interiors of direct
Source, ordinary Hold and supported transparent-fragment children in ordinary
Sequence groups. The captured group remains the insertion owner. Enter a child
group to place inside it; a proposal never silently descends into that group or
rounds the insertion to a neighboring beat.

Replacement removes a captured half-open Edit range. Its endpoints can be
Sequence seams or supported Source/Hold/fragment interiors. Complete composites
between those endpoints are removed as subtrees. The range stays fixed while
the source In/Out changes. Zero-duration children exactly at either endpoint
survive in order; strictly interior ones are removed.

## Keyboard and visible state

| Input | Action |
| --- | --- |
| `i`, `o` | Select included In or exclusive Out for local refinement. |
| `r` | Switch between Insert and Replace selection when a captured range exists. |
| `m` | Switch between Copy and Move for an edited slice; leave Replace. |
| `d` | Select the insertion boundary; replacement keeps its removal range fixed. |
| `h/l`, Left/Right | Adjust the selected control by a frame; counts work. |
| `j/k` | In Insert or Move, next/previous Sequence slot, including distinct zero-length slots. |
| `f` | Inspect the destination picture; in Move, select the insertion join. |
| `s` | In Move, select the source removal join. |
| `b` | Compare Before and Proposed in the captured destination context. |
| Space | Play/pause; resume at the retained heard sample. |
| Shift+Space | Loop insertion/replacement with context; in Move, loop the selected site. |
| Enter | Commit the exact prepared proposal once. |
| Escape | Cancel and restore the entry cursors and pane. |
| Tab / Shift+Tab | Traverse native controls; Enter activates the focused button. |

The source strip shows decoded In and Out-minus-one pictures. Boundaries are
zero-based, half-open values; displayed pictures use one-based frame labels.
The destination keeps a larger picture, a named group and Sequence slot or
interior beat/local boundary, an insertion marker, and a provisional timeline
interval. Original and Edit clocks, linked picture/sound scope, and the unsaved
state remain visible. Replace shows the removed and proposed intervals at the
same scale, both exact frame counts and the signed change in duration.
Pending or unavailable pictures cannot acquire successful display labels.
Text composition and reserved macOS/Kestrel chords retain input ownership.

Move shows two local timelines with separate saved/proposed intervals, the
source and destination groups, and unchanged total duration. Switching `s/f`
changes inspection only; it does not rebuild the proposal. A destination
strictly inside the source range fails. An exact no-op explains that there is
no change and cannot commit, while its source endpoints remain inspectable.

## Comparing move joins

For source `[a,b)` and retained destination `d`, removal compares the saved
source range with the proposed closed join. Insertion compares the saved
destination seam with the proposed inserted range. Before/Proposed translates
prefix and suffix offsets relative to the selected site. Interior time clamps
to its counterpart; an exact empty seam maps to the counterpart start.

Use complete absolute frame-to-sample boundaries for audio. This is a local
time comparison, not a promise that every sound has moved: root-owned sound
events keep their global clocks. Loop lead/follow is bounded by the project
and the other move site. The UI labels context shortened at that other site.
Comparison outside the paired context visibly returns to the counterpart join.
Playing and paused states are retained. A terminal cursor stays at the exact
boundary; the displayed picture is the final included frame.

A move across group ownership at `d=a` or `d=b` can change framing or sound
without changing frame order. Both sides then compare identical global frames
and samples and say **Move between groups; timing unchanged**. The timelines
show the same interval on both sides.

## Authority and lifetime

The native UI owns only a local range and destination draft. It sends a
session/project/base-revision/draft/change identity to the project service.
The service retains one exact `SourceMomentInsertionRequest`,
`SourceMomentInteriorInsertionRequest`, `SourceMomentReplacementRequest`, or
edited-slice `CommandRequest`,
qualified media, proposed document and
compiled plan. Preparation creates no authored revision
or undo entry. It does not emit an asset-import completion or change selection.

Picture and audio consume the same genuine proposed document. Original uses
`Snapshot::proposed`; edited contents use `Snapshot::proposed_edit_slice` with
the opaque store admission described in [edited slices](EDITED_SLICES.md).
The picture worker validates its private admission against the exact committed
base, checks the captured receipts, and compiles its own plan on its worker
thread. It cannot turn a proposed picture into a committed Camera target.
The independent endpoint worker has one active request, one replaceable pending
pair and one reply. Both use the shared SDR picture semantics; endpoint work
cannot replace the main picture request.

Only the matching ready identity can commit. The service uses the retained
request, so the saved document is exactly the previewed document. A successful
receipt survives a later workspace-refresh failure, and retransmitting that
success cannot insert a second slice. Undo restores the preceding structure.

Move retains one `MoveRange` request and admits its proposed media through the
current-revision store validation path. It reuses the edited picture/audio view;
historical copying does not grant authority to move. The result may contain
several complete roots and receives no artificial enclosing group. Both service
and UI validate the complete contiguous result interval. Its commit receipt
carries that exact range and destination parent, session and project. The UI
installs the finished selection once, only with the matching visible revision
and reconciled scope. A failed refresh preserves the old visible selection and
the saved-edit reopening guidance. See [atomic moves](ATOMIC_MOVES.md).

Interior placement uses one `SpliceSourceAt` command. The service allocates all
Split identities once; commit reuses those identities and the exact local
boundary. The core captures original sampling clocks before splitting, retains
both fragments' framing and audio context, then inserts the Source and moves the
suffix. Placed sounds transform once. A failed transaction saves neither the
split nor the insertion; one Undo removes both. This adds no document or table
fields. Frozen legacy command adapters reject this command.

Replacement uses one `ReplaceSource` command, with identities allocated once
for both endpoint splits and the inserted Source. Sampling clocks are captured
before splitting; marks transform only after the final structure exists.
Placed sounds use one direct replacement map, preserving suffix samples that
separate Delete/Insert rounding could lose. Core schema 34 persists this map;
database schema 43 stores it. Under the approved development-format policy,
databases 39–42 are refused without writes or backups. Existing adapters for
schemas 1–38 remain available. Create a new project for current native testing.

Local refinements never modify the copied register. Cancellation revokes the
last **issued** identity, including when a newer local refinement was not yet
submitted. Late endpoint/proposal replies cannot restore an abandoned draft.
A changed project session or revision invalidates the destination, stops
audition and revokes pending picture work. Opening a new draft captures fresh
authority. Audition advances the draft cursor without moving saved editor
cursors or beat selection.

Edit selection has its own project/session/revision/group identity, independent
of the copied register. `v` starts and finishes selection; Escape clears it.
Finished ranges remain while the cursor moves, and playback never extends an
active range. Command entry captures the range, including its absence, so a
later completion cannot supply a new target. Before audition covers the removed
range; Proposed covers the inserted range. Comparison translates the retained
suffix using exact absolute frame-to-sample boundaries.

## Remaining specification work

This is partial [§9.7](spec/DEADPAN_SPEC.md#97-visual-slice-placement).
Picture-only and audio-only policies, named registers, and Repeat/Retime occurrence destinations
remain required. These controls do not establish completion of DP-05 or DP-20.
