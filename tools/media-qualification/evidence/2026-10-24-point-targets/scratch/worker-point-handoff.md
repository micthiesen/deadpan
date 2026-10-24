# First-class point targets: non-App implementation handoff

Ownership and source: 31 non-App Rust files listed with SHA-256 in worker-point-freeze.json. Four new files: Core target/geometry.rs, Core target/point_tests.rs, CLI tracking/point_tests.rs and Store tests/point_targets.rs. Root owns all App source, docs and native qualification. No formatter, Git write, dependency, unsafe code, product/UI runtime or export work performed in this lane. Foreign fuzz, Cargo.lock, .claude and existing net/count-reuse code untouched.

API: TargetRegion remains a rectangle. TargetPoint { center:[u32;2] }; TargetGeometry::{Region(TargetRegion),Point(TargetPoint)} exports center(), center_ratio(), as_region(), as_point(), is_point(), validate() and both From conversions. AttentionTarget.region, TargetSample.region, TargetCorrection.region and EvaluatedTargetRegion.region now Geometry. Existing region_at/evaluated_region_at names stay; shared Plan follow_pose uses geometry center_ratio without changing its projection/evaluation semantics.

Wire: closed custom streaming map visitor, no untagged Value/content buffer. Fixed two-u32 tuple visitors reject extra elements before traversing them. Unknown/duplicate fields, absent/invalid center, null size and zero rectangle size refuse. Region serialization is byte-identical to the old center/size struct; Point serializes center alone. All schema headers/constants remain Core58/SQLite87. No migrations.

Validation and corrections: all observations retain initial geometry kind. Points interpolate center only through the existing exact checked fraction/half-even/confidence/state/correction/Lost rules. with_point_correction is immutable and validates before cloning: shape/kind/span/cap, strict ordering, update or insert at most256 corrections, at-start replacement/removal of equal-time correction, samples invalidated only in [at,next correction/span end). All other samples/clocks/provenance remain unchanged.

Tracking: Vision conversion and compaction remain rectangle APIs with explicit geometry matches. Analysis retrack_target rejects saved points. CLI correct_target rejects points before media preparation or worker launch; target_rect_at returns None for points. Fresh explicit rectangle tracking still operates normally. Face proposals remain rectangle targets. Area-conditioning keeps TargetRegion and explicitly returns RegionCaptureUnavailable::PointTarget for a point lacking an area; no fake box.

Meaningful verification:
- Core target::tests:18 passed, including old rectangle behavior, codec poison-body early refusal, mixed-kind validation, point half-even/confidence/Lost behavior, immutable correction ranges, real256 limit, i64MIN/MAX endpoints, interpolation property, command/raw patch/inverse, and actual followed-beat copy/paste for both shapes.
- Plan point Follow:1 passed; exact picture/framing/context equality vs rectangle centers through nested Source/Retime/Repeat clocks.
- Store point_targets:1 passed; separate SetTarget correction commits, read-only reopen, historical snapshot, Undo/Redo, preserved time.
- Analysis tracking::tests:19 passed, including existing compaction/property/retracking policy and point rejection.
- Models conditioning::region::tests:8 passed, including existing rectangle conditioning and explicit point refusal.
- CLI tracking::point_tests:1 passed; saved point refuses with missing worker/media before preparation or launch, no store mutation.
- All48 tests above pass. Exact commands/exits/logs are worker-point-focused-results.json and worker-point-regression-results.json; pinned shared target and FFmpeg environment are retained in the runner scripts.
- Whole selected non-App --tests compile: check4 exit0 for Plan/Analysis/CLI/Models/Store. New focused tests compiled in their executed runs.
- Initial non-App compile checks1/2/3 failed on expected constructor/accessor conversions, all fixed, streams retained. No behavioral test failures. One Models regression filter matched0; explicitly recorded and corrected to the actual module path with8passes.

Limits: no worker native UI, real detector or encoded-export qualification. Root owns these and broader final tests/review. Proposed next verification: Root App units/native point workflow, strict Clippy/format, full Core/Plan suites as appropriate. No active Cargo/terminal handles. Source frozen pending Root formatting and independent review.
