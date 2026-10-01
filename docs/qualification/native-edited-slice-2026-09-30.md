# Native edited slices, 2026-09-30

`v`, motion and `y` now copy an edited interval without writing history. The
session register retains its exact historical revision through subsequent edits
and Undo. `p/P` inserts it at a seam or replaces an Edit selection; `:splice`
shows copied endpoint pictures, historical In/Out refinement and the proposed
destination before one atomic commit. The [contract](../EDITED_SLICES.md)
defines ownership and the operations still required.

## Historical media and picture identity

The store recaptures the immutable source revision before issuing an opaque
preview view. That view carries its exact document, Original receipts and
accepted generated objects. Playback retains strict validation for ordinary
Original proposals; historical edited proposals require their distinct sealed
view. Closing the source session revokes temporary edited views on cold and
warm access, including cached PCM delivery. Existing ordinary committed warm
private PCM behavior remains unchanged.

Source refinement materializes a neutral document from the captured parent.
It excludes unselected historical ancestors and destination ancestors while
preserving owned framing, freeze geometry and owner clocks. Source admission
precedes destination preflight, so unsupported destinations retain endpoint
inspection and refinement. Destination-only changes reuse the admitted source
view. Copied pictures have their own presentation identity and caption and
cannot provide a committed Camera target.

Real worker fixtures check exact Source ordinals and PTS, historical media after
Undo, Source/Freeze/Background endpoints, framing scopes and revocation. Full
receipt reconstruction occurs once per immutable view; a regression test checks
the count across repeated decoded seeks and new copied/proposed views. Store
tests cover historical generated admission. This milestone does not add a new
decoded generated-Hold copy fixture or AI-generation qualification.

## Review and corrected failures

Independent backend review found no actionable admission or playback defect.
Picture review found a full audio-index reconstruction on each warm picture.
The worker now validates the complete catalog at immutable-view admission and
keeps the existing constant-time checks on warm pictures. The reviewer checked
the correction; 25 focused worker tests pass.

Review also found three asynchronous UI defects: a rejected newer yank left an
older copy pending; a saved placement receipt cleared selection before its new
workspace arrived; and a coalesced copy completion could overwrite saved-refresh
guidance. Production action ingress now supersedes old copy intent. Selection
changes require the visible saved revision, and copy feedback preserves an
outstanding saved warning. Replay uses a genuine service-issued capture, a real
commit with deliberately delayed workspace delivery, and a separately labelled
simulated warning case. Pending pictures and endpoint errors are also gated by
the current draft identity while the last displayed picture remains available.

The final review also found that visible placement retained a successful receipt
but exposed only a raw refresh error after saving. It now reports that the slice
was saved, retains the cause and directs reopening, matching fast paste. Ten
focused refresh-failure tests pass, including both Original and edited visible
placement, duplicate receipts, copied feedback and later reopen/Undo.

Initial test compilation exposed an invalid binary-only `--lib` invocation,
three test ownership/serialization errors, an invalid qualification ID, and five
old harness references to the Original-only register. These were corrected.
Two new fixtures then failed: Undo changed an automatically adopted canvas/rate,
and a captured canvas used invalid odd dimensions. Fixing their setup produced
399 passing UI-feature tests. The later worker admission regression adds one
test, covered by the separate 25-test worker run. No passing suite is inferred
from an earlier failed invocation.

A playback oracle initially selected a silent interval of the impulse fixture;
it now includes the documented event and all seven proposal tests pass. Existing
committed warm-PCM expectations were retained after correcting an overbroad
interpretation of temporary-view revocation.

## Rendered interaction

The final `place-slice` replay passes 547 checks plus the 11,904-case Kestrel
routing audit. It copies an Edit interval containing an authored 11-frame silent
Hold, refines its historical boundaries, rejects an unsupported Repeat interior
without losing endpoints, inserts and replaces through production keys, undoes
exactly and inspects the historical copy after undoing the original Hold.
Original-copy and range-delete regressions pass 16 and 97 checks respectively,
each with the same routing audit. Audition transport delivery in this replay is
injected; decoded PCM tests provide separate media evidence. The retained-project
run includes one additional storage-root check, for 548.

The first slice replay failed an image assertion that expected the endpoint
texture to fill its entire allocation. Inspection showed correctly fitted
canvas imagery. The corrected oracle checks the exact aspect-fitted bounds,
texture identity, viewport/paint clips and absence of later opaque coverage.
Production code did not change for that failure. All failed output is retained.

The broader release replay found minimum-picture failures in Sound placement
(117 versus 120 points), Room tone (133 versus 140) and Gain (136 versus 140).
One diagnostic replay using that exact release binary reproduced all three and
retained screenshots. Compact viewers now use 8-point vertical outer margins
instead of 12, recovering 8 points for the picture while preserving control
reserves, text and hit sizes. The default-size margins remain unchanged.
Corrected visual runs pass 216 Sound, 220 Room tone and 290 Gain checks; minimum
picture heights are 125, 141 and 144 points. Their final captures were inspected.

Final 960×640 and 1280×820 endpoint captures and the committed workspace were
inspected against the workspace design target. The copied source strip, large
fitted picture, unsaved state, keys, distinct copied/destination clocks and
provisional timeline remain visible without overlap. The default committed
workspace preserves the editable copied group and its inspector. This does not
establish complete native accessibility, IME/layout or acoustic acceptance.

## Isolated native keyboard check

A unique temporary release bundle opened a private copy of the retained replay
project. Production keys inserted the fixture Hold, copied `[8,25)`, refined to
`[9,27)`, cancelled, reopened the unchanged register and committed at Edit 60.
The native viewer showed endpoint slates `009`/`015`, and the committed inspector
reported an 18-frame Sequence with three editable children at `[60,78)`.
After two Undos, the register still showed historical endpoint `013` for copied
Edit frame 25, retaining the original Hold's effect on timing.

Consistent SQLite backups show no changed cells across all 20 tables for copy
and cancel, exactly one placement revision, complete authored restoration after
placement Undo, and complete baseline restoration after undoing the fixture
Hold. Each Undo uses its required fresh revision. Sixteen unrelated tables stay
identical throughout. The temporary app exited and released its writer lock.
The user's separate Cursor QA window and desktop remained untouched.
Native screenshots were inspected, but the visible-screen capture clipped the
right part of the large window; full layout evidence comes from Metal replay.
No device listening, native IME or complete accessibility claim is added.

## Release replay

The corrected full release replay passes 3,110 checks, including the shortcut
audit. Its optional generated-picture scenario is explicitly skipped without
the accepted-bundle fixture. On Mac17,7/arm64/macOS 26.5.2 with AC power, warm
navigation p95 is 1.51 ms through offscreen GPU completion and 0.15 ms for input
CPU. Cached Repeat p95 is 5.53 ms and Hold fallback p95 is 5.51 ms. The 10,000
Background/Silence-beat fixture has 0.35 ms navigation CPU p95. These are the
existing bounded fixtures and unchanged targets, not full-size media, physical
display or acoustic measurements. Complete samples and skipped scopes remain
in the replay report.

## Integrated verification

All 2,766 locked workspace tests pass, with none failed or ignored, in 734.26
seconds. Formatting and strict all-target workspace Clippy, including the optional
UI harness, pass on Rust 1.97.1. They took 1.49 and 620.47 seconds respectively.
The checks use source manifest
`65fa24bf37e2a6c647c092a52d5ebf9196b48605b98e2d4107034a48da52dbe6`.
Debug app links report an oversized `__eh_frame` warning; no lint
suppression was added, and the release build completes without that warning.

Final workspace, release-performance and isolated native results are recorded
with [retained evidence](../../tools/media-qualification/evidence/2026-09-30-native-edited-slice/README.md).
Core 34/database 43 are unchanged. Atomic move, cut-to-register, persistent/named
registers, role-only placement and temporal occurrence interiors remain open.
No DP requirement or delivery gate is complete.
