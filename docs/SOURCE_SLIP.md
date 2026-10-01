# Atomic Source Slip

`SlipSource { parent, node, delta_frames }` changes which Original material a
beat uses while keeping its output position and duration. Positive deltas select
later material. Core schema 39/database 48 introduced this command and the exact
[editorial window](SOURCE_EDIT_WINDOWS.md) it requires. The current core 40 /
database 49 increment also records the changed audio joins described below;
[Trim qualification](qualification/source-trim-2026-10-01.md) covers that correction.

## Scope

The target is a direct child of an explicit ordinary Sequence, under ordinary
Sequence ancestors. It may be a Source or one unity Partition containing
a Source. Source-owned framing and audio treatments are retained.

The Source must retain a full qualified video span, an explicit mapping with
adjacent endpoint holds, and a positive editorial window. Enabled audio must
retain the same asset's full measured span and its linked affine clock. Audio
may be intentionally absent. An independent sample offset remains unchanged.

The visible intersection of the editorial window and the selected allocation
must lie within measured picture support. Audio coverage does not constrain
handles: late audio, early endings and dormant linked audio remain valid.

The initial command rejects FitBeat mappings, missing or incoherent editorial
windows, cropped source spans, independently mapped streams, treated or nested
Partitions, authored retimes, Repeat occurrences and audio-only picture lead or
tail. It does not infer or silently promote these shapes.

## Exact transformation

For each stream, convert its retained endpoints to Original seconds. Its mapping
has slope `alpha = mapped_frames / original_seconds` and intercept
`beta = mapping_start - alpha * original_start`. Linked streams must have equal
positive slopes and equal intercepts before the independent audio offset.
Signed source origins and non-natural common rates remain exact.

Let full picture support be `[v0,v1)` and the visible editorial interval be
`[a,b)`. The exact allowed delta is `[v0-a,v1-b]`. The lower bound is rounded
upward and the upper bound downward to whole frames. The requested move is
clamped within those bounds. Invalid entry states and overflow fail without
changing the document.

Both enabled full mapping starts move by minus the applied delta. Full spans,
mapped extents, Source duration, editorial window and Partition allocation stay
fixed. Selected picture support is the editorial window intersected with the
shifted picture support. Selected audio support is the intersection with shifted
effective audio support, converted back by subtracting the independent offset.
An empty intersection retains audio with equal selection endpoints at the nearest
support boundary, so a later Slip can make it audible again.

The operation changes both maps in one reversible transaction. Existing audio
bindings, sample grids, resumes, effect clocks, gain/mute keys and root sounds
stay fixed. Changed source content invalidates its current audio lineage through
the ordinary command path. Local, Occurrence and Sequence marks keep their
coordinates. Source marks retain their immutable PTS and resolve at the changed
output position or report `OutsideMapping` in that occurrence.

A nonzero Slip changes both media joins. It records separate editorial edges
on both target sides and each incident neighboring side, including through
ordinary Sequence ancestors. These use the shared short fades while retaining
raw filtering support and sample phase. Exact coincident Hard policies retain
precedence. This is required even when a Split Partition keeps its full hidden
window: unchanged allocation alone does not make the new media join continuous.
See [audio edges](AUDIO_EDGES.md) and the current [Trim increment](SOURCE_TRIM.md).

## Store and headless use

The store checks the current asset contract, retained qualification receipt,
Original ownership and complete stream spans in the same transaction that
prepares the edit. Ordinary editing does not reopen or hash Original bytes.
Playback and export retain their own fresh media validation.

`ProjectStore::preview_source_slip` returns exact and whole-frame handle limits,
requested/applied deltas, a limiting boundary and an optional edit. It checks
project, revision, new revision identity and stored admission even for an applied
zero. A preview reserves nothing and supplies no authority for a later commit.

The shared headless command path exposes that report as `source_slip` on dry run,
alongside `edit`. An applied zero returns `edit: null`; it creates no history.
A raw authored zero Slip fails with `InvalidCommand`. Nonzero commits recheck
the captured revision and admission, then create one ordinary durable undo entry.

```json
{"protocol":1,"project_id":"project","expected_revision":"current",
 "new_revision":"next","command":{"command":"slip_source",
 "parent":"sequence","node":"beat","delta_frames":5}}
```

Use `deadpan-cli command PROJECT --json REQUEST --dry-run` to inspect the result,
then submit that captured request without `--dry-run` to commit. The same command
is available through `deadpan-app --headless` and the live project writer.

## Native stopped-picture preview

Native `:slip +5f` opens an unsaved preview for the selected picture beat in
Your edit. Negative, unsigned and zero whole-frame amounts are accepted; the
ASCII `f` suffix is required. Positive means later Original material. Clear any
active or finished Visual Edit selection first, including an empty one. Original,
catalog Sound and placed-sound focus do not supply a picture target.

Command entry captures the project session, revision, ordinary Sequence scope,
selected direct child and output range, both editor cursors and selection. It
captures a missing or ineligible target as a failure too. Later selection or
worker completion cannot supply a replacement. The service revalidates this
explicit target and the stored source receipt.

The preview has its own Edit inspection frame within the beat. An entry cursor
outside the selected beat is retained; only the temporary inspection frame is
clamped. Before and Proposed compare the same output coordinate. Neither changes
the real Edit or Original cursor, selected beat, group or Visual selection.

| Key or control | Action |
| --- | --- |
| `h/l` | Select material one project frame earlier/later. |
| `Shift+h/l` | Select material ten project frames earlier/later. |
| Left/Right | Inspect one delivered picture earlier/later within the beat. |
| Shift+Left/Right | Inspect ten delivered pictures earlier/later. |
| `i/o` | Inspect the first/last delivered picture. |
| `b` | Compare Before/Proposed at the same Edit frame. |
| Amount field | Enter an exact whole-frame amount such as `-3f`. |
| Tab / Shift+Tab | Traverse native controls. |
| Enter on the preview heading, or Apply | Save the displayed nonzero proposal once. |
| Escape, or Cancel | Discard the preview and restore valid entry context. |

These keys apply when the preview heading or background owns keyboard focus.
Text fields retain native editing; Enter in the amount field does not apply.
Focused buttons retain native activation. Active IME composition owns Enter and
Escape. Movement keys may repeat; Apply, comparison, endpoint inspection and
Cancel do not repeat while held. Command, Control and Option chords remain
reserved. This surface adds no `,v` binding.

The report separates requested and applied delta, exact rational handles,
inward whole-frame bounds and the limiting picture edge. After an overshoot,
one reverse nudge moves away from the reached handle immediately. Every input
in a key batch changes the intended amount; bounded preparation can coalesce to
its latest value. Invalid text retires the preceding ready proposal. An applied
zero displays the unchanged saved picture, disables Apply and creates no history.

### Display admission and saving

Preparation produces a proposed snapshot tied to the service's exact immutable
base. Apply requires all of the following: a nonzero current proposal; no pending
refinement or error; Proposed comparison; an idle project service; and matching
requested, decoded and successfully GPU-submitted picture identities at the
current inspection frame and viewer raster. A prepared snapshot or completed
decode alone does not enable Apply. Proposed pictures cannot become Camera
targets.

A refinement, inspection change, comparison switch, resize or picture failure
revokes readiness. The prior accepted picture and its caption/geometry remain
while replacement work prepares or fails. Pending picture identities are revoked
at entry, every draft change, cancel, save and captured-context invalidation,
including decoded pictures awaiting submission. Service publications reconcile
the draft before queued decoder replies are admitted. A late success or error
cannot restore a retired proposal.

Apply consumes the exact service-owned revision-bound request once. Further
Apply/Cancel input is disabled while saving. A successful transaction preserves
the captured cursor, selected Source or Partition wrapper and ordinary group.
The service saves a separate receipt before refreshing the workspace. If refresh
fails, the UI reports that Slip was saved and asks the user to reopen the project.
It does not imply the transaction failed or retry it. Duplicate delivery of the
same saved proposal returns its receipt without another write.

The latest saved receipt survives queries, rejected drafts and Undo in that
session; it clears when the project/session changes or closes. The UI consumes
it only for the matching applying draft. Historical receipts do not repeat
selection changes or stale refresh warnings after navigation or Undo. A changed
revision invalidates an unsaved proposal, including after a fresh Undo revision.
Cancel restores entry state only while that captured workspace remains valid.

Service, input/presentation tests, rendered replay and native keyboard/save/reopen
verification pass. See the [qualification record](qualification/native-slip-2026-10-01.md)
for exact source identities, results, inspected captures and physical-input limits.

## Remaining work

The native Slip preview stops playback and has no audition or waveform. It
implements the Slip contract and typed example in specification sections 6.3
and 6.4; it does not complete [Trim mode](spec/DEADPAN_SPEC.md#77-trim-mode).
Full Trim still requires In/Out, adjacent Roll, ripple/overwrite policy, physical
Source growth, nested occurrence editing and paired boundary-frame/waveform
previews. The specified `,v` entry and Tab mode cycling remain unimplemented.
Audio-only lead/tail needs an index-derived adjacent-picture interval before it
can be admitted. Exact windows and stored receipts alone do not qualify those
cases. Full physical-input, accessibility and release acceptance remain open.
