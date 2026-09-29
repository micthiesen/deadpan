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
[SDR encoder pixel boundary](SDR_ENCODER_PIXELS.md) requires even output
dimensions; automatic codec geometry and its aspect-preserving normalization
remain a separate output policy. This preparation API does not silently change
the document or normalize its raster.

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

## Explicit limits

This path supports qualified original Source/Freeze pictures and opaque black
Blank/Background. HDR presentation, Still images and accepted generated media
return explicit errors. An accepted object's existence is not a qualified
decoder/index binding. An unsupported provider may be encountered after earlier
frames succeed; a future export worker must fail the job and preserve the
destination rather than publish a partial output.

The implementation supplies no final-render process isolation, audio mastering,
muxing, encoded-file verification, atomic publication or native Render control.
The approved AAC timing metadata policy still requires complete emitted-file
verification. These remaining steps are required by the full specification.

## Verification entry point

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
