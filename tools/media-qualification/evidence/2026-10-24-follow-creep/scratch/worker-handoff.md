# Follow creep implementation handoff

Worker source edits are ready for Root formatting/build/review. No Cargo, formatter, product runtime or Git execution was performed by this worker. Root owns integration, runtime qualification and final acceptance. No further worker source edits until Root reports a failure/review correction.

API:
- FollowScaleTransition { factor: ExactRatio, curve: framing_zoom::ZoomCurve }, optional FramingValue::Follow.scale_transition defaults None and is omitted when absent.
- EvaluatedFraming<'a> { pose: FramingPose, follow: Option<(&'a TargetId, ExactRatio)> }.
- Framing::evaluate_with_follow_exact(local, duration); evaluate_exact delegates its pose. Scalar arithmetic uses existing numeric primitives; Smoothstep shaping is shared inside framing.rs.
- framing_zoom::Shot.ordinary_source: bool, resolved Source outside occurrence scope. Original range must be None to create a transition; full-range normalization cannot admit it.
- navigation::zoom::follow_scale_label is an App-only reader helper.

Behavior:
- One optional transition, separate authored live/fallback bases and actual scalar endpoints. No pose interpolation for fallback center; exact endpoint contracts retained.
- Plan resolves evaluated live scale using unchanged follow_pose. Lost remains a held target region. Captured-geometry and existing None fallback rules unchanged.
- New ordinary Keep creep records Zoom intent, so macro/dot invocation resolves its own Follow.
- Camera retains authored bases/clock/transition, evaluates live handle at current picture, returns exact entry when unchanged, proportionally scales complete paths, and restores exact Follow centers after scale-only draft quantization.
- Explicit t still saves installed shown Static pose and announces animation removal. Readers show animated endpoints/curve and explain continuing fallback scale.
- No schema bump, following DEVELOPMENT_FORMATS additive optional-field convention. No Store, renderer, target tracker, dependency, crate or structural algorithm changes.
- compound.rs and one_leaf_replay.rs independently rehashed equal qualified wire source; no regression to prior performance work.

New meaningful tests:
Core:
- follow_creep_keeps_both_bases_and_requires_a_whole_ordinary_source
- follow_transition_curves_preserve_exact_fallback_centers_and_endpoint_scales
- follow_transition_non_grid_scales_match_an_independent_integer_grid_oracle
- follow_transition_retained_fractional_clock_and_growth_keep_original_samples
- follow_transition_typed_aggregate_admission_uses_the_real_record_limit
- follow_transition_validates_both_branches_and_closed_raw_vocabulary
- scanner_counts_follow_bases_and_transitions_before_the_extra_record_body
Plan:
- following_creep_keeps_the_lost_region_and_animates_distinct_fallback_scale
- raw_group_follow_transition_survives_ungroup_and_retains_each_child_clock
- following_creep_split_preserves_frames_and_inverse_payload_and_raw_retime_clock
App:
- animated_follow_camera_keeps_entry_clock_bases_and_releases_the_shown_static_pose

The aggregate helper proves the real typed framing-record cap; the scanner test proves real aggregate preflight plus escaped-key/error-before-body admission and null accounting. Neither proves every raw structural ingress. Direct new-feature Command/Subtree/slice/stored-patch aggregate tests, partial copy, Trim/Roll/Slip, Hold/provider changes, isolation/Explode and actual nested Camera outer-clock qualification are not supplied by these helpers. Root integration/native/media tests and independent review remain necessary. Worker has made no test-passing or feature-complete claim.

Owned paths/hashes are in worker-owned-freeze.json (16 files); formatting will intentionally change these draft hashes. Root-owned CLI tests/** and native preview/harness/** were not edited.
