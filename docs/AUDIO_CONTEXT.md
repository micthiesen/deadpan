# Retained audio contexts

`FrozenAudioContext` retains the media-bearing body of one immutable audio
context. It combines a [frozen timing layout](AUDIO_REFERENCE.md) with exact
Source, RoomTone and Tail inputs and their full immutable asset records. The
standalone wire uses schema 1. Core 15/database 21 remain unchanged; live sample
bindings and the arbitrary-boundary Hold command are still required.

## Capture and admission

Capture retains the complete processing tree, compact Repeat order, sparse
overrides, exact Retime selections and pitch policies, transparent Partitions,
audio edges and copy lineage. Source inputs retain their original selected span,
`SourceAudioMapping` and signed `audio_offset`. Their effective placement must
agree exactly with the frozen layout. Source offsets must not be normalized into
rounded frame positions. Even exact normalization into `Placement` is not always
valid: a supported mapping plus offset can place its effective origin beyond
that variant's explicit i64 frame-coordinate range.

Each Source with audio has one Source input. RoomTone and Tail Holds or Repeat
gaps have one Hold input. Silence and Sources without audio have none. The asset
inventory is exactly the set referenced by those inputs, including sparse
overrides. Full asset records retain qualification IDs, content identities and
stream bounds. Picture-only assets, picture providers, live node labels, marks
and presentation policy are omitted. No generated picture objects or model
runtime are needed to interpret these audio facts.

The closed JSON root, exact input/asset inventory, selected stream ranges and
node/policy agreement are validated. The complete wire is bounded to 64 MiB;
input and asset counts use the existing document limits. A borrowed raw layout
passes through its existing streaming preflight before materialization. Each
borrowed raw input is bounded to 64 KiB before its tagged record is parsed.
Captured layouts retain their 100,000-node, edge and aggregate compact-run
limits. No rendered Repeat expansion is performed. A valid serialized context
is still authored intent, not a media-admission token.

## Direct audio compilation

`RenderPlan::compile_audio_context` compiles the retained tree directly. It does
not create an authored document with invented picture sources. A picture-only
Source remains a Source with absent audio; changing it to a silent Hold would
incorrectly suppress processed decay after Preserve and alter its edge policy.
The complete intrinsic source, RoomTone and Preserve contexts remain available
under visible crops, with the original clock and stable occurrences.

The resulting plan rejects all picture evaluation with `AudioOnlyContext`. Its
neutral 16×16 SDR geometry exists only to satisfy shared plan metadata, and must
never be used as an export or presentation basis. Audio allocation and preparation
use the captured frame rate. Tail intent remains explicit and continues to fail
in readers that do not support Tail DSP.

`AudioSourceProvider::source_for_context` is a separate admission call carrying
the expected full `AssetRecord`. Its default rejects the request. An ordinary
revision-aware provider cannot silently treat arbitrary serialized context as
qualified media. Both audio readers use this route, including recursive Preserve
inputs, RoomTone preparation and prepared-stage cache validation. Existing
ordinary plans continue to use the original provider method. Source contracts
must be checked before PCM is returned; matching aliases or revision names alone
are insufficient. Caches remain scoped to one immutable plan and retain their
existing work, residency, deadline and provenance checks.

## Project host

`ProjectStore::snapshot_at` reads an immutable committed revision, including an
abandoned branch, without moving the history cursor. The headless host's
`ProjectAudioSession::open_context` resolves that revision and compares the
complete supplied context with a fresh capture of its retained document. It
rejects altered timing, inputs or asset contracts even when the claimed project
and revision names match.

For every source request, the host compares the expected retained asset record
with that immutable document, then follows the existing receipt and verified
original-byte path. Reopened audio indexes must match their complete qualified
evidence. Historical source identity therefore survives later edits and reuse of
an asset alias. A missing historical revision or qualification is an explicit
error. This host path currently requires retained project history; the standalone
context does not by itself make a bare JSON snapshot a portable media package.
The host remains read-only, keeps one bounded private source cache and performs
all media preparation off the UI and device callback.

## Remaining insertion work

[Physical-domain reads](AUDIO_PHYSICAL_DOMAINS.md) now render full context outside
visible Partitions on the original signed root grid. They seed the physical
subtree rather than selecting an unrelated sibling through the global root.
The corresponding point-grid transfer retains this domain and exact phase.

This body can supply actual retained raw PCM through the existing source,
RoomTone, Preserve, envelope and root-to-point conversion paths. It does not yet
bind a live occurrence to that PCM. An inserted-time transaction still needs
exact structural/sample anchors, composition across earlier cuts, compact Repeat
birth and edit rules, replacement of changed audio/policy contributions and
genuine seam envelopes. Old and current explicit silence must both be respected.
The [splice design](STRUCTURAL_SPLICE_DESIGN.md) records those constraints.

No GUI behavior or ImageGen design target changes in this layer.
