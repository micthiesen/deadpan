# Independent review

## Runtime

No actionable introduced resource/lifetime/cancellation defect found in the assigned scope.

Audit limits: Reviewed the dirty additions in `crates/deadpan-cli/src/export_picture.rs` and `export_picture/contract.rs`, the new `ProjectPictureSession` accessors, and the render readback-dimension helper against `PictureRenderer`/`WorkingReadback`. The qualification example's in-progress content was excluded. No tests or device/GPU execution were run, per assignment. Ripwire found no existing call sites for the new session API; exact source and call-path searches were used to inspect its direct dependencies.


## Clock

# Export picture clock and geometry review

No actionable findings in the assigned lens.

The contract copies project/revision, presentation basis, and range from a read-only `ProjectPictureSession`; its fields and capture constructor prevent callers from authorizing alternate serialized evidence. Raster admission preserves the committed canvas for framing, applies the shared nearest-even rule only to the output raster, records signed exact aspect error, and validates both canvas and raster against renderer/readback bounds before target allocation. The half-open range maps output ordinals to relative PTS with exact `1/N` time base and `D`-tick duration while retaining absolute project frames separately. Both 48 kHz audio boundaries are independently mapped from the common project origin. Checked arithmetic and the existing tests cover endpoint, conversion, and encoder-rational bounds. The media change only exposes and reuses the existing geometry helper.

Reviewed `AGENTS.md`, spec §§4 and 22, the requested contract and tests, `ProjectPictureSession` capture/accessors, `FrameRate::audio_boundary`, and shared renderer bounds. No tests were run, as requested. The in-progress qualification example was excluded. Audio/encoding, HDR, and final-render isolation remain outside this library's current claim.


## Qualification

# Export qualification example review

No actionable findings in the assigned scope.

The pixel reference is an independent f64 implementation of source transfer decoding, linear bilinear sampling, Rec.709 OETF/YCbCr conversion, and the declared left-sited chroma filter; it does not call the production working-matrix, half-float, or I420 conversion path. It intentionally shares `PictureGeometry`, and the report says so. The odd fixture separately checks the committed canvas `[319, 179]`, output raster `[318, 178]`, and signed aspect error `70/28391`, then compares complete I420 planes using those explicit dimensions and framing inputs.

The output helper derives each requested ordinal from the captured range start, checks exact `30000/1001` PTS/time base/frame duration, and exercises full and nonzero half-open ranges plus exclusion of the terminal ordinal. The Generated path is optional and reports `skipped` when no fixture is passed. When supplied, its manifest schema and all frame identities/PTS/opaque RGBA are checked; the fixture producer validates every accepted decoded frame against its independent sampled-RGBA expectation before retaining the package. Generated output checks the accepted artifact identity, frame ordinal, source PTS and captured framing, and compares full I420 output to the retained expectations.

Reviewed the requested example and reference module plus their fixture producer and retained manifest. No Cargo, lint, or native runs were performed, as requested.
