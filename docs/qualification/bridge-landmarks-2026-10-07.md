# AI face geometry and mouth checks, 2026-10-07

Fresh AI candidates now pass a separate native landmark inspection before Ready.
The host retains observations and recomputes conservative face geometry and
mouth-motion assessments. These are rejection heuristics, not proof of identity,
silence or acceptable animation. Generic drift checks for an authored non-face
region remain open.

## Inspection and evidence

`deadpan-track inspect-landmarks` uses Apple Vision
`VNDetectFaceLandmarksRequest`, revision 3, constellation 76. Its separate
version-1 protocol requires every canonical native picture at its exact PTS,
followed by EOF, and optionally both retained conditioning PNGs. AI qualification
always supplies both PNGs. The native adapter copies only bounded eye, nose and
lip points while their Objective-C observations remain alive. Coordinates use
displayed top-left geometry, with explicit quarter-turn mapping.

The host copies verified inputs into a private workspace, checking SHA-256,
BLAKE3 and lengths while preserving the publication readers' cursors. It shares
qualification's cancellation and deadline, supervises the owned worker group,
confirms cleanup and a successful exit, rejects failed I/O pumps, then snapshots
the output through the pinned workspace. Missing helpers, malformed input or
protocol failures fail qualification; they do not become a successful empty
detection. Missing or low-quality detections are separately typed observations.

Bounds are 2–1,025 native pictures, eight faces per picture, 76 retained points
per face, dimensions at most 4,096 each, 16 GiB of native input, 64 MiB per PNG
and 64 MiB of raw observations. The application caps final provenance at 32 MiB.
PNG admission requires exact RGB8 and dimensions, with a bounded decoder;
grayscale, alpha and 16-bit images are refused instead of converted.

Host schema 6, profile `deadpan-ffv1-bridge-6`, retains the existing motion/light
and endpoint reports plus `deadpan-face-mouth-1`. This report binds raw
observations, exact native contract, runtime, timings and policy to the native
movie, context and both boundary objects. Admission recomputes the assessment
and compares captured geometry with both the endpoint report and retained
context. Schemas 3, 4 and 5 remain readable with their original evidence levels.
The Python adapter and context schema remain `0.15.8+deadpan3` and 3.

## Policy and coverage

Only boxes and landmarks inside the presentation crop contribute. Face geometry
requires a uniquely associated track connecting both usable conditioning
boundaries. Overlapping competitors, missing observations and low confidence
make association unavailable. Face list order never establishes identity.
The policy compares center, log width/height/aspect, and roll-normalized
eye/nose geometry with the interpolated boundary path in aspect-correct pixels.

Mouth aperture uses the inner lips' extent perpendicular to the eye axis,
divided by eye separation. It uses native frames independently of boundary
association, seeds later entering faces, and measures continuous qualified
segments of at least three pictures. Gaps, ambiguity or missing landmarks end
a segment. Earlier qualified motion remains measured, and an earlier rejection
survives a later interruption. No comparison crosses an uncertain identity or
missing observation. Constant opening alone does not trigger this check.

Policy `deadpan-generated-face-geometry-mouth-1` retains these thresholds:

| Measurement | Limit |
| --- | --- |
| Face and landmark confidence | At least 0.70 |
| Center residual | More than 0.10 presentation diagonals |
| Log size/aspect residual | More than 0.40 |
| Eye/nose residual | More than 0.20 eye-distance units |
| Mouth aperture change | More than 0.18 eye-distance units |
| Sustained face excess / mouth changes | Two consecutive frames / changes |
| Minimum eye separation | 0.01 of the shorter presentation edge |

These engineering thresholds need corpus calibration under §29.1. Unavailable
coverage is visible in the chosen candidate's accessible detail and tooltip.
Rejections include the native frame, measured values and limits, use
`OutputValidationFailed`, and preserve the committed fallback and earlier Ready
selection. Acceptance remains explicit and undoable.

## Verification

Apple M5 Max, macOS 26.5.2. Source parent `070b2b49`. Evidence root:
`/tmp/deadpan-resume-20261006`.

- `landmark-tests-3.log`: 30 focused analysis, protocol, PNG, host-evidence and
  native tests passed in 0.194 seconds. Regressions cover later faces,
  insufficient observations, retained rejection before loss, gap/crossing
  separation, more than 255 segments and precise unavailable reasons.
- `landmark-native-smoke.log`: real Vision detected both drawn faces with eyes,
  nose and inner/outer lips on each of two pictures, then no faces on two empty
  pictures. Including two retained PNGs, six observations took 117 ms, of which
  89 ms was Vision; the reported 27 ms decode field includes other preparation.
  The strengthened positive landmark assertions also pass in the focused run.
- Independent review found three mouth-coverage bugs in the initial policy;
  the segment walk and regression tests above fix them. Follow-up review found
  no remaining issue in that walk. Host review also led to rejecting a panicked
  I/O pump and raising the final provenance budget to the existing reader limit.
- Standalone helper building exposed an undeclared Foundation `NSEnumerator`
  feature that workspace feature unification had hidden. The explicit feature
  and standalone build now pass (`landmark-helper-build-2.log`).
- `landmark-bundle-tests-2.log`: all six real-media bundle tests passed in
  0.563 seconds, including publication, explicit acceptance, Undo and relocation.
  The first integration run caught a missing host-created output directory;
  the host now creates it before launching the helper.
- `landmark-cli-tests.log`: all 46 generation tests passed in 11.470 seconds.
  A missing landmark helper fails validation while preserving the authored
  freeze, revision and earlier selected Ready bundle. Successful variants
  retain schema-6 geometry evidence, including explicit no-face coverage.

- `gate-15.log`: formatting, strict workspace/UI Clippy and doc tests passed;
  all 4,829 workspace tests passed in 413.433 seconds with ten explicit
  qualification skips, and all 1,030 UI-harness tests passed in 241.232 seconds
  with two explicit skips. Nextest reported a retained pipe for the passing
  dialog cancellation test. That test uses in-memory ready/pending futures and
  refuses the second dialog before opening native UI; it launches no worker.
  Its isolated rerun passed without the warning (`landmark-dialog-recheck.log`).

- `landmark-replays.log`: `ai-variants` and `ai-compare` passed all 43 checks
  in 186.9 seconds. The chosen card's coverage is fully painted, unelided and
  unoccluded; accessible detail states the unavailable face and mouth evidence.
  Offline replay PNGs were visually inspected. All layout retries settled;
  retained warnings describe consecutive retries with distinct causes.
  Replay executable SHA-256:
  `85c089e0a4dcdc23a0a52118865c4f7906cbc781dcee0e4869a56d4324108cb8`.

- `bundle-landmarks.log` and `bundle-landmarks-verify.log`: the release build
  completed in 2 minutes 55 seconds. Ad hoc signing, all 74 Mach-O dependency
  audits, staged AI runtime execution, relocated scrubbed-environment positive
  checks and all helper/runtime tampering and removal checks passed.

## Real model and accepted export

`landmark-generation/summary.json` retains the packaged run using runtime
`0.15.8+deadpan3`, Subtle motion, seed 1 and “Keep the hands still.” guidance.
It reached Ready in 77.741 seconds. The schema-6 report covered all 25 native
pictures at 24 fps and both input PNGs, with exact PTS, object bindings and the
captured 569×320 presentation crop. Native inspection took 264 ms, including
133 ms in Vision. The test footage has no detectable faces; face geometry and
mouth motion are explicitly unavailable, with no invented zero measurements.
Positive landmark extraction is established separately by the drawn-face test.

The authored freeze and revision stayed unchanged until explicit Accept. The
accepted 30-frame Hold at 30000/1001 fps produced a verified, published movie
in 2.240 seconds. Older accepted schema-3, schema-4 and schema-5 projects also
rendered successfully in 2.206, 2.221 and 2.191 seconds, respectively, with all
three documents unchanged. This verifies the new packaged admission and stored
evidence path on this Mac; corpus quality remains on To verify (owner).

Executed release SHA-256 values (`bundle-landmark-hashes.json`):

- App: `be779b6b9e802470b244e9cd9a4b8ec6f9328cb934bb9d85bd62ddc6b3c02081`
- CLI: `ee8a01f10a177d215a777df109feda2cd7ea752cfce5011b104ffeb484399c6b`
- Landmark worker: `ab6d50da01687a17f88d8542f667e8e4a78a51ff3f8070a396f7cb82b650c692`
- Media worker: `d22bee839ab765f23a5fa8fb516a8f36bc017efd685f175e79f9fa0eca15a7d7`
