# Encoded protocol and wiring review

No actionable defect found in this read-only source review. No build, formatter,
compiler, native execution, or tests were run.

Reviewed `encoded_render/protocol.rs` and its tests, the added native evidence
deserializers, shared `RenderContract` validation, the document hash refactor,
`encoded_render/mod.rs`, and CLI/private headless dispatch.

- Native reconstruction checks the complete picture geometry and exact project
  clock, then derives the actual audio count from both absolute boundaries.
  The selected encoder mode and B-frame policy remain explicit request fields.
- The raw spool's total-byte and frame caps remain in raw `validate()`. Encoded
  requests use the shared geometry/clock validation and their native packet,
  output, frame, duration, and sample limits instead.
- Every completed message binds the full expected contract, document hash,
  request, and attempt before being classified as a held completion.
- Report admission checks exact input/packet counts, declared file length,
  output/packet budgets, exact mux clocks, codec policy, both EOF claims,
  fast-start reader balance, and the host-selected moov allocation bound.
  Aggregate packet bytes intentionally do not claim to prove each packet's
  size; actual byte/media verification remains separate.
- The movie reference is exactly `output/movie.mp4`; the wire cannot select a
  runtime, package path, or publication destination.
- Deserialization remains evidence only. `EncodeContract` and
  `ExportPictureContract` still have no deserializer, while new evidence fields
  reject unknown fields and oversized messages retain shared framing limits.
- The shared hash still serializes the complete captured document into the
  bounded streaming hash writer, with cancellation/deadline checks. The old raw
  picture helper delegates to it without changing its hashed representation.
- CLI and `deadpan-app --headless` use the same exact two-argument private entry
  dispatch. Module errors and documentation preserve candidate versus verified
  media distinction.

This review does not qualify codec output, decoded sync, actual B-frame/GOP
behavior, color, publication, or runtime performance. Those need emitted-file
and native execution evidence.
