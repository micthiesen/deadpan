# Plan and renderer framing handoff

Implementation is limited to deadpan-plan and deadpan-render. No commit, push, or whole-workspace check was performed by this owner. All 24 changed/untracked owned paths are pinned in `plan-render-final-hashes.json`.

## APIs and semantics

`PictureSample.framing: Vec<PictureFraming>` records every visited scope in provider-to-root order, including identities. Each `PictureFraming` exposes its `InstancePath`, exact `local_position`, owner `duration`, and evaluated optional `FramingPose`. Repeat gaps have no physical provider node, so the renderer caller prepends one identity provider scope. The Repeat scope's local framing clock remains the whole Repeat clock.

`PictureRenderer::render_framed(frame, target, canvas, mode, layers)` separates committed canvas dimensions from output raster. `render()` remains an identity convenience. `PictureGeometry::framed(metadata, canvas, raster, mode, layers)` is the shared CPU/GPU geometry; `reference_pixel_with_geometry` is its CPU pixel entrypoint. `FramingLayer::{identity,new,pose}` is independent of envelope storage. `source_to_input(index, upright)`, `source_steps(index)`, and `source_to_canvas(upright)` provide normalized target coordinates and source-axis 1% vectors for Camera; clipped targets are explicitly unavailable.

Source interpretation applies coded-horizontal SAR and then rotation before provider Fit/Fill. Provider framing precedes its first canvas clip. Each ancestor transforms the already clipped child and clips again. Full source sampling bounds and visible support remain distinct. Integer half-open pixel coverage is computed once in f64 and sent to GPU; shader UV rounding cannot redefine the admitted pixels. Inverse f32 UV is anchored at the first covered pixel center.

Core bounds remain centers [-16,17], scale [1/64,64], and at most 16 authored operations. The renderer admits at most `MAX_DOCUMENT_DEPTH + 2` (258) scopes, including the root and an optional synthetic gap provider. Nonfinite, collapsed, or nonrepresentable cumulative geometry fails explicitly. This is bounded numerical spatial geometry, not arbitrary precision geometry.

## Verification

Rust 1.97.1, locked dependencies, and `/tmp/deadpan-media-compatible-xyhilms4/prefix` were used. Full scoped tests passed: 125 plan tests and 16 renderer tests. The new tests cover exact owner clocks through Repeat/Retime and gap scopes, Freeze picture identity with changing framing, actual Split picture/framing parity, ordered clips, SAR/rotation, canonical-canvas independence, exact/Q32 pixel boundaries, CPU/GPU parameter agreement, and input limits. Scoped all-target Clippy passed. After the scope-cap correction, `cargo test -p deadpan-render --locked` passed all 16 tests and doc-tests (exit 0), and `cargo clippy -p deadpan-plan -p deadpan-render --all-targets --locked -- -D warnings` passed (exit 0). The final 24-path hash manifest was rechecked without a mismatch.

Actual offscreen Metal qualification passed 84/84 cases on Apple M5 Max, macOS 26.5.2. `framing-metal-1.json` SHA-256 is `3858485e175898471edc3e77f1bdc77d1fd1333b3d6c806e701d33b97024afb2`. All pixels were read back. The four new solid-color crop-boundary cases matched exact codes; the four new rotation/anamorphic/nesting cases differed by at most one channel code, within the declared tolerance of two. Existing negative working-space and target-identity checks also passed.

The Metal report preceded only the 256-to-258 admission-cap correction, its headless maximum-scope assertion, and README wording. No pixel, transform, shader, color, or qualifier calculation changed afterward. The report applies to its 84 admitted cases; it does not prove a 258-scope live GPU workload. `plan-render-final-hashes.json` binds the final source after that correction.

## Limits and integration obligations

Keep identity provider scopes and prepend the synthetic provider for Repeat gaps. Camera must replace or insert at the selected scope, preserving ancestor ordering, and quantize projected f64 values through the explicit core policy before authoring them. Root owns same-ticket presentation guards and native UI verification.

Framing reuses GPU upload allocation but still uploads source bytes for every draw. No decode is required solely for a geometry change at the host layer. Offscreen timings include debug build/readback overhead and are not throughput qualification. This work does not establish live UI interoperability, physical display management, HDR, generated-provider decoding, overlays, encoder semantics, or a release gate.
