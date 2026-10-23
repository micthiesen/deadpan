# First-pause service profile, 2026-10-23

This diagnostic profile instruments the project service around the first
12-frame InsertPause in the 10,000-Hold fixture. It compares three root-control
runs using the already-qualified large-first-pause executable, three runs with
profiling disabled, and three runs with profiling enabled. Every run used a
fresh package and process. All nine reports passed seven scenario checks;
source, binary, and authored-input guards passed. The reports and individual
trials are retained in
[the evidence directory](../../tools/media-qualification/evidence/2026-10-23-ui-first-pause-profile/).

| Condition | Input to commit median | Input to offscreen composition median |
| --- | ---: | ---: |
| Root control | 385.551042 ms | 388.289833 ms |
| Instrumentation off | 392.703375 ms | 395.343125 ms |
| Instrumentation on | 388.363209 ms | 391.181084 ms |

Each condition has n=3. These observations are diagnostic, not an overhead
bound, percentile qualification, or target pass. The timer work is not a
randomized or tightly paired overhead experiment. Each scenario still warns
that it is one first-pause operation and excludes native display, scanout,
source decoding, and PCM. No queue-wait measurement exists.

With instrumentation on, median inclusive service time was 381.847917 ms and
median inclusive `perform_macro` time was 377.865042 ms. The measured sequential
`perform_macro` child phases had medians of 0.142500 ms context validation,
5.554167 ms pre-edit plan/scope, 108.376500 ms semantic program planning,
0.000042 ms transcript guard, 7.306250 ms final plan/scope/receipt,
119.775334 ms preview runtime-bank preparation, 130.474125 ms compound commit,
0.311083 ms post-commit receipt work, 4.128667 ms service refresh, and
0.001833 ms post-refresh receipt work. Median perform-macro residual,
including uninstrumented work and timer overhead, was 2.272665 ms. The service
residual for other dispatch work and timer overhead was 0.003000 ms.

The totals are inclusive: do not add `perform_macro` to its child phases. The
named child phases are sequential and non-overlapping. Residuals include timer
bookkeeping and work outside those spans. In particular, queue admission and
wait are not measured. `service_refresh` is a coarse whole-refresh boundary;
these numbers do not attribute time to the store or distinguish substeps
inside planning, preview, commit, or refresh. Timer overhead is uncalibrated.

The profiling executable was built from an isolated source copy based on
`e9be` plus the three final large-first-pause harness overlays. Formatting and
the locked release build both exited 0. The instrumented `deadpan-app` SHA-256
is `997ce5d6499dcb01d59fc9b5a743307f91307e80736fe767882518abf87e62ff`; the
root-control executable SHA-256 is
`3bf2d47c6fbc7eade3a259fc8e9ffc275043af2bd62e698a0b98c0937dcfd9bd`. The
isolated target began as a copy of the root release cache, then Cargo freshly
recompiled the workspace because the source paths differed. Exact original,
proposed draft, and final formatted source files and their identities are
retained. The formatted `service.rs` and `macro_profile.rs` differ from their
proposed drafts; the `macros.rs` draft already matched the formatted result.

All nine runs used private fixture packages. Their main database hashes and
authored input paths/bytes remained unchanged. SQLite's 32 KiB shared-memory
sidecar appeared during each run and every WAL was empty. Each report has seven
passing checks, two layout findings (two frames settle on a second retry and
one three-frame consecutive retry run with distinct causes), and three skipped
scope notes: diagnostic single-run scope, OS picker behavior beyond the chosen
path, and physical display/VoiceOver/IME/audio-device behavior. These are
reported per trial, not hidden by the aggregate. The strict profile parser
passed for all three instrumentation-on reports, with one matched report and
count one for every phase; the six instrumentation-off/control reports had no
profile output. Its synthetic checks also passed for a valid report, a
duplicate report, a missing phase, zero execution count, and overlapping child
totals.

The phase profile shows where time lies inside this service route, but it does
not implement an optimization. Preview preparation and durable commit each
prepare work, so the selected region is prepared twice. A future transaction
callback design could reuse validated preview preparation, but that remains a
design direction. No production latency improvement is claimed by this
profiling-only change.

The verified inventory contains 105 payload files /1,575,313 bytes, with SHA-256
`fce683290a56b68a1af71de6449c0d8fa87e151485dba69dee214bc05163e040`.
