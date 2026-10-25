# Backups and storage-failure harness boundary repairs

Only these two repository files changed:
- crates/deadpan-app/src/preview/harness/backups/inspection.rs
- crates/deadpan-app/src/preview/harness/recovery.rs

Concurrent source was preserved. No production guard/service/API, root harness,
other scenario, Cargo source/lock, cache, binary, fixture or prior evidence changed.
No Cargo, replay or product invocation. Scoped rustfmt and rustfmt --check exit0.
Root owns compilation and the fresh affected native replays.

## Backups

The first replay passed32checks, including the other owner's Undo and explicit
read-only refresh, then failed Other owner did not reply during owner Redo.
ProjectService::dispatch_request stores busy=false before publish(); take_update
uses try_lock, so idle does not establish a delivered update. The old helper
waited only idle and immediately unwrapped an optionally unpublished reply.

The helper now drains an earlier pending update before submission, then waits
for the actual matching update within the existing Driver::wait_for deadline:
Open must carry its canonical package path; Undo/Redo must carry the matching
saved message and a revision different from captured expected_revision; Close
must carry no workspace. An actual returned service error still fails directly.
The update is latched because wait_for may evaluate readiness several times.
No reply/document is fabricated. Owner's wake callback now requests repaint on
the replay context, so its actual publication wakes the existing event-driven
wait. No polling sleep or timeout increase was added.

All existing inspection, history, picture, refusal, refresh and writable-session
assertions after owner creation are byte-identical to the before file.
Before SHA256 bb04a0f4a1d27019b1445fd283bf893efa90a9740f80306e24eb629730b530a1.
Final SHA256 c40c6f3ade564ffd5a6f453c613abb3e8cfc21b66bbabe53e311998ca1365e90.

## Storage failure

The first replay passed6checks but :split successfully committed with Action
completed and no DiskFull alert. Source proves the fault injection dispatch
checks ONLY ProjectRequest::Edit. Native :split now maps to SemanticInstruction
Split and runs through the semantic/macros route, so it never consumed the seam.
The previous wait could also accept the standing PermissionDenied from the
preceding intentionally denied Open.

The replay now constructs the real direct Split Edit request from the captured
workspace session/revision, selected node, Sequence scope, exact cursor and
existing selection::split_boundary. It submits through the App's existing
submit/ProjectService path after arming the existing failure seam, and checks
admission. This is explicit fault-boundary setup, not a claim that a native
:split command was fault-injected. Nothing directly edits authored state.

The wait requires the delivered DiskFull classification and storage alert,
idle service and unchanged captured session/revision. It cannot pass on the
standing denied-Open error. Every assertion from Alert painted onward is
byte-identical: complete existing error wording, kept revision, Not saved header,
navigation-persistent alert, successful real native :split clearing the alert,
and close-with-Camera behavior remain. The later successful save still follows
the production semantic/native command.

Before SHA256 9a5b8285b1c9662209bf49c085546324f67df9d05c8b862877634cbafe36874e.
Final SHA256 086d38183c0e6627f7936d03f8a82bcf3d012143238b88b939312f2281b963be.

Exact before/final full source .rs.txt, relevant service/router source snapshots,
first-hand failed report excerpts with original report/binary pins, complete
per-file patches and static-review.json are retained here. No runtime pass is
claimed. The original reports remain unchanged.

Model-packs was separately diagnosed as a valid actual disk guard:
actual UI16.36GBfree versus36.42GBrequired, not a license/UI bug.
model-space-diagnosis.json retains source/guard/cache inventory. No cleanup ran.
