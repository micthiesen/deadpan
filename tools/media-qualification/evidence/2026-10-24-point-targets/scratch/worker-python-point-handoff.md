# Cross-crate point audit and Python bridge repair

Read-only Rust audit found CLI/Analysis/Plan/Models paths handle points without fake rectangles: CLI saved-point correction refuses before preparation/worker; target_rect_at is None for points; Analysis retrack rejects points and rectangle compaction explicitly matches Region; face proposals stay rectangles; Plan uses generic exact center_ratio; Models captures selected PointTarget as unavailable while retaining target identity. Core58/SQLite87 untouched in this repair.

Confirmed bridge gaps, now fixed:
1. tools/model-qualification/worker.py validate_context_region rejected Rust's new unavailable reason point_target. Added only that closed reason. Actual bridge and extension context validators now accept the truthful unavailable state.
2. worker_extension_context.py target_record required center+size for initial geometry, samples and corrections. It now accepts closed center-only points or original rectangles, validates exact two-coordinate bounds, and requires uniform geometry kind. Points remain size-free in canonical bytes.
3. The same retained identity parser omitted existing optional display_aspect, which native saved points carry. It now validates Core's 1/64..64 and reduced term <=2^48 bounds, reduces exact fractions, and serializes this optional field last in Rust AttentionTarget field order. Old rectangle bytes without aspect remain identical.
4. A point identity is explicitly forbidden from claiming any available area seed in extension binding validation. Available area geometry remains rectangle-only. No ROI is synthesized.

Modified scope: six paths in worker-python-point-freeze.json. Five are Python implementation/tests/fixture, one adds a Models focused Rust test sharing that fixture. No other Rust production changes. No Git/Cargo command or formatter was executed during this repair.

Verification:
- PYTHONDONTWRITEBYTECODE=1 python -m unittest discover -s tools/model-qualification/tests -p test_extension_worker.py:39passed, exit0, 0.142s.
- PYTHONDONTWRITEBYTECODE=1 python -m unittest discover -s tools/model-qualification/tests -p test_worker_protocol_v2.py:23passed, exit0, 0.008s.
- New cases pass through actual worker.validate_bridge_context and worker.validate_extension_context/validate_continuity, including retained point+samples+correction+measured aspect canonical SHA, stale identity rejection, uniform-kind/bounds/unknown fields, unavailable PointTarget, fake available ROI refusal and old rectangle byte stability.
- Shared fixture tests/fixtures/point-target.json is the expected Rust canonical AttentionTarget wire. Models test point_target_identity_bytes_match_the_python_bridge_golden asserts actual Rust serde bytes match it. Root owns its pending run after current Core/Plan Cargo session; no coverage claimed until that passes.
- No Python test failures. One apply_patch anchor miss after Root formatting was re-read and corrected; it made no source changes.
- No native inference, model loading or encoded export was run. No active worker processes.

All six files frozen for Root review/format/qualification. Remaining runtime source hashes/bundle qualification belong to Root; ADAPTER_SOURCES naturally hashes the changed Python helper bytes.
