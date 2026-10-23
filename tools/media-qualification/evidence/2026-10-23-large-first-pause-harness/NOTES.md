# Draft handoff: large-first-pause

Scratch-only draft. No repository files were edited. No Cargo, rustc, Git, GUI, server, formatter, compiler, or tests were run. Python only constructed draft files and SHA-256 manifests here.

Deliverables:
- large-first-pause.patch: focused unified patch for three files.
- source-hashes.json: original exact SHA-256 for the two existing patched files, plus draft hashes.
- inspected-source-hashes.json: hashes of key supporting source implementations.
- crates__...rs.rs.txt: complete proposed file contents, with original snapshots.
- draft.py: patch generator with read-only repository inputs and scratch-only outputs. It includes final refinements. Do not rerun refine.py or finish.py; those were one-time edits to this generator.

## Behavior

large-first-pause is explicitly whitelisted as a project fixture scenario. It requires an absolute .deadpan project, hashes its actual database instead of a generated-media manifest, validates it read-only, and uses the existing verified portable-copy API so the input fixture is never edited.

Preflight requires current perf make-large --beats 10000 shape: perf-large-10000, large-root, head large-ungrouped, exactly 10,001 nodes, exact direct Hold IDs/order/labels/recipes, empty assets and audio bindings, three validated chronological revisions, undo available and no redo. There are two legitimate generator commits. It records canonical document SHA-256, head/revision, counts, binding emptiness, full history-validation report and undo/redo facts.

The scenario routes sequence, Escape, gg, counted l navigation to frame 17,933, with Edit/Sequence focus and no Visual/Edit/scoped selection. It settles the current Background first and checks root Hold index 1,494 at offset 5. It then snapshots existing telemetry counts and routes exactly one hold 12f, followed by existing changed()/settled() verification.

It requires one admitted, one committed, no rejected command, a new revision and ticket, exactly one existing completed input-to-commit and input-to-picture-composition sample, same-ticket requested/decoded/submitted/composed stages, and current session/revision/cursor display match through settled(). A retained old Background cannot qualify by remaining displayed.

The result must have 120,012 frames, unchanged unrelated root siblings, local root-owner durations [5,12,7], and the selected middle owner must be the new 12-frame Background/Silence Hold. Background plan samples are checked before/inside/after insertion. Current Split uses Partition Retimes around retained full-context copies; prefix/suffix are checked by duration instead of incorrectly requiring shortened Hold recipes.

Scenario raw metrics:
- large_first_pause_input_to_commit_ms
- large_first_pause_input_to_offscreen_background_composition_ms

The latter explicitly describes offscreen Background acceptance/composition. Existing telemetry semantics are untouched. One invocation performs one insertion, no Undo/warm cycles, and no threshold or percentile PASS. It records n=1/diagnostic-only and the need for 40 fresh invocations per PERFORMANCE.md.

Draft tests cover argument acceptance/required absolute fixture and stale/retained ticket rejection. They are uncompiled and unrun.

## Integration and verification owed

1. Re-read current target files and compare hashes before applying. This draft does not own repository state or qualification.
2. Format applied Rust, compile deadpan-app with ui-harness, run new targeted tests and existing fixture scenario-list test.
3. Start with one fresh diagnostic package:
   deadpan-app --ui-check --mode performance --scenario large-first-pause --project /absolute/fresh.deadpan --output /absolute/new-report
4. Inspect report.json baseline/head/history evidence, command counts, selected Hold, timings and final picture stage tickets. Do not relax assertions silently.
5. Once valid, collect 40 fresh process/package invocations under quiet/load/release gates, retaining failures/reruns. This draft does not aggregate or gate load.

## Uncertainty / limits

- No compiler/runtime verification under the parent's resource restriction. APIs and Split behavior were read directly; no success claimed.
- Portable copy/full history validation warm storage/OS caches before insertion. This is first pause in a fresh process/private project with initial Background settled, not cold disk/startup.
- fixture_sha256 is the main SQLite file hash using existing harness logic. Separate canonical document SHA-256 identifies authored state when WAL affects physical bytes.
- Existing telemetry has ticket identities, not per-event revision fields. Exact final displayed-ticket stage matching combines with typed settled() session/revision/cursor matching. No new timing semantics or product instrumentation.
- Setup command traffic precedes the armed sample subset. Production command-entry timing begins at existing Driver text+Enter batch, matching current edit_latency semantics.
- Offscreen GPU completion is not native-window visibility or scanout. Uncaptioned Background bypasses picture texture rendering, then full offscreen UI composition completes. This is the production Background acceptance path.
