# First-class point targets

Base: 43477adbf6d69f6b31da944a8accff692fb3399e. Normative §§4, 7.6,
11.3. Existing targets are rectangles; points must not use fake tiny sizes.

Core contract: retain TargetRegion as the rectangle type; add TargetPoint
{ center: [u32;2] } and TargetGeometry::{Region(TargetRegion),Point(TargetPoint)}.
AttentionTarget.region, TargetSample.region, TargetCorrection.region and the
evaluated region become TargetGeometry. Keep existing field and evaluation
method names. Geometry supplies center(), center_ratio(), as_region(),
as_point(), is_point(), validation and From conversions.

Preserve existing rectangle JSON bytes. Point JSON is the same normalized
center object with no size field. Use a bounded custom map visitor, not an
untagged deserialize buffer: center and optional-present size are fixed u32
arrays; duplicate/unknown fields, null size and oversized arrays refuse early.
An explicitly zero size remains an invalid rectangle. Point center validates
inside [0,1000000]. Enforce one geometry kind throughout a target's samples and
corrections; point samples interpolate center only with existing exact
round-even/source-clock/state/confidence semantics. Existing additive-vocabulary
policy permits Core58/SQLite87 only if old rectangle wire remains byte-identical;
do not migrate or rewrite old packages.

Point manual correction is an immutable pure helper on AttentionTarget:
with_point_correction(at, point). Validate point/target/range/kind. Invalidate
only samples from at to the next existing correction (or span end), preserve
outside samples and all clocks. At span start replace initial point and remove
an equal-time correction; otherwise insert/update one correction. Respect the
256 correction limit and reject atomically. It must not start a tracker.

Vision's seed and observations stay rectangles. Track a saved point refuses
truthfully before worker launch or writes. Fresh explicit rectangle tracking
is still normal tracking; no fabricated area or protocol shape change.

Native plan: n opens the existing new-target editor; n there toggles rectangle
and point. Retain rectangle size only as temporary UI memory. Point mode has
only center adjustment; Tab cannot select hidden width/height. Corrections
lock the existing geometry kind. Enter uses typed SetTarget; Escape restores
the Camera draft. Saved/new points draw markers, not rectangles. Existing
picker, Follow, creep, rename/delete, aspect projection, clocks and own-save
continuation remain shared. Teach controls in registry/help/footer/inspector.

Ownership: worker owns Core/Plan/Analysis/CLI and necessary non-App Rust
adaptation/tests; root owns all deadpan-app code, docs, native harness, Git,
qualification and progress artifact. Preserve foreign fuzz/Cargo.lock and
.claude/. No agent worktrees, commits, pushes or whole-tree formatters.
Coordinate Cargo use in /tmp/deadpan-follow-creep-2026-10-24/target.
