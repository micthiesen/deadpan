# Project footage for AI extension, 2026-10-07

This milestone connects the development extension contract to immutable project
footage. It does not enable extension jobs in the app or mark DP-12 complete.
The approved model capabilities and the overall 88% estimate remain unchanged.

## Implemented behavior

`prepare_extension_scoped_with_options` captures nine chronological pictures at
exact 24 fps spacing in the Hold's authored definition, below outer Repeat and
Retime owners. Both directions use the shared qualified picture reader and
canonical SDR conversion before editorial framing. The manifest retains measured
Original or generated identities, exact PTS, definition coordinates, PNG hashes,
content geometry and the selected region anchor. A present opposite seam is
retained explicitly as unconditioned. A missing side is never fabricated black.

The host matches the worker's development envelope: 768×320, K9/E8, project
rates 1–120 fps and a positive authored interval no longer than 1/3 second.
Capture shares one 120-second deadline checked between bounded calls; source
admission and store validation retain their existing cooperative limits, so this
is not a preemptive wall-time guarantee. PNGs have a 16 MiB aggregate cap and
the manifest has a 1 MiB cap. Nothing in preparation writes authored state.

### Pictures between model samples

The canonical picture walker now also describes exact affine provider spans
over a closed definition interval. Half-open spans plus a separate terminal
sample cover every structural seam. The query preserves Source selections,
forward/reverse Hold clamps, accepted movie clocks, cutaway Hold/Loop/Gap/Bounce,
removed pictures and explicit/implicit Repeat gaps under rational retiming.
It does not expand unrelated occurrences. Temporal samples, boundaries and spans
share traversal work, a 64 MiB metadata budget and a 512-span aggregate cap.

The host resolves each span against its measured index and examines every
intervening source ordinal, including pictures the model's 24 fps grid skips.
It rejects discontinuous mappings of the same Original or generated master and
checks actual pictures at other provider seams. Source identity and normalized
generated-content aspect key a bounded signature cache. At most 512 context
analysis/admission frame reads are permitted; complete source admission/indexing
has its existing separate media limits. The subsequent PNG capture reads at
most ten pictures. Preview proxies are never involved.

`deadpan-context-shots-1` reuses the existing picture signatures and gradual
transition candidates, but does not apply navigation's six-picture separation,
flash suppression or global greedy transition suppression. It measures all
requested ordinals with 125 real padding pictures per side, clipped only at the
physical media's edges. It checks the full blended range and keeps absolute
measurement coverage. Singleton spans also receive the padded check: splitting
a fade into one-frame beats cannot disable detection. Provider seam checks
inspect abrupt change; they do not claim full cross-provider gradual coverage.

These are conservative rejection heuristics. Rapid motion and flashes may be
rejected; low-contrast cuts and unsupported transition lengths can be missed.
No detected transition is not a guarantee of one scene or a suitable AI result.

The host report retains policy versions, relative input positions, measured
identities, structural support and detector coverage. These are observations for
later recapture. Manifest schema 1 does not yet bind this in-memory report.
The report does not yet grant persistent request relevance or accepted-media
authority.

## Verification

On the reference Apple M5 Max / 128 GiB Mac, using the pinned native FFmpeg
8.0.3 development prefix:

- The corrected full `cargo xtask gate` passes: formatting, strict workspace
  and UI-harness Clippy, 5,179 workspace tests, 1,070 UI-harness tests and both
  doctests. The runs leave 10 workspace and two UI tests skipped as declared;
  skipped checks are not evidence. No Rust source changed after this gate.
- Fourteen new plan coverage tests include 128 generated property cases against
  canonical picture evaluation. Twelve analysis guard tests cover abrupt and
  gradual changes, singleton spans, real padding and exact allocation bounds.
- Thirteen focused CLI tests pass, including real qualified video in both
  directions, fractional clocks, exact measured IDs/PTS, PNG geometry, interior
  unconditioned seams and definition isolation below Repeat/Retime.
- Real capture rejects a one-frame cutaway and a source jump that every sparse
  model sample misses. Refusals, missing context and cancellation leave the
  project and history unchanged.
- A real lossless fade is rejected before and after canonical one-frame
  partitioning. The regression verifies all displayed picture bytes/identities
  and total duration are unchanged, and proves each fragment touches just one
  measured ordinal.
- The fade fixture's 829,440 decoded RGB channels match its source formula
  exactly. Its retained generator, exact clock and hash are documented in
  [the fixture directory](../../crates/deadpan-cli/tests/fixtures/README.md).

The retained test logs, source/binary hashes and review record are in
[`2026-10-07-extension-context`](../../tools/model-qualification/evidence/2026-10-07-extension-context).
No new inference, candidate acceptance, packaged export or native UI claim is
made by this milestone.

## Review fixes and retained failures

Independent reviews covered the plan walker and host capture separately.
They found and corrected the singleton fade bypass, a host/worker project-rate
limit mismatch and unbounded display-aspect expansion in the shared conditioning
helper. Display width now uses exact checked rounding and the shared raster
limits before allocation. Ordinary anamorphic preparation remains covered.
New extension manifests also refuse legacy approximate BT.709 conversion;
historical bridge readers are unchanged.

The first compile exposed a missing checked `ceil` result in accepted-frame
selection. A source-jump test initially retained an incompatible audio mapping;
the fixture now resets it when removing audio. The first fade container rounded
timestamps to milliseconds, so it could not prove the singleton regression.
The replacement uses exact MP4 ticks. Native admission then correctly rejected
its missing transfer metadata; explicit per-frame colour properties in the
generator fixed the fixture. No decoder rule or test assertion was relaxed.
The first full gate also found manual ceiling division in an analysis test;
the corrected test uses `div_ceil`.

## Still required

Share the full temporal descriptor through preparation, current requests,
accepted origins and dependency replacement decisions. Implement operation-specific
quality admission, accepted artifact persistence/reopening, native controls,
comparison and explicit acceptance. Exercise generated-neighbor capture and
cross-provider gradual checks. Qualify longer durations with the real model
before advertising them. Those are implementation obligations, separate from
the owner-only perceptual checks under spec §29.1.

The retained [integration design](../../tools/model-qualification/evidence/2026-10-07-extension-context/next-integration.md)
describes the next bounded store milestone: shared temporal input identities,
all-support relevance, variable-degree replacement dependencies and durable
intent history. V3 Ready admission remains refused until its quality and
accepted-media evidence paths are implemented.
