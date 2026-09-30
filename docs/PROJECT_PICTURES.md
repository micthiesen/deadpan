# Committed project pictures

`deadpan_cli::picture::ProjectPictureSession` prepares exact pictures from one
explicit committed revision. It opens a read-only store, captures that document
and compiles its immutable `RenderPlan`. Live edits, undo, redo and writer close
cannot change its picture selection, framing or revision identity. This host
boundary creates no GPU, thread, process, encoder or output file. Call it on a
preparation worker, away from the UI and audio threads.

## Time and geometry

`open_revision` accepts an optional nonempty half-open project-frame range.
Omitting it selects the complete committed duration; empty projects, negative
ranges and frames outside the captured range fail. A prepared result carries
project ID, revision ID, exact project frame, rational frame rate and unchanged
committed canvas. Original source ordinal and signed source PTS remain separate.
Generated pictures carry their complete accepted artifact identity and sampled
master ordinal separately from Original receipt-bearing frames.
The plan selects the measured source frame through cuts, repeats, freezes and
retimes; neither nominal source frame rate nor a rounded seek substitutes for
its mapping.

Provider-to-root framing, identity clips, default Repeat-gap scope and captured
Hold context remain intact. The native preview and this session use the same
pure decoded-frame and framing adapters. Send their result to
`PictureRenderer::render_composed`; do not reconstruct framing in an encoder.
An authored Blank or Background instead uses `render_background`, which clears
both working and display targets to opaque black on the same renderer queue.
It is never a fallback for failed media. Foreign targets and an outstanding
picture submission are rejected through the usual renderer admission rules.

The session preserves legitimate odd committed canvas dimensions. The
[encoder picture host](EXPORT_PICTURES.md) owns legal output geometry and exact
output timestamps, then uses the [SDR pixel boundary](SDR_ENCODER_PIXELS.md).
It keeps this unchanged authored canvas for framing before the final raster
mapping. This preparation API does not change the document or normalize its raster.

## Original admission and ownership

Source and Freeze providers resolve the receipt by `(captured revision, asset)`.
The immutable asset must name that receipt and original content. Admission
checks the retained original object, SHA-256, byte length, selected stream,
complete source interpretation and measured frame index against fresh decoding
of a private verified source snapshot. The index comparison includes terminal
endpoint evidence and checks cancellation between bounded chunks.

One session retains at most one source decoder, index and private input. A
source switch releases the previous session before a new admission. Repeated
reads may use the admitted immutable bytes even if a linked external path later
disappears or changes. Cold admission rejects missing or changed originals.
This cache does not promise offline access to every asset in a compatibility
project. The caller owns and bounds completed RGBA results independently.

Decoder byte, frame-index, raster and cooperative timeout limits apply before
or during admission. Cancellation is checked around store and media work;
synchronous store validation and individual native calls cannot be preempted.
A supervised worker still needs an external deadline, bounded result retention
and stale-job rejection. No writer lock, authored transaction or operational
recovery is acquired by this read-only session.

## Accepted Generated Hold admission

`Picture::Accepted.generated` carries the effective compiled provider's complete
`GeneratedArtifact`, shared across seeks. Default Repeat gaps and sparse play
overrides retain their own provider identity. Legacy Accepted providers carry
no invented artifact evidence. The reader uses sampled-master ordinals directly;
shortening and re-extending within the available range reuse the stored prefix.

`open_generated_picture` is the shared cold reader for native preview and fixed
revision preparation. A revocable, package-anchored store handle gives its worker
access without borrowing the writer or SQLite connection. Every cold admission
requires all six retained objects: native master, sampled master, host provenance,
context manifest and both conditioning inputs. Private snapshots verify object
length and BLAKE3; the conditioning inputs also match their retained SHA-256.

The models crate strictly parses bounded schema-3 host provenance and revalidates
its retained provider capability, bridge plan, worker declarations, context,
conversion reports and measured spans against the authored artifact and project.
It never opens old worker paths or requires an installed model. Current request
relevance, candidate availability, selection, allocation revision and current
Hold identity do not replace the durable authored acceptance. Historical reads
remain valid after request staleness, undo, copying and provider reversion.

Both immutable asset records must agree with the retained object, video-only
span and frame count. Fresh decoding of the sampled FFV1 master verifies square
pixels, no rotation/audio, full-range RGB8/sRGB/BT.709, every expected millisecond
PTS and exact terminal duration from the final decoded frame. No nominal frame
rate is substituted for observed endpoint evidence. The native master is hash
verified as a retained dependency; the sampled master supplies the picture.

Cold reads share one cooperative 300-second deadline, a 16 GiB per-master byte
bound, 32 MiB host provenance, 1 MiB context and 64 MiB per conditioning input.
Contract frame/raster limits and native decoder bounds also apply. Only one
decoder is retained across Original/generated switches. Native reuse keys the
project session, full artifact, both asset records and color policy; ordinary
revision and framing changes reuse its private immutable bytes. Missing or
corrupt package objects fail cold admission while already admitted bytes remain
usable. Closing the owning store revokes future preparation, including warm
decoder requests. Previously returned owned pixels remain independent.

## Explicit limits

This path supports qualified Original Source/Freeze pictures, schema-3 Generated
Holds and opaque black Blank/Background. HDR presentation, Still images and
legacy Accepted providers without qualified generated evidence return explicit
errors. An object's existence alone is not a qualified decoder/index binding.
An unsupported provider may be encountered after earlier
frames succeed; a future export worker must fail the job and preserve the
destination rather than publish a partial output.

The implementation supplies no final-render process isolation, audio mastering,
muxing, encoded-file verification, atomic publication or native Render control.
The approved AAC timing metadata policy still requires complete emitted-file
verification. These remaining steps are required by the full specification.

## Verification entry point

The real `bundle_qualification` integration test independently checks all 30
sampled RGBA frames and PTS after canonical conversion, explicit acceptance,
relocation and removal of worker files. It also exercises historical reads,
shortened prefixes, re-extension, undo/redo/revert and cold missing/corrupt
dependencies versus an already admitted private decoder.

To retain that test's actual package for native replay, set
`DEADPAN_GENERATED_PICTURE_FIXTURE_ROOT` to a new absolute directory under an
existing temporary scratch parent when running
`real_bundle_acceptance_is_explicit_durable_and_reversible_after_relocation`.
After completing its assertions, the test restores the accepted 30-frame head
through undo, closes every store and reader, and moves the complete package to
`ROOT/accepted.deadpan`. Its adjacent bounded expectations file records the
independent pixel/PTS oracle. No live SQLite main file is copied.

The optional app harness consumes that package with
`--ui-check --scenario generated-picture --project ROOT/accepted.deadpan --output NEW_DIRECTORY`.
It preflights the fixture read-only, then uses production Open, `:sequence`,
frame navigation and resize with real decoding and Metal submission. Visual
mode captures the compatibility workspace; separately built release performance
mode measures cold admission and warmed navigation without screenshot readback.
This 4×2 fixture is evidence of routing and exact pixels, not representative
video throughput, AI inference, an acceptance UI or single-Original creation.
Ordinary replay without the explicit fixture reports this scenario as skipped.

The [Generated picture qualification](qualification/generated-pictures-2026-09-29.md)
records exact decoded-frame checks, the full workspace gate, release Metal
replay, inspector image review and separate cold/warm measurements. The Hold
inspector keeps Duration, Picture and Sound together, with the duration action
above secondary treatments. Schema-3 Generated providers display "Accepted AI";
legacy Accepted providers retain their distinct label and unsupported error.

Eight module tests exercise real qualified source bytes, historical receipts,
exact structural mapping, retained private input, malformed or unsupported
admission and the shared adapters without a native window. The separate
`qualify_project_picture` example renders a synthetic committed project on
actual Metal and retains complete I420 planes and their identities. It compares
repeated retimed pictures and explicitly captured freezes, verifies opaque
black gaps, and changes/undoes/redoes the live writer while checking the captured
session's pixels. It also proves a new Source pose changes the new revision's
picture while the captured Hold keeps its own geometry.

The example requires a new report path and new work directory. Its whole-run
60-second cooperative deadline must be paired with an external process timeout.
It writes a synced running report before work and final results on ordinary
completion; forced termination during a final report write is not atomic
publication. Use the exact Cargo-reported example executable, retain its hash,
command, source identity, output and terminal exit, and never select a binary
with a stale glob. These fixtures establish neither throughput nor complete
preview/export equivalence.

The [measured qualification](qualification/project-pictures-2026-09-29.md)
records all 31 Metal checks, 18 complete frames, workspace and optional app
coverage, independent review, and the failed fixtures with scoped corrections.
