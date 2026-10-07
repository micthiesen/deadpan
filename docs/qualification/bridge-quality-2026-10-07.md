# AI candidate motion and lighting, 2026-10-07

Specification §12.5 now has host motion and abrupt-lighting rejection before
Ready. The host decodes every native canonical picture under the existing
qualification deadline. It reads the private, verified descriptor; SHA-256 is
measured alongside BLAKE3 during canonicalization, without another copy or
worker-path open. The decoder checks frame count, raster, RGBA depth and each
exact Matroska PTS. Cancellation and the shared deadline remain active.

## Measurement and policy

`deadpan-analysis::generation_quality` reduces each full-range sRGB picture to
a 64×36 encoded-luma grid. It retains two grids and compares 36 fixed 5×5 blocks
across a ±4-cell search. Zero-mean matching separates brightness from movement.
Texture, match error, uniqueness, search-edge and coverage checks determine
whether displacement is available. At least nine blocks must match; even then,
measurements describe those blocks, not every object. Textureless pictures,
ambiguous patterns and movement outside the search remain explicitly unavailable.

P95 and maximum displacement divide X and Y by picture width and height before
taking Euclidean magnitude. The rejection rate uses the authored sampling clock:
one native pair spans `(N+1)/(project_fps×(M−1))` seconds for N interior project
frames and M native pictures. Using the native rate alone would overlook speed
changes from exact-duration resampling.

Policy `deadpan-motion-lighting-1` rejects P95 displacement above 0.5, 1.0 or 2.0
normalized units per authored second for Still, Subtle or Moderate respectively.
It rejects an absolute mean encoded-luma step of at least 32/255 when at least
half the grid changes in the same direction by at least half that mean's
magnitude. Directional support tolerates clipping and regions with different
brightness changes. An initial near-equal-change metric could miss a white
flash when a small distinct region shifted the mean; the final metric and tests
cover that case and a small local bright region. Each adjacent pair is inspected,
so a flash returning to its entry picture cannot disappear into endpoint checks.

These are versioned engineering limits for gross failures, not calibrated human
acceptance thresholds. Insufficient motion evidence is retained as unavailable;
it does not become a measured zero or a successful motion check. The inspector
shows the coverage status beside the chosen variant and in each variant tooltip.
Endpoint RGB readings remain advisory. Face/region geometry and mouth checks
remain separate implementation work; no placeholder reports claim they passed.

## Durable behavior

New host provenance is schema 4, profile `deadpan-ffv1-bridge-4`, with the typed
quality report, policy thresholds, native contract, authored clock, motion
setting and every pair's measurements/coverage. The reader validates these
against the captured plan and refuses incomplete, reordered, inconsistent,
unknown or rejected evidence. Schema 3 without a quality field remains readable
for previously accepted footage. It is visibly identified as lacking these
checks. A present null report is invalid in either schema.

A rejected candidate uses the existing durable `OutputValidationFailed` path.
Its diagnostic retains the policy, offending frame, measured value and limit.
It cannot publish a Ready receipt or replace an earlier Ready selection. The
committed fallback and authored revision remain unchanged. Full successful
reports are retained with the six-object candidate bundle and survive acceptance
without its model. Rejected attempts retain the bounded failure diagnostic;
they do not publish a purportedly qualified bundle.

## Verification

Apple M5 Max, macOS 26.5.2. Evidence root:
`/tmp/deadpan-resume-20261006`.

- Nine pure measurement tests cover known translations, brightness changes,
  clipped flash entry/return, distinct regions, insufficient texture/coverage,
  ambiguous matches, search edges, padded rows and malformed inputs.
- Six policy tests cover authored-time thresholds, selected motion, flash return,
  unavailable evidence, incomplete/extra frames and nonfinite/inconsistent data.
- Eight stored-evidence tests cover schema 3 and 4, strict shape and identity,
  altered thresholds, missing/reordered pairs and rejected reports.
- `quality-media-tests.log`: both real-media attempt tests passed in 2.115
  seconds. A full-white middle frame is rejected with its exact frame/value,
  with no Ready receipt, document/revision change or replacement of the earlier
  selected Ready bundle. Normal variants retain schema-4 evidence.
- Independent reviews checked timing, private descriptor ownership, bounds,
  lifecycle failure, partial coverage, native status and stored compatibility.
  No correctness findings remained.
- `gate-13.log` stopped on Clippy's checked-division lint; the reducer now uses
  `checked_div`. `gate-13-2.log` passed strict lint and ran 4,769 workspace tests
  in 413.889 seconds: 4,765 passed, four failed, and ten explicit qualifications
  were skipped. All four failures were success-path bundle tests using the old
  modulo-RGB fixture, whose frame 7 has a measured −48/255 mean step over half
  the picture. The new guard correctly refuses that fixture. The dedicated
  gradual `rgb25_24_smooth_candidate.mp4` replaces it only for those success
  tests, with independently calculated native/sample hashes and expected
  pixels. The original conversion fixture is retained unchanged.
- `gate-13-completion.log`: formatting and strict workspace/UI lint passed.
  All six bundle tests passed in 0.424 seconds, all 1,030 UI-harness tests
  passed in 239.444 seconds, and both doc tests passed. Two explicit UI
  qualifications remained skipped. No output-pipe leak diagnostic appeared in
  either workspace or completion runs. The complete required gate components
  passed after the fixture repair; the initial `xtask gate` failure is retained.
- Initial replay assertions found the status in accessibility, but a stronger
  paint check found it below the inspector's visible clip. Moving a separate
  label immediately after the chosen row still clipped it at the default
  1280×820 window. The compact coverage reading now sits inside that card,
  whose height follows its text. Stable attempt IDs preserve keyboard focus
  as the selection changes. The full explanation remains in its accessible
  label and tooltip. The replay now requires actual visible, untruncated paint.
  The first compact build caught a moved accessibility string in an `Fn`
  closure; retaining a clone fixed it before replay execution.
- `quality-replays-compact-2`: all 43 variant and comparison checks passed
  (97.7 and 99.3 seconds). Review then clarified the visible label to
  `Motion coverage 0/16`. `quality-replays-coverage` passed all 24 variant
  checks in 96.3 seconds with that final text fully painted at y=664–673,
  inside the inspector clip ending at y=677, without elision or opaque
  occlusion. The full explanation is also in the accessible card label.
  Final replay binary SHA-256:
  `0422a79f76626df95dcc150d476a4c518c019d7326e60edb4e1acc5133f6b9d7`.
  The reports retain nine/four second-retry frames and two/three runs of
  consecutive layout retries with distinct causes, respectively. No ignored
  retry or repeated identical cause failed the replay. Strict UI lint and
  final formatting checks passed.
- `bundle-quality-coverage` built a 722.8 MiB ad hoc signed app with all 74
  Mach-O files audited. Its relocated, scrubbed-environment `bundle-verify`
  passed every positive and negative check. No native desktop window was
  opened; visual inspection used the offscreen replay render.
- `quality-generation/summary.json`: the packaged real model reached Ready in
  78.810 seconds on `cfr-bframes.mp4`, retaining 24 transitions across 25 native
  pictures. Twenty-three pairs had measurable motion; one was explicitly
  unavailable. Between two and fifteen blocks matched per pair. The maximum
  mean luma shift was 18.323/255; measured P95 displacement was zero in the
  qualifying blocks. This synthetic picture fixture does not establish human
  face quality or whole-picture stillness. The request retained Subtle motion,
  literal guidance and 75 actual prompt tokens. The document stayed unchanged
  until explicit acceptance. The verified new movie published in 2.462 seconds;
  an older accepted schema-3 artifact also rendered in 2.234 seconds without
  changing its document. App SHA-256:
  `b5e6ff32cd3ff1dfb60f5bc973b6605a11d8d2c3b8ae46a9278a117827807dd6`;
  CLI SHA-256:
  `18c1f338fae15c562ff52f6e6fdc3fb8dffa57c4fd3cd690efa275588cf88141`.
  Source parent was `4ff5594f` with this change; the recorded binary hashes
  identify the executed builds.

## Remaining qualification

The §13.4 rights-cleared human footage corpus and perceptual calibration remain
To verify (owner) under §29.1. Run Still, Subtle and Moderate on the recorded
source moments, inspect retained motion/lighting coverage and rejected frames,
then audition accepted candidates against both joins. Record false positives,
missed discontinuities and unusable motion rather than inferring identity or
silence from these measurements. DP-12 remains Partial.
