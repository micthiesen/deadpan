# Current leaf timing and owned memory diagnostic proposal

Plan only. No source copy, overlay code, builds, tests, app, profile or memory sampling has run. The only writes are in this new scratch root. Current observations are provisional; wait for Root's final source commit before creating or labeling an immutable2051-Rust-file baseline. That baseline must include qualified full-layout reuse plus selection.rs and tests/semantic_refresh.rs. Do not reuse the old Core24 instrumented source or Cargo cache.

## Preferred small overlay

Five existing source files and one new temporary module:
1. crates/deadpan-app/src/project/service.rs: app-only DEADPAN_CURRENT_LEAF_PROFILE=1 gate on native user-lane Macro dispatch; inclusive outer clock; constant-label buffered output after publish returns and the outer guard drops.
2. crates/deadpan-app/src/project/service/macros.rs: zero-time refresh-success marker after the same refresh call succeeds; keep both original result branches.
3. crates/deadpan-core/src/lib.rs: export the temporary numeric diagnostic module.
4. crates/deadpan-core/src/semantic/planner/pause.rs: bracket only commit_leaf's existing apply_with_result(&self.current, &edit.request(&self.current))? expression, including its argument evaluation.
5. crates/deadpan-core/src/compound.rs: bracket only ordinary replay's existing apply_with_result(&current, leaf)?.1 expression. Keep isolation unchanged.
6. NEW crates/deadpan-core/src/leaf_profile.rs: fixed four-slot numeric TLS measurements, Instant guards, begin/mark/take. No environment, I/O, Store/CLI, IDs, content or dynamic labels in Core.

No Store, command reducer, refresh, audio binding, cache, proof, API plumbing, validation, serializer, authority or runtime adapter changes. All other2051 baseline Rust files remain byte-identical. Root retains Cargo/format/build/measurement ownership. The optional diagnostic export is temporary and does not ship.

## Exact timing contract

schema.proposed.json specifies four rows/six lines under the distinct DEADPAN_CURRENT_LEAF_PROFILE prefix. Each row count is exactly1: app_service_total, semantic_leaf_apply_call, replay_leaf_apply_call, app_refresh_succeeded. Marker elapsed_ns is0. The first three are inclusive call clocks; the two leaf calls are disjoint sequential executions contained in the app outer route. Each guard ends after its unchanged fallible expression completes. Original errors naturally drop active guards.

Outer starts just before service dispatch and ends after existing busy/continuation reconciliation and shared publication. It excludes queue wait, caller receipt processing and downstream UI/GPU/scanout. Core leaf clocks include full apply entry/reducer/net/result work and caller argument setup, but exclude later semantic size charge/refresh or Store visitor/final preparation. They must not be labeled reducer-only costs.

Only app reads the environment and activates the thread-local buffer. Emit once after successful native publication, after outer drop, requiring exactly one refresh-success marker; otherwise consume/discard. Best-effort stderr writes cannot change saved results; absent/partial blocks invalidate a trial. Output occurs on the service thread after the operation, never the UI thread. It can overlap UI consumption, which remains a calibration limitation.

Require exact ordered constant rows and one BEGIN/END, full-match decimals, reject unknown/duplicate/missing/count-changing rows, nonzero marker and negative containment. Compute app_service_total minus the two disjoint leaf calls per trial only as other service work/cleanup/overhead. Never subtract overlapping inclusive values or subtract reported medians to invent attribution. Do not label the residual refresh, Core, Store or any individual algorithm.

The gate-disabled private binary must emit no block and create no clock, buffer activation or diagnostic output; Core guard sites retain disabled TLS/branch overhead. Compare Root control/private off/private on in rotated fresh-process triplets using the unchanged exact native first-pause producer and seven replay checks. All source/binary/six-input identities, authored-input equality, empty WAL, scenario count, release receipt and quiet-load gates remain mandatory. A cached/rejected/multileaf/multiple-Macro run invalidates this one-call contract.

## Owned memory observation without runtime adapter changes

Use a scratch external Rust observer, compiled only after review by Root against the exact locked deadpan-native-process rlib from the copied source. It is diagnostic runner code outside the source tree, not a new production crate or Cargo dependency. Do not copy or modify the adapter. The observer directly launches the app with deadpan_native_process::spawn, retains its Child and sole reaping ownership, and invokes existing owned_child_memory(&child) from immediately after spawn until Ok(None). Only then wait/reap once. It must not wrap the app in a shell/time process or sample arbitrary PIDs. Configure a new owned process group; timeout cleanup uses existing checked group teardown and leader fallback before the sole reap.

Sample at requested5ms intervals, retain actual monotonic sample timestamps/gaps and both returned phys_footprint/resident_size values in a bounded60000-row buffer. Write raw samples and summary only after exit; sample failures remain explicit, never zero. Maximum runtime180s; reaching time/sample bound or teardown/sampling failure invalidates the memory trial. No pipe polling/log writes in the app or UI, and no memory query in a leaf-timing span.

This adapter exposes instantaneous physical footprint and RSS, not OS high-water fields. Report observed_max_phys_footprint_bytes and observed_max_resident_size_bytes with sample count, start/end times and maximum gap. These are sampled lower bounds for the owned main app process over its entire replay lifetime. Brief peaks can be missed; startup/open/fixture copy, other app threads and post-publication rendering are included. Child workers/escaped processes and GPU/service memory outside the app are excluded. There is no per-leaf or pause-only peak, retained-allocation or cache-size attribution. Do not sum separately timed worker maxima into an invented concurrent peak.

Prefer separate lanes: timing triplets without the observer, then memory triplets with it on the identical fresh workload. The latter can quantify the instrumented app's observed memory but cannot replace the former timing controls. Three rotated triplets per lane match the old diagnostic convention; Root may stage one smoke triplet first. Sampling itself adds CPU/syscall load, so report it as unqualified until matched observed/unobserved controls run. No baseline gain, latency target or memory ceiling follows from this plan.

## Review gates before implementation

Root first approves this exact six-file timing scope and the whole-process sampled-memory limitation. Root's final source commit then supplies baseline/source2051 map, six input hashes and executed control binary hash. Recheck the exact anchors/hashes immediately before patching. Retain before/proposed .rs.txt snapshots, pre/post-format patch/hash maps and full nonowned-file audit under this root. Build a separate private target; no old overlay or source/cache clone.

Proposed scratch runner outputs after approval: memory_observer.rs (owned-child wrapper), measure.py, check_profile.py and parser synthetic cases. Those files are not written yet. Parser must reject duplicate blocks, altered counts/kinds, nonzero marker, missing/out-of-order rows and containment violations before any real trial. Any unsupported observer linkage or adapter permission must be reported as missing evidence, with no adapter/API change inferred.

Risks: stale baseline until Root commit; timer/TLS/output/sampler overhead; whole-lifetime sampled memory is not exact peak or phase memory; operational input/quiet-load gates remain independent correctness evidence. All costs and gains are unmeasured.
