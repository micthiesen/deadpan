# General review: target-following creep

Scope: requested uncommitted production files and the new CLI/App follow-creep tests, against HEAD `8b8bfec40a9260ac9245b9b8aff70f69cfa6bc22`. Design/spec/FRAMING contract checked. No repository edits, build, formatting, test execution, or runtime use.

## Confirmed production defects

None found in the reviewed paths. The new representation preserves separate live/fallback bases, evaluates both on the stored owner clock, keeps Lost target handling on the existing `follow_pose` path, and routes plan and Camera evaluation through the same Core evaluator. Admission checks retain explicit Visual range information and reject non-ordinary/scoped creation. Fixed Follow wire shape remains unchanged when `scale_transition` is absent.

## Key test gaps (not confirmed defects)

1. `crates/deadpan-core/src/framing_zoom.rs` / `crates/deadpan-cli/tests/project_commands/macros/framing/follow_creep.rs`: add the explicit `from=S` path with live and fallback bases that are unequal. Current semantic integration exercises only `from: None`; the specified `F × (S/L)` fallback rebasing and `to/S` factor are not directly covered.
2. `crates/deadpan-plan/tests/picture_plan/follow.rs`: add direct raw transition cases for a Hold owner with captured geometry and for a Repeat owner (including its gap), plus nested Follow owners with distinct local clocks. Current additions exercise Source, Group, and Retime, while captured Hold suppression and Repeat-owned timing are called out as invariants in the design.
3. `crates/deadpan-app/src/preview/harness/zoom/follow_creep.rs`: cover an actual Camera scale edit, explicit target switch, and release followed by adjustment/re-follow. Existing harness covers unchanged Enter, release/Escape, save, undo, macro, and dot; unit tests cover draft transformation but not these routed native actions together.
4. `crates/deadpan-core/src/framing/preflight.rs`: test aggregate framing-record limit across mixed fixed Follow, animated Follow, and Envelope records, including escaped `scale_transition` spelling. The new scanner test covers fixed/transition accounting and escaped fallback, but not mixed shapes or escaped transition key.
