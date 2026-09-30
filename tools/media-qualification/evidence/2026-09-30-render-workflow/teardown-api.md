# Explicit render teardown API

Implemented source, not yet executed or formatted. Parent owns all execution.

## Coordinator contract

`EncodedRenderError::cleanup_confirmed() -> bool` is the stable predicate.
Only `true` allows a host-stage error to become durable Failed or Cancelled.
`false` requires retaining/requesting Cancelling and retaining the execution slot.
Do not derive this decision from the primary error category or diagnostic text.

`EncodedRenderError::CleanupUnconfirmed { primary, cleanup }` preserves both
the original error and structured cleanup issues. A post-spawn setup failure
can instead be nested under `EncodedRenderError::Supervisor` as
`SupervisorError::CleanupUnconfirmed`; the predicate inspects that wrapper.
`jobs::verify_checkpoint` propagates `failure.error` unchanged, so both shapes
survive its existing adapter without a signature change. The verifier continues
to retain the completed encoded candidate in `VerificationFailure`.

## Supervisor contract

`SupervisedProcess::finish_owned_work(&mut self, deadline: Instant)` returns
`Result<StoppedProcess, CleanupFailure>`. The stopped receipt has private
construction and exposes group scope, exit status and a joined-pump panic flag.
The cleanup error exposes issues classified as Group, LeaderFallback, Reap or
Pipes. Successful leader fallback never clears an earlier group failure.

The operation sets cancellable I/O, releases bounded channel backpressure,
stops the owned group before reaping, marks each reap attempted before waiting,
then joins stopped pumps. It uses actual monotonic time. A prior ambiguous wait
prevents all later signal/wait operations, including Drop. Repeated cleanup
retains earlier uncertainty and cannot manufacture a clean receipt.

`StoppedProcess::require_membership()` rejects the explicit Linux
SignalAndLeaderOnly scope. Darwin returns MembershipConfirmed. Render host
finalization and partial-spawn error cleanup require membership confirmation;
Linux signal-only cleanup is never promoted to the durable macOS guarantee.

Every post-spawn return in encode_guarded and verifier inspect passes through
one finalization helper. Its two-second cleanup deadline is separate from an
expired render deadline. A joined pump panic prevents successful media output
but still permits a truthful terminal failure after confirmed cleanup.
Drop remains fallback cleanup and grants no returned evidence.

## Tests added or strengthened

- Every partial setup stage, including pipe setup, three pump starts and initial
  request enqueue, with a real child and actual reaping checks.
- Setup failure plus injected group-query failure preserves both diagnoses.
- Failed group query followed by successful leader fallback remains unresolved.
- Wait error after actual reaping prohibits every subsequent PID operation.
- Explicit teardown joins all pumps before creating a receipt.
- Pump panic is distinct from normal completion and cannot yield host success.
- Confirmed cleanup preserves the original host Deadline error.
- Existing cooperative cancel, deadline, orphan/forking-descendant and inherited
  pipe integration tests now invoke and inspect explicit finalization.

Owned source files: deadpan-jobs/src/process.rs, deadpan-jobs/tests/supervisor.rs,
deadpan-cli/src/encoded_render/{host.rs,host/tests.rs,verification/host.rs}, and
the error enum/impl in encoded_render/mod.rs. No jobs.rs change was needed.

No builds, tests, formatter, native execution, commit or push ran in this agent.
