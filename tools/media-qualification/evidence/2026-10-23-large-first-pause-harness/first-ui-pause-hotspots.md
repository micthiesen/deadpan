# First-pause latency: route and first diagnostic

Source snapshot inspected read-only on 2026-10-23. The referenced report is a single first-pause pilot, not a quiet/load-qualified latency result.

## What the pilot actually measured

From \`first-diagnostic-report/report.json\`, scenario \`large-first-pause\`:

- One command was admitted and committed. \`hold 12f\` produced 120,012 frames and a new revision; all seven scenario checks passed.
- \`input_to_commit_ms\`: 398.123 ms. \`input_to_offscreen_background_composition_ms\`: 400.722 ms. This is end-to-end input-to-commit and input-to-accepted offscreen composition, not store-only time.
- In diagnostic stage records, command admission is at wall 118.057 ms (frame 132); \`command_committed\` is at wall 511.730 ms (frame 643). The observed interval between those markers is about 393.673 ms. This is a useful bounded interval for finer instrumentation, but not a direct measure of one service phase.
- UI-frame CPU across 643 samples: p50 0.249 ms, p95 0.294 ms, p99 0.892 ms, max 12.021 ms. The input-to-state subset has n=17, p50 0.500 ms, p95/max 12.021 ms. This says ordinary measured frame CPU was small in this pilot; it does not account for background service wait or attribute the 394 ms gap.
- GPU composition submit CPU: p50 0.115 ms, max 1.002 ms. Composition-complete: p50 0.486 ms, max 8.844 ms. These metric summaries pool the pilot's 643 frame samples; they are not a breakdown of the single pause's service interval. The ticket-bound post-commit picture path reports request-to-complete max 7.181 ms across four requests, while the pause-specific input-to-picture complete sample is 400.722 ms. No GPU readback was part of the timing run (\`metadata.gpu_readback_in_timing_run=false\)); the scenario reports offscreen composition acceptance, not scanout.
- The pilot includes setup warnings: 2 frames settled layout on a second retry, plus a 3-frame consecutive retry sequence. It passed the harness's settled-layout checks but is not a quiet first-pause timing trial.

These values do not isolate Core planning, store commit, SQLite, refresh, UI polling, or driver scheduling. In particular, the 398 ms end-to-end number must not be called “store time.”

## Current production route

1. The command parser maps \`hold 12f\` to \`BeatEdit::InsertHold\` (\`crates/deadpan-app/src/navigation/command.rs:1901-1909\`). In the Sequence view, \`DeadpanApp::edit\` routes this insertion to \`insert_pause\`; fixed-duration Hold editing is handled separately (\`crates/deadpan-app/src/preview.rs:1620-1656\`).
2. \`insert_pause\` verifies captured session/revision/cursor/scope and builds \`SemanticInstruction::InsertPause\` (\`preview.rs:1761-1815\`). Here the cursor has no scoped or Visual selection, so it is the root ordinary Sequence path.
3. \`apply_recorded_instruction\` delegates to \`apply_semantic_instruction\` (\`crates/deadpan-app/src/preview/macros.rs:1637-1644\`). That checks pending state and captured context, allocates a serial, creates \`Operation::Apply\`, and submits \`ProjectRequest::Macro\` (\`preview/macros.rs:1733-1807\)).
4. The service dispatches \`ProjectRequest::Macro\` to \`macro_command\` (\`crates/deadpan-app/src/project/service.rs:614-616\`; \`project/service/macros.rs:540-550\`). The Apply/Run path constructs a pre-edit \`RenderPlan\`, resolves scope, builds a one-instruction \`SemanticProgram\`, calls \`deadpan_cli::macros::plan_program_with_transcript_guard\`, compiles a final \`RenderPlan\`, derives final scope/receipt, asks the plan for a preview and runtime bank, then calls \`plan.commit_compound(writer)\` (\`project/service/macros.rs:701-742, 776-805\`). This route performs checks needed for captured authority, exact selection/cursor behavior, macro receipts and atomic compound semantics.
5. On successful commit, semantic repeat proof is recorded against the exact old/new revision; normal service refresh loads the new snapshot/workspace (\`project/service/macros.rs:798-820\`; \`project/service.rs:1796-1820\)). The new workspace then drives the latest-ticket picture request and GPU acceptance represented in the report.

## Why it may exceed direct perf, without measured attribution

The direct \`perf edit\` path constructs a typed \`CommandRequest\`, calls \`store.commit\`, then performs \`snapshot_validated\` + one retained-scope \`RenderPlan::compile\` refresh (\`crates/deadpan-cli/examples/perf/edit.rs:244-297\`). Its pause command is directly \`Command::InsertTime\` (\`edit.rs:338-372\`).

The UI path uses semantic authority and durable macro receipt/compound APIs, with both a pre-edit and final \`RenderPlan::compile\`, semantic program planning, runtime-bank preparation, a compound preview/commit and the service-owned workspace refresh. Those are concrete extra call sites and candidate work. Code alone cannot say how much they cost, whether any are cached/inlined, or whether they explain the gap; the pilot has no spans around them.

## Next useful instrumentation

On the next quiet pilot or controlled series, add monotonic spans around these non-overlapping boundaries in the actual service path:

- queue admission to service-start (distinguish waiting from work);
- \`perform_macro\` total;
- pre-edit \`RenderPlan::compile\` + \`scope.resolve_document\`;
- \`plan_program_with_transcript_guard\`;
- final-plan compile + final-scope/receipt construction;
- \`plan.preview\` + runtime-bank preparation;
- \`commit_compound\` through durable commit return;
- service \`refresh\` through workspace snapshot publication;
- UI receipt/workspace receive to picture request, then ticket-bound picture submit and completion (already partly present).

Emit timings only after each measured span. Keep current captured-context, store validation, receipt, exact inverse and durability paths intact. Compare against the same quiet, fresh-package direct perf baseline. This will distinguish service computation from request queueing and post-commit refresh/picture work without treating an aggregate interval as attribution.
