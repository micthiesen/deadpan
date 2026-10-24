# Whole ordinary Hold Keep Follow creep: Core/CLI

Six owned paths and source hashes are in worker-hold-freeze.json. No App/Plan production or tests, docs, point evidence, dependencies, Cargo.lock, foreign fuzz or .claude edits. No Git writes or formatter. Root owns final formatting/review/native qualification; captured_target_projection owns all Plan changes.

API: replace Shot.ordinary_source with ordinary_owner: Option<OrdinaryFollowOwner>, enum derives Clone/Copy/Debug/PartialEq/Eq and has Source/Hold. Semantic ordinary planner classifies actual NodeKind::Source/Hold; all other kinds and scoped planner pass None. Builder accepts Some only with original range None, unchanged plain-Follow eligibility. Refusal wording names whole ordinary Source or Hold. No serialization/schema/timing/payload change.

Core tests:
- Existing builder test now executes the same live/fallback/explicit-from/path/refusal assertions for both Source and Hold.
- New semantic actual-Hold test resolves owner, preserves entire node except framing, owns transition over intrinsic eight-frame output, checks independent live/fallback scales, replays Compound/inverse identity, rejects full/interior/reverse ranges, existing transition and noneligible Sequence owner.

CLI integration:
- Actual qualified Original and retained metadata/index. Three ordinary Freeze Holds: live left target, other right target with different base, clipped captured-view fallback.
- Named macro Save/Run, dry-run unchanged package, predicted framing/trace equality, scale endpoint blend, complete Hold node/provider/audio/geometry and picture identity retention.
- Durable Undo/reopen/Redo; refusal on an existing transition; same program rebinds invoked target/base; inline semantic Apply replays the boundary native dot uses. CLI has no persistent dot register, so actual native dot is Root's witness.
- Full/interior/reverse range refusals and an actual single-Repeat-play Hold scope refusal preserve whole SQL package/register state.
- Captured canvas deliberately has central scale4 crop [.375,.625], target x=.3 outside. Final fixture records measured aspect from registered qualification visible bounds/SAR/rotation, with source-aspect>=project assertion, so fallback is due clipping rather than missing aspect. This remains honest after new Plan capture projection; it does not claim every capture falls back.
- Captured fallback expectation independently blends separately quantized .6 and1.2 endpoints, preserving exact fallback center.

Executed:
- cargo test --locked -p deadpan-core --lib follow_creep:2passed, exit0.
- cargo test --locked -p deadpan-cli --test project_commands hold_follow_creep: first failed on fixture root literal; corrected actual root; second passed1, exit0 (1.10s).
- Exact commands/env/streams/hashes/terminal IDs in freeze manifest. All worker sessions completed.

Pending final CLI rerun: the passing run predates final measured-aspect strengthening and Plan projection integration. Root/Plan worker own shared Cargo now and must rerun final CLI filter; no final measured-aspect success claimed yet. Core/CLI production is frozen. All changes preserve exact owner clocks, Follow transition representation, live/fallback bases and retained authored timing/provider/audio bytes.
