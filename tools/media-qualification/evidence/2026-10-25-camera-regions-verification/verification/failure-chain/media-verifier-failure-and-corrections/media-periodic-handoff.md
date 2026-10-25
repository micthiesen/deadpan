# Periodic segment assertion correction

Frozen media.py replaces only the overly strong segment acceptance assumption, with an explicit helper and truthful alignment summary. The prior4fc79ca0driver and failed actual run/report/commands/failure streams are preserved verbatim under media-periodic-before; original media-final files remain untouched. No product source changed. No Cargo, CLI, detector, render or verify-export was rerun in this lane.

Actual retained result:
Production verify-export exited0 and passed all120picture comparisons and the complete192192sample signal window at measured aggregate offset0. It compared401blocks, worst block SNR36.26551241453646dB, max block level difference0.12231543447524729dB, with zero content/level/silence failures. One4096sample segment beginning45056 has a unique peak at0; seven segments are honestly periodic, including nonzero dominant maxima near their equivalent tone periods. Those seven are not independent zero-offset measurements. The prior driver stopped after production verification because it demanded every segment be uniquely verified_zero. This is a verification-driver error, not a production verifier failure. The five portable copy/validation/dump/checkpoint operations have not run.

Exact current policy sources:
- crates/deadpan-cli/src/export_verification/audio_check.rs:64-104 selects decoded samples at the declared window coordinates and compares complete PCM/block content before any correlation analysis. Audio is not shifted to a correlation peak.
- audio_check.rs:144-175: any Offset wins and fails; otherwise at least one VerifiedZero establishes aggregate VerifiedZero; all-periodic is Unobservable and fails; any Uncorrelated segment adds a failure flag.
- crates/deadpan-cli/src/export_verification/metrics.rs:260-275 defines Periodic as individually unobservable, never offset0.
- metrics.rs:372-387 permits Periodic only with a separated local maximum within correlation1e-3 of the peak and zero-lag correlation also within that same existing periodicity criterion. This is a correlation-observability test, not a time-offset tolerance.
- metrics.rs:452-500 compares every480sample block at unchanged coordinates for SNR/level/silence.
- crates/deadpan-cli/src/export_verification.rs:174-192 defines unchanged default gates.
- crates/deadpan-cli/src/export_verification/tests.rs:394-458 preserves regression policy: pure periodic windows fail, one identifiable transient permits aggregate observation, and shifting it1024samples fails.
- docs/PREVIEW_EXPORT_VERIFICATION.md:166-211 documents those exact per-block/segment/window policies and forbids shifting/trimming.

Corrected helper requires:
The full signal window0..samples with exact project coordinates, zero uncovered samples, passed/empty flags, aggregate VerifiedZero and measured0, actual default threshold values, complete480sample block accounting, no low-SNR/level/sound-in-silence block, finite gated SNR/level extrema, at least one unique zero-lag segment, and only VerifiedZero or source-policy-consistent Periodic segments. Any Offset/Uncorrelated, purely periodic window, weakened thresholds, missing block/coverage, or periodic maximum without equivalent zero correlation refuses. No samples are shifted, events aligned, period rounded away or nonzero-lag tolerance introduced. The result records identifiable segment starts and periodic segment starts/dominant lags separately; original report states remain unchanged.

Pure scratch assertion regressions:
python media-periodic-regressions.py imports definitions without running the driver main and rechecks retained JSON only. It passes the actual retained report and rejects14negative mutations: aggregate offset, missing coverage/window/block, SNR/level/silence failures, weakened threshold, nonzero unique witness, Offset/Uncorrelated status, invalid periodic zero correlation/separation and missing identifiable witness. Exit0. Python syntax compilation exits0. These are assertion regressions, not a verifier/decoder rerun.

Root should review new media-periodic-freeze.json/policy source snapshots/.patch and execute the corrected driver with a NEW output label if full15operation qualification is desired. Keep the retained first run as evidence. The release-only, all5binary/native bindings, full8table/atomic history, picture provider, readonly detector and portable contracts remain unchanged.
