# Repaint waits and worker timing evidence

See [the qualification record](../../../../docs/qualification/repaint-wake-2026-09-26.md) for the exact scope, review fix, failed attempts and remaining host checks.

The original gate and final feature checks retain separate source manifests. Logs and blocked replay reports are gzip-compressed. `incremental-source.patch` identifies this increment against the prior source checkpoint; it is not a patch against Git HEAD. The first app test log retains the corrected pinned-egui API compilation errors. No file here establishes a new GPU latency or aesthetic result.
