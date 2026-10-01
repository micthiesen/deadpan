# Combined Trim foundation

Full [Trim mode](spec/DEADPAN_SPEC.md#77-trim-mode) needs one temporary draft
and one Enter transaction across In, Out, Slip and Roll. This work prepares its
shared geometry and audio timing. Native controls, overwrite structure,
boundary pictures, waveform and audition remain required.

## Current implementation

`SourceTrimIntent` retains four signed accepted frame values and an explicit
Ripple/Overwrite policy against the entry document. `SourceTrimControl` selects
which value a nudge changes. The active display control is separate from the
intent, so Tab cannot change the proposed edit.

`source_trim_geometry` resolves the complete intent exactly. A policy toggle
preserves all four values or refuses with its blocking constraint.
`adjust_source_trim_geometry` validates the accepted intent, changes only the
active value and clamps it once against the full proposal. It returns the
applied value and exact bound. Reversing after a clamp starts from that value;
rejected overshoot is discarded.

The target is a qualified Source or one unity Partition under an explicit
ordinary Sequence. Roll uses its captured literal right neighbor. Missing or
ineligible neighbors leave In/Out/Slip available when Roll is zero. The geometry
report supplies availability and does not infer a new neighbor from a cursor.

For entry target allocation `[a,b)`, output `[T,U)`, and right allocation
`[c,d)`, output `[U,V)`, accepted values I/O/S/R resolve together:

| Quantity | Result before physical-prefix translation |
| --- | --- |
| Target allocation | `[a+I,b+O+R)` |
| Target media placement | Both linked maps shift by `-S` |
| Right allocation | `[c+R,d)` |
| Ripple target output | `[T,U+O+R-I)` |
| Ripple right output | `[U+O+R-I,V+O-I)` |
| Ripple suffix | Moves by `O-I` |
| Overwrite target output | `[T+I,U+O+R)` inside the selected scope |
| Overwrite right output | `[U+R,V)` before structural overlay |

Source owners only grow; exact fractional pads, hidden selection context and
independent audio offsets are retained. Arithmetic and physical storage overflow
fail explicitly. The minimum Source-wrapper budget is separate from future
overwrite refinement identities.

Overwrite geometry keeps the selected Sequence, ancestors and project duration
fixed. Its right interval is a proposed placement before overlap resolution.
The report does not authorize splitting or consuming neighbors, nor does it
construct silent filler. Those require a separately qualified structural edit.

## Timing prerequisites

Each owner report identifies the overlap between its entry and final unprefixed
allocations. A fully earlier selection reports the old closed start; a fully
later selection reports the old closed end. An explicit Source endpoint binding
preserves the historical sample phase when no allocation overlap remains.
Its closed-end sample
boundary must be `B(end)`, and virtual boundaries may be signed. Dormant audio
and an empty structural allocation are different cases. Slice capture retains
endpoint history before appending its own allocation-entry step; the final
resume can therefore be at local zero while preserving a nonzero phase.

The independent root sound bus needs one operation derived from entry In/Out
intent. Slip and Roll alone leave that bus fixed. Equal nonzero In/Out values can
remove and insert sound time despite unchanged project duration. The integrated
`RootSoundOperation::Trim` retains up to three old intervals and inserts their
complementary gaps. Adjacent retained intervals with the same translation merge
before sample origins and fades are calculated. Routing, retained sample support
and edge envelopes share this checked projection. It preserves previous route
history and uses the logical selection only when the original event allocated
no samples. Exhausted sampled support cannot reappear through that fallback.

For old root `[0,D)`, target `[T,U)` and `K=O-I`, the retained intervals are
`[0,T)` unchanged, `[max(T,T+I),min(U,U+O))` shifted by `-I`, and
`[max(U,T-K),D)` shifted by `K`. Clip to the old root, omit empty intervals and
normalize before deriving gaps and cuts. A globally identity projection restores
the exact previous sound objects without appending a journal entry. Equal In/Out
alone does not establish identity. Sequential saved scalar edits do not implement
this complete-intent policy.

## Verification status

The [verification record](qualification/trim-foundation-2026-10-01.md) covers
independent source review and focused runtime checks for all three primitives,
including 57 new tests across core, indexed planning and decoded PCM. Test-fixture
and oracle corrections are retained in the verification record. The complete
workspace passed 3,333 unit/integration tests and both documentation tests.
Strict workspace/all-target lint and final formatting passed on the same
unchanged source inventory. Core schema 42/database 51 identify the new persisted
timing vocabulary; unused development databases 39 through 50 are refused
without migration.

No complete combined authoring command or native Trim interaction is claimed.
DP-02/DP-05 and all product gates retain their current partial/open status.
