# Hold Follow creep and captured geometry

Read-only map. No source/Cargo/Git writes or builds/runtime jobs.

## What :hold captures

- Native insertion flows through crates/deadpan-app/src/project/pause.rs:75-124. Ordinary non-black pauses call fallback() at :192-210; black pauses explicitly use Background + picture_context=None (:112-117).
- fallback calls deadpan_cli::pause::pause_provider_within. In crates/deadpan-cli/src/pause.rs:84-117 it samples the preceding displayed picture and resolves insertion parent; Source/Freeze selects freeze() (:104-116).
- freeze() at pause.rs:120-168 captures a CapturedCanvas for every Source/Freeze, including an unframed one: it captures canvas dimensions, Fit and an empty layers vector when there are no inner poses (:134-155). Thus ordinary identity/unframed :hold is still picture_context=Some; it preserves fitting/letterboxing. Only black/background pauses and empty-plan fallback are None (:85-90, :169-172).
- HoldRecipe stores that context at crates/deadpan-core/src/document.rs:420-428. Plan compilation copies it to CompiledKind::Hold at crates/deadpan-plan/src/plan.rs:818-827; sampling returns it with the hold picture at :1613-1622.
- The plan calls resolve_follows(..., picture_context.is_some()) at plan.rs:1745. resolve_follows returns before reading any Follow target when captured_geometry is true (:1299-1311); the comment states it is between picture and every framing layer. All Follow layers then retain evaluated fallback.

## Camera behavior on a captured Hold

- Camera opens against the selected Hold framing layer and composes using picture_context (crates/deadpan-app/src/preview/camera.rs:666-713). Target picker entries project saved-target centers through that captured input geometry (:715-730), so a target can be visible/selectable and its center appears in the actual captured Hold canvas.
- Camera’s display-time Follow resolver separately sets captured=true and returns fallback (:548-557). target_center_at rejects any captured Hold picture before target resolution (:1616-1638). Camera t can still create a Follow from selected target if its asset matches the picture asset (:2288-2310), but this does not make the Follow live on Hold. Hold’s Follow is currently fallback-only.
- Therefore a captured Hold may carry a syntactically valid Follow/FollowScaleTransition, but it cannot follow the target center. Creep can animate the Follow fallback scale across Hold OwnerOutput; the captured inner geometry remains unchanged. Do not describe this as live target following.

## Smallest honest qualification

1. For native replay, create an ordinary visible :hold from a Source that has a saved target and a visible framed view. Assert recipe.picture_context is Some and unchanged; select target in Camera (picker projection works), t, Enter, then :creep … Keep. Verify exact FollowScaleTransition, OwnerOutput duration, target and fallback bases; sample Hold start/middle/end and assert plan resolves the fallback (not target pose), while captured context and frozen picture stay the same. This proves a Hold owner-clock scale transition preserves captured geometry and truthfully exercises the fallback path.
2. Separate “actual Follow” unit/plan case: construct an ordinary Hold with Freeze provider and picture_context=None plus a matching source asset/point target, and assert plan resolves target center through its scale transition at start/middle/end. Existing project fixtures already construct Hold Freeze with context None, but this is not reachable through normal native :hold insertion for Source/Freeze. This proves the plan’s uncaptured Hold path, not native workflow.
3. If the feature is meant to be called “Keep target creep” on native :hold, captured-geometry Follow projection is a prerequisite. Map the target’s source point through the captured canvas fit and each captured pose, then through live outer framing in plan and Camera; maintain bars/clipping behavior and exact rational transforms. Replace the coarse captured_geometry early return only after defining how target coordinates traverse CapturedFraming. Until then, scope UI copy/tests as captured-view/fallback animation, or keep native Hold excluded from Follow creep admission. Do not let a raw Hold fixture with picture_context=None stand in for the user’s normal Hold path.

## Test anchors

- Existing resolution refusal: crates/deadpan-cli/src/framing/tests.rs:238+ (semantic_framing_keeps_captured_hold_geometry_and_refuses_a_clipped_center) tests captured geometry and target center limits.
- Native Hold context construction: crates/deadpan-app/src/project/tests/pause.rs:1435-1457 confirms black pause has None; :164-215 checks an ordinary inserted hold preserves captured framing and picture.
- Current Camera target center refusal and fixed target picker projection: crates/deadpan-app/src/preview/camera.rs:715-730, 1616-1668.
- Render-plan Follow early return: crates/deadpan-plan/src/plan.rs:1299-1311; caller derives flag at :1745.
- Core creep admission now appears concurrently broadened in the shared checkout: crates/deadpan-core/src/framing_zoom/tests.rs names follow_creep_keeps_both_bases_and_requires_a_whole_ordinary_source_or_hold. I did not alter or validate that work. The native captured-geometry limitation above still applies.

