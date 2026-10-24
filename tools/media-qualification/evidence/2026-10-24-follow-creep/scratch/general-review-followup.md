# Follow-up review: Camera scale effect and native Follow creep replay

Scope: `retain_follow_scale_effect` and its `camera_input` caller in `crates/deadpan-app/src/preview/camera.rs`, plus `crates/deadpan-app/src/preview/harness/zoom/follow_creep.rs` and its registration. Read-only; no tests or runtime executed.

## Confirmed production defects

None found in this narrow scope.

The helper preserves `Rejected`, `Reset`, and other effect variants, and only promotes `None` to `PoseChanged` after a scale input had already returned `PoseChanged`. This lets the caller run complete Follow validation and preview after restoring an exact target-owned center. If complete framing validation fails, `camera_input` restores the pre-key draft and returns before submitting a preview, preserving the old presentation. The unit cases cover both an exact grid center and non-Q32 center, plus the later transition endpoint bound.

The expanded replay exercises a valid routed scale edit and checks proportional live/fallback bases with unchanged center, clock, target and transition; exercises endpoint-bound refusal with unchanged revision and draft pose; and covers target-save continuation, target switching, unchanged Enter, release/Escape, explicit static release, release-adjust-refollow, Reset, Undo, macro replay and dot replay. The cases align with the design contracts. No outcome assumption is made here because the replay was not executed.
