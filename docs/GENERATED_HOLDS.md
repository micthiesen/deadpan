# Authored generated Hold foundation

Core schema 6 retains the schema-5 generated Hold intent and exact retained sampling metadata.
It does not establish that a candidate has passed media validation or been auditioned
in the application. Generic project creation and command ingress
reject new generated artifacts with `GeneratedAcceptanceUnavailable`. The dedicated
[store acceptance API](GENERATION_ACCEPTANCE.md) binds a selected Ready receipt,
verified dependencies and an undoable edit. The native application remains a shell.

## Authored representation

`HoldVideo::Generated` contains an immutable `GeneratedArtifact` and a captured
`HoldFallback`. The fallback is Background or a source Freeze, never another
generated provider. The artifact names two immutable video-only assets: the sampled
master and the original native sequence. Each has a strict algorithm-tagged BLAKE3
object reference and exact byte length. A third object reference retains the
provenance manifest. Pure identity types live in `deadpan-core`; the storage module
re-exports them without changing their JSON wire format.

`BridgeSamplingMap` version 1 retains project/native rational rates, original output
count N, native sequence count M, and the explicit encoded-sRGB RGB8 linear half-up
interpolation policy. For output frame j, the exact native position is
`(j+1)*(M-1)/(N+1)`. The compact formula is the sampling map; seeking does not allocate
N entries. Both conditioning boundary frames are excluded from output sample positions.

Document validation requires the map's project rate to match the document, a
positive Hold duration no greater than N, exact N/M asset frame counts, matching
content identities, video-only masters, and a valid fallback. Native/container time
bases remain explicit. The Matroska millisecond clock does not replace the exact
sampling map or frame ordinal. The core cannot inspect manifest contents or prove
that media bytes implement their declared metadata.

## Commands and frame reuse

`AcceptGeneratedHold` atomically registers up to two referenced immutable asset
records and replaces the visual provider. It captures the existing Background or
Freeze fallback, or preserves the fallback from an already generated provider.
Legacy `Accepted` providers have no captured fallback and must first be changed to
an explicit Background/Freeze. Unrelated asset insertion and replacement of an
existing asset record are rejected. `RevertGeneratedHold` restores the captured
fallback. Both commands support occurrence isolation and exact inverse patches.
Neither changes Hold audio or downstream source coordinates.

Shortening a generated Hold retains the original artifact and sampling map. Its
visible interval is the sampled master's prefix `[0, duration)`. Re-extending up to
N reuses those retained frames. Extending beyond N restores the captured fallback
in the same duration edit. The host must separately reconcile context and allocate
a replacement request; this core operation does not launch a job.

The picture plan requests sampled-master frame ordinals and keeps compiled revisions
immutable. It never resamples the original map after a duration change. Reversion
does not delete asset records or files; inverse history retains the full provider.
History-aware media reference accounting and cleanup remain unimplemented.

## Storage and migration

Database schema 11 stores core schema 6 while retaining the operational generation
tables introduced in database schemas 5 and 6, plus separate
[modern bundle receipts](GENERATION_BUNDLES.md) with optional admission evidence.
Database schemas 1 through 10 migrate through
complete chronological replay on a consistent backup. Frozen core-schema-1-through-4 adapters reject
new generated providers and commands even when nested in old subtrees, gaps, or
patches. Those asset records also retain their original SHA-256-only contract;
the newer BLAKE3 vocabulary cannot enter an old document, command, or patch.
Every old snapshot and forward/inverse transaction is compared during
replay. Existing requests, clocks, attempts, candidate receipts, and selection rows
remain unchanged. Interrupted-job recovery occurs only on a subsequent writer open.
Schema-7/8/9/10 history uses the frozen core schema-5 adapter; old Source nodes
gain explicit `fit_beat` audio mappings without changing generated Hold semantics.
Old requests gain no inferred bridge plan and old receipts gain no admission evidence.

Generic store ingress examines the resulting authored providers, including Repeat
gaps and override-owned nodes. It can retain or copy an already present artifact,
resize it, revert it, and navigate existing history. It cannot introduce a new
artifact through a command or initial project snapshot. Direct `SetHoldProvider`
also rejects Generated in core; acceptance has its own typed operation. Read-only
history inspection does not depend on a model or reopen worker output paths.

## Verification and remaining integration

Core tests cover atomic asset/provider changes, inverse restoration, occurrence
isolation, map validation, resize reuse, fallback, and strict legacy vocabulary.
Picture-plan tests compare every reused frame and the first following source frame.
Store tests check generic ingress rejection and resize/revert/undo/redo across reopen
using an explicitly synthetic preexisting authored fixture, not playable media.
Migration tests use a genuine schema-6 database produced by commit
`5100432be7b95600d88179254624dd9d78096714`, including pending redo, nine requests,
ten attempts, evicted and selected receipts, failures, cancellation and interrupted
states. Corrupt history and operational metadata must preserve the original and backup.
Review caught the legacy asset-hash vocabulary gap. A regression that adds an
otherwise valid BLAKE3 asset to every old snapshot reproduced erroneous promotion
before the fix; old-schema asset parsers and projections now reject it.

On 2026-09-21, the full repository gate passed on Apple Silicon macOS with pinned
Rust 1.97.1: formatting, workspace Clippy with warnings denied, 268 Rust tests
(none failed or ignored), workspace build, and `deadpan-cli doctor` reporting
database schema 7 and core schema 5. The audio, model, and FFV1 report suites passed
20, 47, and 5 Python tests. An obsolete migration test expectation of schema 6
was corrected before the full passing run. Native startup, GUI, inference and
actual media-rendering checks were not repeated for these pure semantics.

Candidate-to-master conversion, provenance binding and selected-Ready/relevance
checks now support explicit durable store acceptance. Audition, exact source/color
context, application acceptance, model-independent rendering and portable copy
remain open. [Generated-object storage](GENERATED_MEDIA.md) supplies byte ownership;
[the acceptance contract](GENERATION_ACCEPTANCE.md) describes the integrated boundary.
