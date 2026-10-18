# Original mark macros, 2026-10-18

Semantic macros now resolve supported motion and mark instructions on the
active invocation clock. Original uses its qualified measured picture index;
Your edit retains its existing ordinary Sequence semantics. This advances
DP-06 and the shared DP-21 command path. Other macro instructions, selectors
and parser work remain open.

Base revision `637553a2791c9a2c1f63004e2a325b3ff4955d7f`, Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Clocks and admission

`SemanticContext.original` captures an asset, qualification and presentation
ordinal independently of the retained Edit context. The host loads the
committed receipt's immutable index and checks its complete asset metadata.
It does not decode, hash or copy source media, accept an index from the request,
or read a preview proxy. The pure planner checks the asset, qualification,
time base, start/end, picture count and inclusive terminal boundary.

Original dispatch permits frame and start/end motion, letter mark saves and
jumps, and named calls. Marks use the exact measured video PTS or terminal
video end and belong to the document root. Jumps require a bound mark on the
same Original and an exact indexed boundary. The saved program retains intent,
not the recording's position. Recursive calls, fuel and depth limits, staged
proofs and one atomic Compound remain unchanged. A late failure saves no marks,
history or register changes; navigation-only programs create no history.

Native recording names Original in its hint and preserves the independent Edit
cursor, selected beat, ordinary scope and oriented Visual range. Original
Visual selections and other instructions refuse explicitly. Escape cancels
the draft without clearing the inactive Edit range. Capture guards include the
source, qualification, Original cursor and selection; away-and-back navigation
permanently revokes a pending receipt's cursor authority.

## Native metadata publication

Original macro marks and direct headless marks publish a separate mark-only
revision proof. It preserves retained selection, jump metadata, the accepted
picture and nested inspector navigation, without granting remote cursor
authority. The service keeps one contiguous mark-run base; the UI retains one
last-seen proof. That bounded state admits coalesced saves and a previously
visible intermediate revision. An intervening structural edit, another project
or session cannot inherit the proof.

Independent review found that an immediate-edge proof lost a coalesced save,
and that recorded Original jumps bypassed native Back/Forward bookkeeping.
The fixes retain the bounded run proof and use the ordinary native jump path.
Follow-up review found that remote marks still closed a nested inspector;
scoped rebasing now receives the admitted actual visible-to-current transition
and changes only its document/plan references. Final review found no remaining
issue in those fixes, core admission or the host path.

Release replay then exposed a stale generic Edit continuation: after an Original
mark run, saving a later Macro replaced its dedicated receipt and let the old
generic commit switch the viewer back to Your edit. Original authored success
now remains exclusively in its Macro receipt and metadata proof; the service
clears the generic Edit continuation. The regression fails on the preceding
implementation and passes after the fix, alongside all 21 native macro tests.

Visual review also caught Edit-only recording hints and an incorrect Escape
label on Original. The footer now teaches only the supported clock's actions.
The generic Escape handler preserves the inactive Edit range when cancelling
Original recording. A further review found that this cancellation also needs
to revoke pending cursor ownership explicitly when there is no Original
selection to clear. The replay withholds an exact real call receipt, presses
Escape, then verifies that its later completion cannot move either cursor or
save the cancelled draft.

## Tests

Five focused core tests cover VFR marks, terminal/clamped motion, current
qualification/index admission, wrong-clock/ambiguous/unresolved/named-mark
refusals, nested counted calls, proofs, limits and atomic late failure.
Real `vfr.mp4` CLI tests cover dry-run/commit equality, exact PTS, reopen,
Undo/Redo, navigation-only execution and unchanged banks. They also reject
stale qualification, mismatched metadata, a wire-supplied index and an
unsupported late instruction without writes. The native service test checks
both retained clocks, mark metadata, durable history and navigation-only runs.

The integrated scoped gate passes 181 tests, followed by 67 registry, palette,
marks and scoped-inspection tests after the final native fixes. Both strict
Clippy configurations pass. With `ui-harness` enabled, all 87 selected native,
registry, marks and inspector tests pass. These are scoped selections, not a
full workspace gate.

After the final publication and Escape corrections, the scoped gate and
UI-harness runtime selection each pass 27 tests. Two existing pure receipt
tests reported leaked output handles in concurrent runs; both pass sequentially
in 0.020 s without leak warnings. The final UI-harness run also has no leak
warning. The initial warnings and isolated results are retained.

Initial compile attempts caught missing context literals, a temporary error
borrow, a larger enum variant and test-module ordering. The enum capture is
boxed to satisfy the size lint. The command-reference generator exits
with its deliberate rewrite assertion; the subsequent normal registry check
passes. Failed reports are retained separately from successful checks.

The first release replay failed because the fixture's navigation helper pressed
Escape and cleared its inactive Edit range before recording began. The fixture
now navigates without that cancellation. The second release replay exposed the
real stale-publication bug above. The development replay then passes 195 checks,
before the final footer and Escape additions. None of those preparatory results
substitutes for the final release runs.

## Final release replay and visual review

Release visual and performance `marks` replays each pass 203 checks. The broader
release `macros` replay passes 401. Every run also passes the production shortcut
audit against the local Kestrel source: 62 reserved bindings, 21,885,504 routing
cases, no conflicts or source drift.

The marks replay uses real `q`, mark and jump keys, paired native `@` text,
actual service receipts and two real live headless mark saves. It verifies exact
mark PTS at a new invocation, the ordinary Ctrl O/Ctrl I trail, inactive backward
Edit selection, pane and selected-beat retention, away-and-back revocation,
Escape cancellation with a delayed call, and coalesced remote publications.
Full-size 960×640 captures were inspected. The Original recording hint, supported
motion/mark/call keycaps and both cancellation labels paint fully; unsupported
Edit range/group hints are absent from that recording footer.

The separate performance run has 417 completed input-to-state samples: median
0.434 ms, p95 1.052 ms and maximum 2.031 ms. Fifty input-to-picture-complete
samples have median 1.674 ms, p95 10.263 ms and maximum 92.129 ms, including
initial project work. Neither metric has failed or timed-out samples. No build
or other replay from this task ran concurrently with this retained timing run.
An earlier performance run overlapped the macro replay and is retained
separately; its measurements are not used here. AC power was observed at 80%
battery; thermal state and OS caches were uncontrolled. These are offscreen
short-fixture measurements, not physical input-to-display or long-clip evidence.

Both marks runs retain 69 frames settling on a second layout retry and six runs
of consecutive retrying frames with distinct input causes. The macro replay
retains its screenshot allowance warning, 115 second-retry frames and 32 such
input-driven runs. No ignored retry or repeated idle retry fails a check.
The final release rebuild passes. One earlier rebuild was deliberately
interrupted before completion to correct the inspected footer; it is not
reported as a successful build.

Executed final app SHA-256:
`d917dddfb391a1af51a48fa5c608e1b8b61d5fdf49d153f7195c33a39e1a0995`.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-18-original-mark-macros/)
contains final and failed reports, source/binary/fixture identities, the source
diff, inspected captures, reviews and a verified inventory of 64 files.

## Limits

Original supports only the stated instruction subset and requires the retained
Edit context to be an ordinary Sequence. Remote mark publication preserves an
already open occurrence inspector; that does not authorize recording from it.
The fixtures do not establish long-clip index-loading performance, physical
input or VoiceOver speech. Exact owner steps remain in
[Requirements](../REQUIREMENTS.md#to-verify-owner). No picture/audio rendering
algorithm or AI provider changes in this slice.
