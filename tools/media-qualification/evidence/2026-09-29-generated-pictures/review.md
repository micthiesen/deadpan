# Generated Hold picture integration review

Scope reviewed: current uncommitted Generated Hold picture integration, including `crates/deadpan-cli/src/picture/generated.rs`, `crates/deadpan-models/src/stored_bridge.rs`, the committed-picture session, native preview worker/cache path, generated read handle, and the corresponding render-plan mapping. I read the repository `AGENTS.md` and used Ripwire for the dirty-tree map and caller navigation.

Findings: none in the requested historical-media identity, six-object cold admission, sampled-master timing, revoked-handle, or native-cache identity areas.

Evidence checked:

- Stored provenance verifies the content-addressed envelope and strict schema, binds it to the project and exact authored artifact (native/sample object refs and sampling map), and validates the retained context and worker declarations without consulting current request relevance or a model.
- Cold picture admission snapshots and verifies provenance, context, both conditioning objects, native media, and sampled media. It checks conditioning SHA-256 values, both generated asset records, and freshly opens the sampled master. Decoded codec/color interpretation, frame count, every expected Matroska PTS and frame ordinal, and independently observed terminal duration are checked against the retained contract.
- The plan selects original sampled-master frame ordinals from the retained sampling allocation; shortening or supported re-extension keeps the same artifact and selects its prefix. The picture session is fixed to one document, while the native cache key includes workspace session, complete artifact, both asset records, and color policy, so revision/framing/canvas changes can reuse admitted bytes without crossing project sessions or changed asset interpretations.
- Both cold and warm native paths check handle liveness around work. Object snapshot copying also observes the store's closed flag; cache hits check liveness before and after decoding.

Limits: no compiler or tests were run, as requested. I did not audit the separate footer visibility issue or claim that the focus cue is visually verified. Earlier storage/provenance/reader reviews and their test results were supplied by the parent, not independently rerun here.

## Footer correction follow-up

Scope reviewed: the inactive gain and placed-sounds panel branches, their call order in `DeadpanApp::ui`, and the new footer paint-occlusion assertion and generated-picture labels. No compiler or tests were run; the corrected replay was still in progress.

Findings: no behavior defect found. Both inactive branches still call `.show` with the same stable explicit panel IDs, so the structural panels remain in the UI tree. The gain panel uses `Frame::NONE` and disables its separator only when no draft exists; the active gain-draft panel path and its stored draft handling are unchanged. The placed-sounds branch disables its separator only when the list is suppressed; the active list path is unchanged. The generated-picture check now covers `NORMAL`, `SEQUENCE`, and `Focus: Viewer`; the supplied old replay report shows the intended `NORMAL` failure where a later opaque rectangle covered its text bounds.

Test-helper limitation: `text_paint_visibility` marks a match occluded when any later opaque rectangle intersects the full galley bounding box. This can conservatively flag a visible substring if the rectangle overlaps blank spacing or another part of a composite text galley rather than the target glyphs. It also only models opaque `Shape::Rect` occluders (and shrinks rounded rectangles), so it can miss non-rectangular occlusion and overlaps confined to rounded corners. This is a harness-check accuracy risk, not a product behavior issue.

## Room-tone harness follow-up

Reviewed only the `saved_layout` assertion change and the new rectangle exclusions in `text_paint_visibility`. No source defect found. The scroll loop now continues until the sample range, crossfade note, and both room-tone actions are all painted and unobscured; the final assertions check that same four-label group. The occlusion heuristic now ignores textured and blurred rectangles, so their nominal fill color no longer counts as proof of a solid cover. The earlier false-positive caveat remains for overlaps against a full text-galley bound, but this delta reduces false alarms from non-solid rectangle fills. No tests or Cargo commands were run.

## Compact viewer layout follow-up

Reviewed the `viewer` clock row and copied-moment picture reserve in `crates/deadpan-app/src/preview.rs` (current lines 2529 and 2560-2567), with the compact-mode predicate and copied/playback controls in context. Findings: none. The 14-point row applies only to read-only clocks in the empty-Sounds compact Sequence layout; normal layouts keep egui's interaction height. The wrapped child keeps the same nested UI and explicit LTR wrapped layout used by `horizontal_wrapped`, so changing the row's requested height does not change its widget-ID scope. The copied-moment reserve drops by 24 points only in that same mode, adding the space needed for the minimum picture while leaving the copied label and paste controls in their existing `moment_controls` row. I did not run checks; the parent reports the scoped replay passes at minimum/default sizes and across scale changes. This review does not independently verify that replay.
