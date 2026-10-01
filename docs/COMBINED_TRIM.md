# Combined Source Trim

`ApplySourceTrim` combines accepted In, Out, Slip and Roll values into one
reversible backend edit. [Qualification](qualification/combined-trim-2026-10-01.md)
records passing workspace, focused media, durable history and strict lint checks. Native
[Trim mode](spec/DEADPAN_SPEC.md#77-trim-mode), boundary pictures, waveform and
audition remain separate integration work.

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
