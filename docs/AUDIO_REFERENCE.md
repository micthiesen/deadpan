# Frozen audio timing and policy clocks

`FrozenAudioLayout` captures timing and audibility independently of a later live
tree. `AudioReferencePlan` queries that snapshot in an explicit old clock, and
`RetainedRootPolicy` applies its silence to resumed PCM. These are checked
foundation APIs. They do not yet persist a Hold binding or implement an
inserted-time command. Core 15/database 21 now retain [audio copy lineage](AUDIO_LINEAGE.md).

## Retained facts

The flat, closed layout retains durations, edge policies, exact Source audio
placement including its offset, Hold/gap policy, compact Repeat order and sparse
overrides, and Retime mapping/pitch/purpose. Tail policy retains its maximum in
the original Hold's local frame units. No asset IDs, source media coordinates,
labels, picture providers, marks, or previous resume bindings are captured.
This snapshot cannot authorize media access or establish supported Tail DSP.

Node IDs are aliases scoped to the frozen layout. They are not references into
the current document. A later Split, reorder, shrink, move or deletion cannot
change their old placement. A complete `InstancePath` selects an effective play;
the default child of an overridden play is rejected. Gap scopes name the Repeat
and the stable preceding play. A final play has no following gap.

Capture and JSON admission share validation: unique closed maps, positive exact
placements, recomputed durations, owned tree closure, bounded depth/edges/bytes,
valid overrides and Retime/Partition invariants. A streaming preflight counts
collections before typed JSON materialization and retains no document-wide JSON
value. It distinguishes grammar fields from node aliases and rejects malformed
container shapes, unknown or duplicate record fields, and trailing data before
the typed pass can buffer them. The frozen format permits at most 100,000 nodes, 100,000 total
structural edges including overrides, and 100,000 total compact iteration runs,
with the existing 64 MiB JSON ceiling. Capture checks these counts before cloning
collections. The aggregate run cap is specific to this new API; a larger valid
live project can exceed its capture limit. One run can still represent a billion
plays. Cached parent and compact
Repeat indexes support bounded projection without expanding plays. Projection
allows signed coordinates outside a visible crop so a retained anchor is not
silently clamped to its current display range.

## Explicit clocks

Reference queries require a handle borrowed from one admitted plan:

| Clock | Support and sample allocation |
| --- | --- |
| Project root | Full old project, 48 kHz samples with absolute round-even boundaries |
| Preserve input | Selected child input range, point-ceil grid beginning at its exact selected origin |
| Preserve output | Full intrinsic stage output, point-ceil grid beginning at local zero |

Only a valid nonunity Preserve occurrence can own a preparation clock. A
`ReferenceSample` has meaning through its handle; a count or equal grid values
do not establish interchangeable signals. Query results retain that handle,
full allocation and exact extent, occurrence, gap identity, and policy class.
Retime descent is flattened for policy inspection, not for source rendering or
DSP. Span and traversal limits match the other audio query APIs.

## Retained and current silence

An integer root-output resume advances one old root sample per new output sample.
`RetainedRootPolicy` maps a bounded PCM block into that old policy clock, unions
retained explicit silent Holds with current explicit silence, and returns merged
zero ranges. Missing source audio and outside-placement regions do not force
processed output to zero: Preserve can spread neighboring energy into those
gaps. The query retains their classification, but this post-processing mask
preserves that decay, matching the existing source/stage readers.
It also zeros demand outside the old root domain. Wide arithmetic handles an
entirely exhausted request without narrowing an impossible old index. Validation,
reference lookup and cancellation checks finish before PCM mutation. PCM demand
and the current interval list are independently bounded to 256 entries.

At 30000/1001 fps, a silent Hold inside Preserve at output frames `[2,3)` starts
at old root sample 3203, but intrinsic prepared-point suppression begins at 3204.
Inserting 1f at f=1 maps new 4804 to old prepared 3203, while the new structural
Hold begins at 4805. The retained root mask closes that one-sample hole. Another
rounding phase starts current silence before the shifted retained mask; the
current mask must win there too. Tests reproduce both cases with actual canonical
Preserve DSP output, not only synthetic policy labels. They use generated stereo
PCM and do not qualify listening or recorded-speech continuity.

This consumer does not resample PCM, apply fades, mutate authored state, or
transfer a root clock into a newly introduced Preserve input grid. It is not yet
connected to an authored Hold command or the application transport.
The separate [sampled-root transfer](AUDIO_SIGNAL_TRANSFER.md) now implements
bounded conversion of already masked root PCM into a point grid. Authored
bindings must still supply the correct retained context and active map.

## Physical processing domains and root maps

`ReferenceAudioClock::processing_domain_at` resolves one allocated sample to a
borrowed `ReferenceProcessingDomain`. It shares the bounded Sequence/Repeat
descent with policy queries, but stops at the first nonunity Preserve. The
descriptor retains the full intrinsic selection, duration and authored rate;
an inner silent Hold is still available through the separate flattened policy
query. FollowSpeed and unity retimes remain transparent to processing lookup.

Visible allocation and meaningful processing extent stay separate. Partition,
Sequence and Repeat allocation do not shorten the retained context. Source
placement, Hold/gap duration, ordinary Edit crops and an explicit Preserve input
selection still constrain it. Both extents use the owning clock's boundary rule;
the local sample coordinate remains exact. No PCM, source identity or decoder
admission is derived from this timing information.

Handles can only be obtained from an admitted plan. Their physical identity
includes that plan, the clock owner, full occurrence, gap identity and meaningful
domain. Equal node names or equal grid values across two plans do not establish
identity. A root-only `place_root` rejects preparation-clock handles. It accepts
the domain's new meaningful-start sample, which the eventual host must resolve;
passing a query or transparent Partition start would establish the wrong phase.

`RetainedRootMap` preserves one old root sample per current root sample. Its
`resume` evaluates the current map at the cut before setting a new anchor. Each
later genuine domain uses its own old and new meaningful starts. At NTSC:

| Domain / edit | Current sample | Retained sample |
| --- | --- | --- |
| A 2f, insert 1f at f1 | 3203 | 1602 |
| Following B 2f, now starts at f3 | 4805 | 3203 |
| Second insertion inside a longer A, current f3 to f4 | 6406 | 3204 |

The map uses wide exact arithmetic for coordinates outside retained support.
It does not clamp, authorize reads, change rate or apply fades/policy masks.
`reference_position_at` also accepts an explicitly calculated fractional current
root position for a subsequent `RootSignalTransfer`; a `SignalSample` cannot be
passed as a root sample. The transfer still requires its explicit carrier grid
and admitted, masked PCM. Tests feed these composed coordinates through the
existing resampler and another real canonical Preserve operation.

## Remaining authored work

An insertion must bind live owned domains to frozen aliases atomically, compose
the current sampled coordinate at its cut, retain later domains' old starts,
and preserve full processing contexts. Split/refinement and occurrence isolation
must transform live bindings without rewriting frozen facts. Repeat growth,
copying, deletion, policy changes and pruning need explicit lifecycle rules;
changing a silence policy must replace its retained contribution rather than
mute that location forever. Cross-grid conversion must consume the correctly
bound context through the sampled-root adapter. Migration must freeze the previous grammar and give
legacy projects no invented bindings.

Split and occurrence isolation now persist [audio copy lineage](AUDIO_LINEAGE.md).
The frozen layout retains those explicit relationships separately from physical
alias identity. Compatible lineage, clock, occurrence and exact placement can be
compared without inferring a relationship from timing or media equality. The
binding layer still needs to carry an active resume through these related
contexts and replace changed contributions. Lineage alone is not a sampled
signal, a live binding or proof of equal PCM.

The [sampling contract](AUDIO_SAMPLING.md) and
[splice design](STRUCTURAL_SPLICE_DESIGN.md) describe the remaining composition.
[Qualification](qualification/audio-reference-2026-09-23.md) records tests and
review for this foundation. No product requirement or delivery gate is complete.
