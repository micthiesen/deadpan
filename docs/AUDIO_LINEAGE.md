# Audio lineage across structural copies

Core schema 15 persists an optional `audio_lineage` map from owned physical node
IDs to `AudioLineageId { allocation, origin }`. Split and automatic occurrence
isolation establish these relationships as part of their normal atomic edit.
This supplies copy lineage for later inserted-time bindings. It does not yet
persist sample-resume anchors or implement arbitrary-boundary Hold insertion.

## Identity and ownership

The allocation is the copying transaction's revision; the origin is an opaque
historical node name. It is not a live node reference. The original physical
owner can be deleted while surviving copies retain the same lineage. An imported
initial document reserves its lineage allocation revisions for the store's
entire lifetime, even after all current owners disappear.

A first transparent copy seeds both retained originals and copies. Further
copies inherit existing tokens, so refinement does not allocate a new family
at every transparent seam. Root Split also transfers the old payload's lineage
to its retained left context. Repeat orders and stable play IDs stay compact.
Unrelated imports do not acquire relationships by matching media or timing.

This is authored copy provenance. A token alone is not evidence of equal current
PCM, source qualification, a particular sampling clock or permission to read
media. Actual continuity bindings must resolve the live recipes, retained
processing context, exact map and explicit policy. Map keys must belong to the
current owned tree; values remain historical names. The map is bounded by the
100,000-node document limit, uses unique closed JSON records and participates in
document validation.

## Edit lifecycle

Every edit and inverse includes lineage changes in its guarded `DocumentPatch`.
`changed_ids` includes a retained original whose only change is its new lineage.
Undo restores the exact earlier map; durable history still allocates a fresh
revision rather than recapturing relationships.

Split, Group and Ungroup preserve the raw signal through their structural
changes. Occurrence isolation first copies lineage transparently, then applies
the actual node edit against that isolated state. It does not mistake the newly
created override for an audio change by itself.

Changes to Source audio selection, placement or offset, Hold audio/duration,
Repeat order/gaps/overrides, or other raw structural contributions detach the
changed contexts and their ancestors. Both old and new parent chains participate
when content moves. Unchanged local sources and other copied branches retain
their relationships. A later copy of a detached context receives a fresh token.
This does not retain an obsolete source mapping or silence policy forever.

Labels, picture mappings/providers, marks, canvas geometry and postmapping audio
edge choices do not alter raw audio lineage. Deleting, ungrouping or replacing
owned nodes removes their map keys; surviving tokens are not discarded merely
because they currently have one owner. The ancestor walk visits each affected
node once over the bounded union of two validated trees.

## Frozen reference use

`FrozenAudioLayout` captures the optional map with its timing aliases. Existing
serialized reference layouts without lineage remain valid and gain an empty
map. Admission checks bounded unique aliases and closed token fields before
materializing them; a value's origin never resolves against live structure.

The reference plan retains physical `same_domain` identity separately from its
copy-lineage comparison. A compatible comparison needs the same admitted plan
and owning clock, explicit related aliases, matching stable Repeat paths/gap
identity, processing kind and exact meaningful placement. Numeric grid or media
equality cannot create the relationship. These checks remain timing/provenance
information; they do not authorize binding a decoder or a different live edit.

## Migration

Database schema 21 stores core 15. Database 20 replays through a frozen core-14
grammar, including direct and occurrence Split with closed identity pools.
Every older initial snapshot gains an empty map. An initial snapshot containing
similar partitions is not sufficient evidence of historical copying.

Migration replays actual historical commands into the modern implementation.
Copying commands establish lineage at their historical allocation revision.
Strict legacy projections compare every old snapshot, forward/inverse patch,
description, duration and changed-ID summary. Only the newly calculated lineage
is omitted from those projections; old JSON rejects the new field even when
empty or null. Old changed-ID summaries remain the exact sorted union of their
projected node/override changes.

The complete calculated modern transactions and snapshots are then retained.
Historical undo/redo uses those rewritten patches, so it restores lineage too.
The migration keeps its original backup, validates all chronology and preserves
workflow profiles and operational rows before SQLite promotion.

[The splice design](STRUCTURAL_SPLICE_DESIGN.md) and
[reference contract](AUDIO_REFERENCE.md) track the remaining authored sample
bindings, policy replacement, carrier-grid transfer and Hold command.
