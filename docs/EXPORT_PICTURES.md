# Committed pictures for the encoder

`deadpan_cli::export_picture::ExportPictureSession` converts one captured
`ProjectPictureSession` into owned, timestamped SDR I420 frames. It uses the
same picture admission, framing, working texture and pixel conversion as native
preview and the existing qualification paths. Run it on a preparation worker.
It creates no thread, encoded file, authored edit or UI control.

## Captured output contract

The session owns its input reader, renderer, target and immutable
`ExportPictureContract`. It derives the contract from that reader's committed
project ID, revision, presentation basis and nonempty half-open range. A caller
cannot construct a contract from arbitrary deserialized evidence or swap a
prepared picture from another revision into this producer.

The committed basis remains authoritative. Source import has already derived
its practical raster and exact cadence; reusing source footage or reading an
accepted Generated Hold cannot change it. The output policy preserves even
canvases. For a legitimate odd compatibility canvas, each axis uses the shared
nearest-even rule: ties downward, minimum two. Each axis changes by at most one
pixel. The contract records the exact signed relative aspect error, which may
be a large percentage for tiny test canvases. It does not assert a universal
small percentage tolerance.

Authored framing still uses the original committed canvas, including odd
coordinates. The renderer applies captured context and provider-to-root clips
there, then maps that full canvas to the output raster. It adds no independent
source fit, crop or player-shaped border. The output is progressive,
square-pixel, limited-range Rec.709 with left-sited chroma as defined by
[the SDR pixel boundary](SDR_ENCODER_PIXELS.md).

Raster, pixel count, padded readback bytes and address-space bounds are checked
before target allocation. The renderer separately checks device limits. HDR
and unqualified providers fail explicitly; this SDR boundary supplies no tone
mapper or inference fallback.

## Exact independent clocks

`OutputFrameOrdinal` is relative to the captured range. For normalized project
rate `N/D`, the output time base is `1/N`, PTS is `ordinal × D`, each frame lasts
`D` ticks, and the exclusive output endpoint is `frame_count × D`. All arithmetic
is checked. Rates whose components exceed the encoder's signed 32-bit rational
representation and endpoints beyond signed 64-bit ticks fail admission.

Each result also retains the absolute project frame `range.start + ordinal`.
Original receipt/asset, source ordinal and signed source PTS remain provenance.
Generated pictures retain the complete accepted artifact and sampled-master
coordinate. Repeated source pictures therefore receive distinct output PTS;
source and Matroska millisecond clocks never determine the output timeline.
DTS and encoder reordering belong to the encoder integration.

The contract records both absolute 48 kHz boundaries `B(range.start)` and
`B(range.end)` independently from the common project origin. A diagnostic
nonzero range can have a different sample count from `B(range.duration)`.
This retains the information needed for later PCM integration; it supplies no
mastered audio or mux alignment claim.

## Work and result ownership

A synchronous `prepare` admits one output ordinal, decodes its exact committed
picture, renders to the session's target, drains bounded working readback and
converts to owned I420. Authored Background clears both targets to opaque black;
missing media returns an error. One private decoder and one target are reused.

Only one completed `ExportPictureFrame` may remain held per session. Preparing
another returns `OutstandingFrame` until the prior result is dropped. An encoder
can borrow its planes while consuming them. The result and its contract may
outlive the session. Explicit caller copies are outside this producer's memory
ownership. Failed work releases the completed-result budget and publishes no
frame; retries retain the requested output ordinal.

The caller supplies cancellation and a monotonic deadline. Checks surround
store/media preparation, GPU polling/submission, readback and conversion. Idle
polling parks for at most one millisecond between checks. Dropped or cancelled
readback keeps the renderer's allocation permit until both mapping and submitted
GPU work drain. A retry cannot accumulate staging buffers while those callbacks
remain outstanding.

These are cooperative bounds. They cannot preempt a SQLite call, native decoder,
driver call or bounded pixel conversion. The [render worker](RENDER_WORKER.md)
now supervises this producer in its own process with an external deadline and
checked teardown for bounded raw ranges.

## Required integration

This is the picture input boundary for an encoder. Product Render still requires
durable render jobs, the complete shared effects/audio graph, qualified
VideoToolbox/OS fallback encoding, approved AAC timing metadata, independent
emitted-file verification, atomic publication and the native workflow. Existing
legacy Accepted/Still and HDR failures remain explicit until those paths are
implemented and qualified. No requirement or release gate closes here.

[Qualification](qualification/export-pictures-2026-09-29.md) records actual Metal
Original/Hold/Background and Generated output, exact full/nonzero-range clocks,
odd-canvas mapping, retained-result limits and independent complete-plane
comparisons. The example reports an explicit Generated skip when its optional
accepted fixture is absent.
