# Focused qualification batch

Frozen file list is worker-qualification-batch-freeze.json (seven paths). Three new test modules plus their minimal declarations; existing Plan Retime oracle strengthened; one separately authorized Camera production correction and regression after Root's actual native replay failure. No Cargo, formatting, product runtime or Git execution by this worker.

New test modules:
- Core tests/framing/follow_boundaries.rs: raw Command/Insert Subtree/captured slice/stored patch reject an invalid fallback endpoint; stored patch before image independently rejects; valid raw Hold Follow commits/reopens/undoes; escaped transition key round-trips and unknown collection/control fields reject; actual 100,002-record incoming Subtree rejects at real framing aggregate limit before prepared subtree cloning.
- Core src/framing/follow_preflight_tests.rs: mixed fixed Follow (1), animated Follow (2), ordinary one-segment Envelope (2) fill the actual 100,000-record scanner budget; escaped scale_transition is counted; next malformed body refuses on record admission. Test explicitly does not call empty scanner bodies valid typed documents.
- Plan tests/picture_plan/follow/structural.rs: seven actual-command/math cases listed below.

Plan cases:
1. Step, Linear, Smoothstep propagate independent live and fallback scale oracles into PictureFraming.
2. Nested Source/Repeat/group use explicit distinct owner extents and positions, including raw Repeat gap fallback and second-play source restart. Scale and SourcePoint oracles are independent of evaluator.
3. Raw Hold with captured geometry suppresses target following while fallback scale changes; actual SetHoldProvider preserves geometry, poses and duration.
4. Actual cropped slice paste and Duplicate retain selected Original points, target and scale phase.
5. Actual qualified, complete explicit linked A/V Trim In, Trim Out growth, Roll and Slip preserve scale clocks on surviving frames and exact audio SourcePoint phase. Growth holds terminal scale. Slip intentionally changes target center. Source selection changes are independently checked rather than demanding whole Picture equality.
6. Actual EditOccurrence Rename isolates second play; actual Explode retains each picture/pose and repeat-gap clock; every command checks inverse and JSON reopening.
7. Actual InsertTime captures the evaluated Follow pose in a silent freeze, preserves subsequent Source points/poses, and actual SetHoldProvider keeps capture independent of provider.

Existing raw Retime test now asserts local=(2f+1)/2, mapped Original ticks=(2f+1)*1001, expected scale=(9+2f)/4, and known target center on either side of its source-time sample step.

Root reported first Core boundary run passed all 21 tests. Root reported first structural run passed 17/18; the one failure was the oracle demanding unchanged full Source selection after legitimate Trim growth. It now asserts exact Original point/identity/full affine span/endpoint policy/poses plus independently expected changed selection (left growth 0..10010, surviving right 8008..16016). Rerun remains Root-owned.

Root's first native replay found a production defect in my earlier center-preservation branch: set_pose returned None when centers were already equal, discarding the original Scale PoseChanged. This skipped complete transition validation and picture preview. Corrected with retain_follow_scale_effect: successful unchanged restoration yields PoseChanged(current pose), other effects including Rejected propagate. New unit exercises grid/non-Q32 centers, successful Scale effect, retained restoration effect, 75-step current-valid/later-endpoint-invalid transition and preservation of restoration rejection. Root owns routed native rerun; first-failure evidence remains retained.

Omissions/limits:
- Independent maximum-sized aggregate fixtures for stored patch and slice are not added. Their new invalid-endpoint cases reach those actual public ingress/apply boundaries; real aggregate cap is directly exercised through Subtree and mixed document scanner, without lowering caps or claiming one proves all others.
- Actual Wrap/SetRetime commands are not added in this batch: raw Retime owner timing is independently covered, and Split/retained growth/Ungroup were already covered. Full retime lifecycle regressions may be extended if review finds a distinct transform risk.
- Picture plans and exact audio SourcePoint mappings are pure evidence; this batch does not claim decoded PCM, emitted pixels, physical display or native routing success. Root owns actual-media/export/native qualification.
- No production structural algorithm changed. Lost sample regions retain their existing validated last-confident-region contract; no sentinel change was made.
