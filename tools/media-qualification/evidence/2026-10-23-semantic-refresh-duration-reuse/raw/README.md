# Semantic refresh duration reuse qualification (draft)

This directory contains adapted qualification drivers and frozen qualified-control manifests. The drivers are not authorized or run by this draft task.

The control is the previously qualified full-timing-closure build, implementation `ae5eedd065b851242917b38a428ee40a3588978d`, app SHA-256 `2761f275c70d471b202ffdf2faa9e047724a169c9607775ff4b4ee6208942608`. Its source and six input identity maps are retained as `qualified-control-source-identities.json` and `qualified-control-inputs.json`.

If the private candidate is adopted for qualification, the expected source-set difference from the control is one changed file, `crates/deadpan-core/src/semantic/planner/selection.rs`, plus the new `crates/deadpan-core/tests/semantic_refresh.rs` integration target. That changes the Rust file count from 2,050 to 2,051. The four added regression names are checked by `post.py`; no result counts, runtime results, or performance claims are asserted here.

`qualify.py` defines the eight-stage sequence: Core, broad, UI, strict Core, strict UI lint/format, debug media, release media, and release-native replay. `post.py` defines the subsequent portable-copy check, five native replay scenarios, and five alternating control/candidate pairs. All fixtures, reports, source maps, binaries, and timing measurements remain pending.
