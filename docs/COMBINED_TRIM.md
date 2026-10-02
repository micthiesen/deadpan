# Combined Source Trim

`ApplySourceTrim` combines accepted In, Out, Slip and Roll values into one
reversible backend edit. [Qualification](qualification/combined-trim-2026-10-01.md)
records passing backend workspace, focused media, durable history and strict lint
checks. The native [Trim mode](spec/DEADPAN_SPEC.md#77-trim-mode) now connects the
complete draft to boundary pictures, waveform and audition. The
[native qualification record](qualification/native-trim-2026-10-01.md) retains
its compilation, replay, keyboard and native interaction evidence separately.

## Captured intent

The command names an ordinary Sequence, its direct Source or unity Partition,
and an optional captured literal right neighbor. `SourceTrimIntent` holds four
signed frame values relative to that entry document plus Ripple/Overwrite policy.
The active inspection control is not part of authored intent. A missing or
ineligible right neighbor permits non-Roll edits when Roll is zero.

The [timing foundation](TRIM_DRAFT_FOUNDATION.md) resolves all four values together.
Exact resolution does not clamp. Interactive nudging changes and clamps only the
active accepted value; switching policy preserves all values or refuses. The
command consumes accepted values, not a series of saved scalar edits.

`SourceTrimResources` supplies exactly the required fresh target/right wrappers,
complete Split identities, at most two silent filler identities and an optional
audio timing identity. The resolver reports these counts before allocation.
Timing identities belong to the command's new revision. Missing, extra, reused
or colliding resources fail before writing history. Capacity checks include
complete temporary Split copies before removal, even when the final tree fits.

## Ripple and overwrite

For old A output `[T,U)`, old B `[U,V)` and accepted I/O/S/R, Ripple makes A
`[T,U+O+R-I)` and B `[U+O+R-I,V+O-I)`. Later content moves by `O-I`.
Slip changes the linked media maps without changing picture time.

Overwrite retains the selected Sequence and all ancestor/project durations.
A authors `[J=T+I,E=U+O+R)`. An eligible B retains only
`[max(T,U+R,E),V)` when nonempty, keeping its complete hidden physical context
behind that final crop. Its earlier hidden extension cannot consume left-hand
neighbors. A surviving B crop may contain only endpoint padding; this is
different from a fully removed B.

The affected interval is
`[min(T,J),max(U,E,clamp(U+R,T,V)))`. Without an eligible B it is
`[min(T,J),max(U,E))`, with Roll zero. A overlays this interval; B keeps its final
tail; uncovered parts become fresh silent Background Holds. Existing content
outside the interval stays in place. Pure Roll has no filler gap.

For A `[10,20)` and B `[20,40)`:

| Accepted change | Final output |
| --- | --- |
| I=5, O=20, R=-15 | Silence `[10,15)`, A `[15,25)`, B `[25,40)` |
| I=0, O=-5, R=5 | A ends at 20, silence `[20,25)`, B `[25,40)` |

Partial neighboring composites use the existing complete-context Split path.
The duration-preserving refined tree becomes the baseline before growth and
removal. This preserves copied sound permissions, binding history and mark
lineage for one final reconciliation. Near-edge empty structural children move
with that edge; far endpoint empties stay; strictly interior empties retire.

## Audio, marks and reversibility

Capture the old tree once. Ripple keeps target, separately rolled B and suffix
reanchor groups disjoint. A retained overlap supplies its old entry phase;
disjoint earlier/later selections use the historical closed Source Start/End.
Positive Roll anchors B at its retained old `U+R`, not its removed old start.
Physical prefixes rebase existing owner clocks once. Marks receive both Source
prefix translations in one final transform.

The root sound bus receives one normalized `RootSoundOperation::Trim` derived
from entry In/Out values. Equal nonzero In/Out may remove and insert sound time
while preserving total duration. Overwrite restores its existing root sound
mapping; new silent fillers grant no sound permissions. Surviving composite
permissions and complete Preserve contexts remain part of the structural edit.

One outer transaction contains the complete patch and inverse. No intermediate
scalar Trim, Slip, Roll or Split is saved. The store assigns never-reused
revision IDs to commit, Undo and Redo.

## Native Trim

Select an eligible Source or neutral unity Source Partition in an ordinary
Sequence in **Your edit**, then press **`,v`** or enter **`:trim`**. A Partition
is a Source fragment that retains its full hidden context. Original, catalog
Sounds and Placed sounds cannot open Trim. Clear any active or retained Edit
Visual range first, including an empty range. Enter an ordinary group before
opening Trim to target its direct child. Roll needs an eligible literal right
neighbor; the panel explains when it is unavailable.

`,v` opens once without a count. Holding the activation key cannot open it
again. The pending comma hint includes `v Trim`; plain `v` remains Visual
selection. Entry captures the target, scope, revision, both cursors and literal
right neighbor before stopping audition. Later navigation or replies cannot
supply a different target. Bare `:trim` starts on In with all four values zero
and Ripple policy.

```text
:trim
:trim edge=out delta=-3f mode=ripple
```

The parameter form requires `edge`, `delta` and `mode` exactly once each, in any
order. `edge` accepts `in`, `out`, `slip` or `roll`; `mode` accepts `ripple` or
`overwrite`. `delta` is a signed or unsigned ASCII whole-frame integer with an
`f` suffix, such as `-3f`, `+5f` or `7f`. Missing, duplicate or unknown arguments
fail. It initializes only the named amount; the other three remain zero.

These keys act when the Trim heading or background owns input:

| Key | Action |
| --- | --- |
| `Tab` / `Shift-Tab` | Cycle In, Out, Slip and Roll forward/backward, preserving all four values. |
| `h/l` | Nudge the active amount by −1/+1 project frame; Shift changes −10/+10. |
| `r` | Toggle Ripple/Overwrite for the complete draft, preserving all values or reporting a refusal. |
| `i/o` | Select In/Out; while Slip is active, choose its inspected edge and retain Slip. |
| `b` | Compare Before/Proposed at the inspected junction. |
| `e` | Focus the native amount field, which uses the same whole-frame grammar as `delta`. |
| `Space` | Audition, pause or resume the inspected junction at the heard position. |
| `Shift-Space` | Restart a context loop from its beginning. |
| `Enter` | Apply one nonzero edit when all input is acknowledged and the current Proposed pair is displayed at the current viewer size. |
| `Escape` | Cancel and restore entry context before saving starts. |

Only `h/l` repeats while held, including Shift steps. Tab and all other actions
require a fresh key press; Trim has no count prefix. Native fields and buttons
retain Tab and activation. Enter in the amount field accepts text and returns
to the Trim controls without applying on that same event. Plain Escape cancels
from native fields/buttons outside composition; IME owns Enter and Escape while
composing. Command, Control and Option chords remain reserved. See
[keyboard compatibility](KEYBINDING_COMPATIBILITY.md#combined-trim).

Native Tab navigation also reaches the feedback viewport. Its visible focus
ring identifies when Up/Down, Page Up/Down and Home/End scroll the details.
These keys preserve the accepted draft and picture pair. Tab leaves feedback;
Escape still cancels Trim. The waveform and feedback stay bounded at the
minimum window size so the boundary pictures remain visible.

The outgoing/incoming picture pair stays fixed during audition. Its accepted
pictures and labels remain together while a newer proposal prepares. The
waveform and temporary heard position belong to the inspected Before/Proposed
context; neither moves the ordinary Edit or Original cursor. Space resumes a
paused loop; changing the draft or inspection resets its loop and resume state.
Playback failures stop audition and appear at the top of feedback without
manual scrolling. They preserve the accepted picture pair, draft and Apply.

Trim excludes ordinary editing, history, other editing drafts and Render until
Apply or Cancel. Apply saves the complete accepted intent in one transaction;
zero intent saves nothing. A saved receipt remains independent of preview
refresh, so a refresh failure reports that the edit was saved and needs reopening.
The separate **`:slip +5f`** stopped-picture preview remains supported with its
own inspection keys; it does not gain Trim's Tab cycling or audition.

## Host admission and headless access

The store validates A's immutable measured Source receipt, Original ownership
and complete picture/audio spans. It checks eligible B whenever Roll or the
overwrite overlay uses B, including a fully removed B. An unused diagnostic
neighbor does not gain a media requirement. The core owns final tree, allocation
and wrapper validation; the host owns stored media admission.

`ProjectStore::preview_source_trim_edit` and commit share the same admitted
command path. Zero intent validates its empty resource pool, A and unused new
revision, returns no edit and reserves nothing. Authoring an unchanged command
is refused. CLI dry runs return `source_trim_edit` resolution with the exact
optional transaction; both standalone and owned-project requests use the shared
dispatch. A serialized resolution supplies no authority to skip validation.

Core schema 43/database 52 identify the new command vocabulary. The approved
unused-project policy refuses development database schemas 39 through 51
without migration. This backend increment does not complete DP-02, DP-05 or a
product gate.
