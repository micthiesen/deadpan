# AI sampled endpoint checks, 2026-10-07

The host compares the first and last pictures that enter the edit against the
retained conditioning PNGs before Ready. The model's native conditioning
endpoints are excluded from the authored interval and cannot prove these joins.
The existing Smooth/Noticeable/Jump readings remain advisory.

## Geometry and measurement

Conditioning context schema 3 retains the exact centered presentation crop and
each boundary's fitted content rectangle in native-raster pixels. These come
from the same dimensions and offsets used to prepare the PNGs. Odd-size
rounding, rotation and sample aspect remain part of the preparation path.
Authored black has no content rectangle; decoded black still has one. Earlier
contexts remain readable without inventing geometry, but fresh qualification
requires schema 3. Adapter `0.15.8+deadpan3` checks the same strict grammar before
model imports. The standalone `prepare_run.py` requires `--legacy-only-probe`;
its supplied PNGs lack captured project boundary evidence.

The host opens the sampled canonical master through its private verified
descriptor and checks its raster, frame count and exact millisecond PTS. It
decodes ordinal 0 and N−1; N=1 reuses one decoded picture for both joins. Retained
PNG reads restore their cursor for subsequent publication. PNGs must be RGB8 at
the exact native dimensions; malformed or silently convertible formats fail.
All stages share qualification cancellation/deadline and bounded input, decode,
index and pixel limits. The pixel bound is 4096², PNG bytes 64 MiB, sampled
index 65,536 frames and 16 MiB. Image decoding and reduction are bounded
cooperative work; they are checked before and after execution.

Inside the presentation crop, every R/G/B absolute difference is computed
before spatial reduction. A fixed 64×36 grid measures broad changes; cells are
weighted by their actual pixel area, so tiny or odd rasters have no empty-cell
dilution. Bars inside the presentation crop remain visible comparison content;
padding outside it is ignored.

Policy `deadpan-endpoints-1` rejects when mean RGB difference is at least 64/255
and cells with mean difference at least 64/255 cover at least 75% of the crop.
Both conditions must hold. These versioned limits target gross discontinuity;
they are engineering thresholds, not calibrated perceptual acceptance.
Face geometry and mouth motion remain separate implementation work.

## Evidence and lifecycle

Host schema 5, profile `deadpan-ffv1-bridge-5`, retains the unchanged
motion/lighting report plus a typed endpoint report. The latter binds exact
sampled contract/ordinals/PTS, thresholds, measurements, geometry, sampled
movie, retained context and both input objects. Stored admission rechecks
shape, policy and object identities, then binds geometry to the separately
retained context. Old schema 3/4 footage keeps its original evidence semantics.
The inspector reports the new checks or their absence for an older candidate.

Endpoint rejection follows `OutputValidationFailed` and includes the failing
join, sampled ordinal, measured difference/coverage and policy limits. It
cannot change the authored freeze, revision or earlier selected Ready bundle.

## Verification

Apple M5 Max, macOS 26.5.2. Source parent `e95569c9`. Evidence root:
`/tmp/deadpan-resume-20261006`.

- `endpoint-analysis-model-tests.log`: all 163 analysis/model tests passed in
  3.738 seconds, one explicit qualification skipped. They cover difference
  math, chromatic changes, crop/padding, exact thresholds, malformed layouts,
  schema strictness, geometry, sampled timing, retained evidence and decoder
  cancellation/format checks. An initial one-frame unit fixture used an
  incompatible 25-frame minimum; its explicit test capability now admits the
  requested interval. Production capability was unchanged.
- Python model qualification: all 79 tests passed, including 16 focused worker
  tests. Cross-language review found a color-description mismatch; Python now
  mirrors Rust's 4096-byte UTF-8 bound, NUL rejection and exact Unicode
  White_Space trimming rules for schemas 1–3, with boundary tests.
- `endpoint-cli-tests-3.log`: all 45 generation tests passed in 11.370 seconds.
  Real encoded candidates prove uniformly wrong footage and independent
  entry/exit mismatches fail while fallback, revision and earlier Ready remain
  intact. A one-frame Hold reports sample 0/PTS 0 for both joins. That fixture
  uses a 2 fps project so its native interval fits the unchanged provider
  minimum; the initial 24 fps fixture correctly failed planning. A prior
  compile caught a borrowed rather than owned qualification ID, now fixed.
- `endpoint-bundle-tests.log`: all six native bundle tests passed in 0.468
  seconds, including retained actual RGB8 PNGs, explicit durable acceptance,
  relocation and Undo. Boundary snapshots survive inspection and publication.
- Independent review found no remaining issue in bounds, deadline/cancellation,
  sampled clocks, crop/coverage math, evidence binding or legacy admission.
- `gate-14.log` stopped on Clippy's range-pattern lint; the schema match now
  uses `1..=3` without changing admission. `gate-14-2.log` then passed strict
  workspace/UI lint and 4,792 of 4,793 workspace tests in 413.908 seconds,
  with ten explicit qualification skips. The sole failure was an integration
  assertion still expecting context schema 2. It now checks schema 3 and the
  actual 569×320 crop at native offset (99, 0), including both content bounds.
  All three conditioning integration tests passed in 1.636 seconds on rerun.
- `gate-14-completion.log`: formatting and strict workspace/UI lint passed
  again, all 1,030 UI-harness tests passed in 256.620 seconds (two explicit
  skips), and workspace doc tests passed. The corrected assertion was the
  only change after the full workspace test run.
- `bundle-endpoints.log` and `bundle-endpoints-verify.log`: the fresh release
  bundle passed signing, all 74 Mach-O dependency audits, staged runtime
  execution, relocated scrubbed-environment checks and every helper/runtime
  tampering or removal check. This is evidence on this Mac, not a second Mac.
- `endpoint-replays.log`: `ai-variants` and `ai-compare` passed all 43 checks
  in 197.7 seconds. The chosen card's motion coverage remains fully painted,
  unelided and unoccluded, and its accessible detail reports both endpoint
  checks. Every painted layout settled; the retained warnings describe
  retries with distinct causes. Replay executable SHA-256:
  `c2c6780462f35c83e370079f711e21832586670ba88faa30341381db76677816`.

## Real model and accepted export

`endpoint-generation/summary.json` retains the packaged run with runtime
`0.15.8+deadpan3`, Subtle motion, seed 1 and “Keep the hands still.” guidance.
It reached Ready in 78.206 seconds. Its 25 native pictures produced exactly
30 sampled pictures at 30000/1001 fps. Both sampled joins passed inside the
captured 569×320 crop: entry frame 0/PTS 0 measured mean RGB difference
1.7644/255; exit frame 29/PTS 968 measured 1.7025/255. Both had zero gross
cell coverage. The context, both PNGs and sampled movie matched their retained
object identities and the report's exact geometry.

The committed freeze and revision remained unchanged until explicit Accept.
The accepted project then produced a verified, published movie in 2.518
seconds. Previously accepted schema-3 and schema-4 projects also rendered
successfully in 2.160 and 2.167 seconds, with their documents unchanged.
These are deterministic-fixture integration measurements, not human quality
or performance qualification over the §13.4 corpus.

Executed bundle SHA-256 values:

- App: `f6cc041abb314faf4545d9ead2d2c154423310606b032cee0015ecd97529e1e6`.
- CLI: `ef0df506a6b25d3aecaf7e341f8fbb03ddc6acda3c5663c6d0b5afa49a19abf7`.

Perceptual calibration on the §13.4 rights-cleared corpus remains To verify
(owner) under §29.1; record false rejections and missed joins for all durations
and motion settings. These checks do not establish identity preservation.
