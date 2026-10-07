# Selected-region checks for AI pauses, 2026-10-07

AI generation can capture a saved attention target with `:generate target=ID`
or `generate-hold --target ID`. `target=none` / `--target none` explicitly
clears it. Omission preserves the current request's choice, including absence.
The first request has no region target. Changed controls start a new request;
target corrections and removal make the old request stale. The inspector
shows the target and the candidate's actual measurement coverage.

## Capture and inspection

Context schema 4 retains the target ID, label and full record's SHA-256,
plus its evaluated source region, origin and confidence at each decoded
Original boundary's exact PTS. Regions are mapped through the actual fitted
content rectangle, including SAR and odd placement. Editorial Camera framing
is not baked into conditioning. Generated or authored-black boundaries, a
different asset, unsupported source time, lost/interpolated/weak tracking,
off-source boxes and regions too small after fitting remain explicitly
unavailable. No source edge is clipped to manufacture a usable seed.

The version-2 `inspect-landmarks` protocol accepts the exact two captured
seeds only alongside both retained PNGs. One supervised inspection runs face
landmarks and, when seeded, Apple Vision `VNTrackObjectRequest` revision 2.
The region tracker visits the left PNG, every canonical native picture in
exact PTS order, and the right PNG. It retains raw rectangles and confidence;
no held or interpolated substitute is inserted by Deadpan. Host policy stops
measurement permanently after loss, weak confidence or an out-of-crop box.

Revision 2 reads back tracking level Fast on this Mac even after Accurate is
assigned. The adapter explicitly pins and checks the observed revision 2/Fast
configuration before and after each picture. Apple's SDK states that this
revision ignores the level distinction. The report records the actual value;
existing selected-target tracking keeps its previous configuration.

Host schema 7, profile `deadpan-ffv1-bridge-7`, retains the region capture,
native movie and context/PNG object bindings, geometry, pinned runtime,
inspection timings, raw observations and recomputed policy. Even absent
targets have an explicit unavailable report. Schemas 3–6 remain readable with
their original evidence and cannot claim a selected target without a region
report. Worker runtime `0.15.8+deadpan4` validates the new context and controls;
model weights and prompt version are unchanged.

## Policy

`deadpan-generated-region-1` compares the continuously tracked subject with
the path between captured endpoint boxes, interpolated using exact native
PTS fractions. Distances use aspect-correct pixels inside the captured
presentation crop. These engineering thresholds are rejection heuristics:

| Measurement | Limit |
| --- | --- |
| Native tracking confidence | At least 0.70 in the native f32 domain |
| Center residual | More than 0.10 presentation diagonals |
| Log width/height/aspect residual | More than 0.40 |
| Sustained excess | Two consecutive native frames |

A weak or mismatched left boundary prevents measurement. A later loss or
right-boundary mismatch cannot erase a measured prefix or earlier rejection.
Unavailable frames and missing maxima stay distinct from zero drift. JSON
uses exact float round-tripping, so reopening preserves raw seeds and
deterministically recomputed measurements without a comparison tolerance.

Rejection uses `OutputValidationFailed` before Ready. It preserves the
committed freeze, revision and previously selected Ready candidate. Acceptance
remains explicit and undoable. Corpus calibration belongs to To verify (owner)
under §29.1.

## Verification

Apple M5 Max, macOS 26.5.2. Source parent `0b27e50b`; evidence root
`/tmp/deadpan-resume-20261006`.

- The first focused integrated run executed 175 tests: 173 passed. It exposed
  the Vision level readback above and one outdated unavailable-message
  assertion. The message assertion passed after correction.
- `region-native-4.log`: the real Vision worker covered all 48 native
  pictures of `moving-square-cut.mkv` plus both PNGs. The first twelve boxes
  were within 0.38–0.97 pixels of the authored fixture trajectory. Confidence
  fell to 0.687 at ordinal 10, so the policy retained ten measured frames and
  38 unavailable frames. It rejected sustained drift beginning at ordinal 7
  and preserved that rejection after confidence fell. This verifies a real
  detected rejection, not only fabricated observations.
- Pure and host regressions cover exact irregular PTS, one-frame spikes,
  permanent loss, small and off-source seeds, crop/aspect mapping, tampered
  raw measurements, policy/object/runtime substitutions, inclusive f32
  confidence, JSON round-tripping and actual schema-6/context-3 admission.
- Native service tests cover retained/cleared target choices, missing targets,
  runtime failures, late target arrival and correction/removal staleness.
- `region-python-final.log`: all 82 Python tests passed, including strict
  context/target binding, unsupported capture states and malformed coordinates.
- Independent review found and fixed tiny-region conditioning failure and
  legacy profiles claiming target constraints without evidence.

### Repository gate

`region-gate-3.log` passed both strict Clippy configurations and ran all 4,882
workspace cases: 4,881 passed, with only the generated routing snapshot still
stale. That snapshot's parser examples depend on its own retained command
labels, so the new `target=` label required a second regeneration. The reviewed
diff changes only Generate usage, examples and error text. No key chord changed.

`region-gate-finish.log` then passed formatting, both regenerated registry
checks, all 1,032 UI-harness cases and both doc tests. Ten workspace and two
UI cases were skipped by the standard gate. The earlier five bundle-fixture
failures used context 3 for fresh admission; all six real-media bundle cases
passed after their fixture adopted context 4 and explicitly checked the
unavailable region report (`region-fixture-fixes.log` and the workspace run).

### Inspector layout regression

The first compact `ai-variants` replay found that the new region row pushed
the chosen variant's coverage below the inspector clip. `VariantReveal` now
reveals a newly offered/chosen variant once, repeats that request during the
same outer frame's layout retries, and leaves later native scrolling alone.
Reopening the inspector also reveals its choice. It never changes keyboard
focus or authored state.

`region-reveal-test.log` passes the layout-retry/scroll regression. The corrected
`region-replays-2/ai-variants` passes both real paint visibility and native
wheel input after reveal. The inspected offline capture
`ai-variants-030.png` shows the target controls and complete chosen coverage
inside the 1280×820 inspector. No live screenshot was taken.

Both corrected replays passed all 44 checks: 25 for variants, 19 for
comparison. Layout warnings describe settled retries with distinct causes;
neither scenario painted an ignored retry or repeated the same unresolved
cause. `region-ui-clippy-final.log` passes strict UI-harness Clippy after the
layout fix. The executed replay app SHA-256 is
`ce8f280052fa9001a80f30cbf6b6b64fbf4c4a1fbc4f954f29ead2387a4b2547`.

### Packaged model and compatibility

`bundle-region.log` built the ad-hoc signed, 725.4 MiB release app, with all
74 Mach-O files audited. `bundle-region-verify.log` passed the relocated,
scrubbed-environment checks, native startup/shutdown, verified movie output,
bundled MLX/Metal and encoder checks, and damaged-helper/runtime refusals.
The 36.2 GB approved model pack imported into a fresh scratch home and passed
its bundled runtime smoke test (`region-model-import.log`).

`region-generation/summary.json` records a real Subtle generation with the
saved `center-detail` target and instruction “Keep the hands still.” It became
Ready in 77.715 seconds. Schema 7/context 4 retained the selected target and
exact seed boxes, and all 25 native PTS observations. Shared face/region
inspection took 318 ms: 164 ms decode and 154 ms Vision.

The synthetic source's region was too weak to track from the first generated
picture. Its left boundary measured successfully, while all 25 native frames
remained unavailable with low-confidence/lost-track reasons and no fabricated
maxima. This run verifies truthful unavailable coverage through the real
model path; the moving-object fixture above separately proves measured drift
and actual rejection. It does not establish perceptual model quality.

The committed fallback stayed byte-for-byte equal until explicit acceptance.
Acceptance then produced a fully verified MP4 in 2.495 seconds. Previously
accepted schema 3, 4, 5 and 6 projects rendered in 2.229, 2.190, 2.163 and
2.144 seconds, respectively, with their authored documents unchanged.
All proof commands exited zero. No bundle process remained afterward, and
the 60 captured source/configuration files matched their pre-build hashes.

Executed release SHA-256:

| Executable | SHA-256 |
| --- | --- |
| App | `e39c8030e7c3cc57f76da891eb35ab295b494c179f5151ab016a53ad3b7e3bc8` |
| CLI | `74ef9a80bfd26e3b5de4067ddf48ccd15bc15b52b6c212d66eebaaac65875f7d` |
| Vision worker | `430c241439d1b9e65997a65974955d0139f080a3fccd041c95270021625628ac` |
| Media worker | `915f69312e238c62b9a2e99b7094a190aa683671d0f93f6b4199bcb5636fda20` |
