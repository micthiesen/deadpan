# Prepared compound pause profile (2026-10-23)

## Result

An isolated, instrumented release build captured the prepared-compound service path during nine rotated runs: three root-control runs, three with profiling disabled, and three with profiling enabled. All nine runs passed their seven pause checks, source/binary guards, and authored-input checks. The profile-on parser found exactly one macro report, one store commit, and one count for every phase in each run. All six control/off runs produced no profile output.

The profile is diagnostic instrumentation in a private source copy. Its build did not change the production checkout, Core 58 or SQLite 87. The final profile app binary was `f995232c3c20570b1de5e9e52e77945495c1c4b5b230b8b1f0773c76f757cbbc`; the root-control app was `69978abd09ffc52dcd281066bc615d7b3e1d5cb11653f5d888d0738015ba1496`.

## Pause measurements

The medians below come from three samples per condition on the same 10,000-Hold fixture. All are diagnostic; no percentile, calibrated instrumentation overhead, or target pass is established.

| Condition | Input to commit | Input to offscreen composition | n |
|---|---:|---:|---:|
| Root control | 269.306 ms | 272.090 ms | 3 |
| Profile build, off | 271.569 ms | 274.149 ms | 3 |
| Profile build, on | 270.734 ms | 273.230 ms | 3 |

This later diagnostic batch does not replace the separate qualified five-pair
[before/after comparison](compound-runtime-preparation-2026-10-23.md). Three
observations per condition with uncalibrated overhead establish neither an
instrumentation cost bound nor another production improvement.

## Captured service phases

Profile-on medians, in milliseconds:

| Phase | Median | Measurement relationship |
|---|---:|---|
| App service total | 264.275 | Inclusive outer service interval |
| App macro work | 260.341 | Nested inside service total |
| Semantic planning | 108.337 | Disjoint app child |
| Prepared commit in app | 131.619 | Includes the store prepared commit |
| Store prepared commit | 131.618 | Inclusive store interval |
| Store command preparation | 117.677 | Disjoint store child |
| Store callback | 0.000791 | Disjoint store child |
| Store write plan or bank | 7.352 | Disjoint store child |
| Durable SQL commit | 6.503 | Disjoint store child |
| Service refresh | 4.204 | Disjoint app child |
| Shared publication | 3.958 | Disjoint service child |

The immediate store guard and cache publication medians were 0.022958 ms and 0.000792 ms. The refresh-success marker is zero-duration. The six store children are disjoint; app and store totals are inclusive and overlap, so totals must not be added. Queue wait was not measured. Logging occurs after shared publication. The parser derives residuals to verify phase containment; it does not attribute all residual time to application work.

This fixture has an empty register bank. Its sub-microsecond host callback is
not a bound for programs that prepare copied content. The measured preparation
phase still includes its existing validation and work-limit accounting; the
profile does not yet separate those internal costs.

## Checks and limits

The three profile-on logs each contain one complete 16-line profile block. The parser validated phase order, `count=1` for every phase, one store commit, nonnegative residuals, and a successful refresh marker. Each run passed seven checks, kept authored input paths and bytes unchanged, left an empty WAL, and passed source and binary guards. The six profile-off/root-control logs have no profile block. Raw logs, parser output, source identities, and all reports are retained.

The nine replay reports each include Kestrel and seven large-pause checks. Each
retains two layout/retry warnings and three skipped-scope notes. These are
informational scope exclusions, not skipped unit tests or hidden failures.

An independent static timing review found no issues. That review is retained separately from the release build, runtime replays, and parser validation. The qualification made no physical-display timing, source-decoding or PCM performance claim.

## Evidence

The complete bounded package is [2026-10-23-prepared-compound-profile](../../tools/media-qualification/evidence/2026-10-23-prepared-compound-profile/). It includes the five exact instrumented release source files, original and proposed profile source snapshots with their draft hashes, the `29a318547b491c7265e2c00c1a6dc2d7d18842e4` source-basis record, instrumentation patch and scripts, build and measurement receipts, all nine before/after input maps, reports, logs, and parser outputs. Executable binaries, Cargo targets, the source archive, and generated SQLite project packages are excluded.

The independently re-read inventory lists 129 files totaling 1,718,991 bytes. `inventory.json` SHA-256 is `79090d0ded9ef8894ab40168e8bbe00545d07c758babb9ac84dad77a2a967677`.
