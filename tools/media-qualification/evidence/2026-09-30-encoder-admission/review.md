# Automatic encoder admission review

Read-only review, 2026-09-30. No builds, tests, formatters or native execution.
Repository files were not changed. Other agents are still editing this tree.

## Scope inspected

- native/deadpan-encode/src/probe.rs and tests/probe.rs
- crates/deadpan-cli/src/encoded_render/admission/mod.rs, protocol.rs, host.rs
  and their current tests
- Relevant existing supervisor/finalization behavior and native dynamic linking
- After source freeze, inspected admission/worker.rs and content.rs, including
  descriptor reopening, controls, content oracle and unit tests, plus new
  tests/encoder_admission.rs and encoder_admission/fixture.py.

## Finding resolved during review

### P1: preserve unresolved teardown when cancellation/deadline is also set

Location: admission/host.rs:99-105, qualify's Err(error) branch.

The probe already calls finish_owned_result, which can return
EncodedRenderError::CleanupUnconfirmed after cancellation or deadline. The outer
loop immediately executes `check_control(cancelled, deadline)?`. On exactly these
paths, this replaces the structured cleanup error with plain Cancelled/Deadline.
Both plain errors report cleanup_confirmed() == true. The caller therefore loses
the fact that it must keep owned work/session resources fenced. The same early
return also discards later-fault detail from WorkerFault.

Fix: preserve the returned error and its cleanup uncertainty before applying
control precedence. For any still-eligible typed capability failure, cancellation
must invalidate fallback while retaining the original error. A returned
CleanupUnconfirmed must remain unresolved at the top level or inside a wrapper
whose cleanup_confirmed delegates to it.

Test: use a deterministic attempt/selector seam to inject CleanupUnconfirmed
with the cancellation token already set and separately with an expired deadline.
Assert the returned error still reports !cleanup_confirmed(), retains its cleanup
issues and primary error, and launches no further probe. Also cover WorkerFault
with a concurrent control interruption.

Sent to parent immediately on discovery.

Parent removed the outer Err-branch check_control. Re-read the corrected source:
the original structured error is retained, and only a permitted next iteration
checks control before spawning. Unresolved cleanup and wrapped faults have no
next choice. This resolves the source defect. Parent owns execution evidence.

## Current inspection conclusions

- Native generator arithmetic is bounded and origin-based. The admitted probe
  duration supplies 3*GOP+1 frames; picture filling uses exact I420 cell ownership
  without allocations; marker coordinates are independent per channel. No
  concrete generator defect found by inspection.
- next_choice only matches top-level typed WorkerFailure native capability kinds.
  Generic prose, other stages and wrapped faults do not trigger fallback.
- Supervisor enforces terminal ordering and permits exit 1 only after an admitted
  Failed terminal. Bad tails/other exit codes become Faults, which the probe host
  invalidates before considering selection. Finalization remains mandatory.
- Success binds report spec, manifest contract/hash, verifier report and bounded
  private snapshot bytes. QualifiedEncoder's fields are private and it has no
  deserializer. No persisted report or public settings are added in this wave.
- Frozen worker consumes/drops the encoder before opening any decoder. Its
  read-only descriptor must match the owned output's inode, device, uid, extent,
  mtime, ctime and single link; it compares metadata and rehashes after complete
  inspection. Host then hashes a contained private snapshot after process exit.
- Shared verification covers container/packet clocks, every picture, fresh GOP
  decoding and manual/ordinary AAC agreement. The content oracle additionally
  compares each complete image plane against the recipe, including per-frame
  limits, and checks distinct signed channel markers at exact sample coordinates.
  Oracle errors are Output failures, never encoder-capability failures.
- No additional blocking correctness defect found in frozen worker/content.

## Additional gaps to address or document

- Resolved on final re-review: qualify now calls check_control immediately before
  constructing QualifiedEncoder, after runtime_identity and equality admission.
- AdmissionRuntime hashes only the helper executable and uname data. Native media
  libraries are dynamically linked (native/deadpan-encode/build.rs:75-79), and
  probe reports retain numerical versions rather than loaded-library byte/build
  identities. This is an incomplete runtime fingerprint for later cached/durable
  consumption. The present wave has no project-encode consumer, so do not describe
  it as authority that survives runtime replacement. Freeze loaded-runtime
  identity requirements before the DB42 integration described in the storage plan.
- Parent added process-result integration fixtures during review. Inspected
  coverage includes fresh probe identities and allowed choice steps, capacity,
  misleading prose, stale identity, unknown fields, invalid terminal tail,
  duplicate terminal, SIGKILL, exit 2, post-terminal hang and live cancellation.
  These cover the key late-fault boundary beyond pure next_choice tests.

## Verification limits

These are source-level conclusions. Parent owns all execution and will supply
native qualification and test evidence. Full assigned source review is complete.

## Final bounded re-review

Confirmed MAX_PROBE_PACKETS = 1024 is the default and is enforced by both the
host and wire request validation. No native run or tests were executed here.

Found one misplaced final edit: the added request_cancel(now) call is inside
ProcessEvent::Exited's error branch (host.rs around line 450), while the Progress
regression branch around line 408 still only records its protocol failure. Thus
a live worker that regresses progress and then stalls is not cancelled until the
shared deadline. Parent was notified to move/add the call directly after the
regression failure. The new fixture emits [2,1] and immediately exits Failed,
which covers refusal to fall back but does not prove prompt cancellation. Make
that fixture wait for a valid Cancel after regressing progress.

Resolved in the final frozen correction: re-read host.rs and confirmed
request_cancel(now) now sits directly inside the regressed Progress branch,
with no cancellation call in Exited. The strengthened fixture waits for Cancel,
asserts exact identity/token, records `requests.cancelled`, and exits only after
that receipt. The integration test requires the receipt as well as one probe
and confirmed teardown. This would distinguish prompt protocol-fault stopping
from waiting for the shared deadline. All actionable findings from this review
are source-resolved; test execution remains the parent's responsibility.

Read the retained `ntsc.json` result. It records typed hardware TargetTwo
VideoTimestampOrder rejection, followed by hardware None admission at 320x180,
30000/1001. Selected evidence has 46 pictures, 73,674 authored sample frames,
three GOPs, all 46 frames freshly GOP-decoded, exact coordinates for all six
audio markers, maximum Y/Cb/Cr errors [4,7,5], and an 18,742-byte retained movie
whose report/retention hashes agree. This is evidence supplied by the parent,
not independently rerun here. Its maximum_moov_bytes is 257,048,576, so this first
native result predates the tightened 1024-packet allocation and should not be
represented as execution of that last bounds change.
