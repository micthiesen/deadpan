# Native integration and teardown review

Native integration source is frozen for the parent verification pass. No builds,
tests, formatters, or native programs were run by this agent.

## Confirmed finding

P1: `SupervisedProcess::drop` in `crates/deadpan-jobs/src/process.rs` unconditionally
joins the remaining pipe pumps after its bounded `finish_owned_work` call.
`finish_with` deliberately retains unfinished pumps and returns `CleanupStage::Pipes`
when its deadline expires. The destructor's subsequent join can therefore wait
without a deadline, preventing an encode or verification `CleanupUnconfirmed`
error from reaching the coordinator. The native service then cannot report its
truthful unresolved state. Parent has been notified and owns the fix.

Suggested validation: hold one owned pump beyond the explicit cleanup deadline,
assert the structured pipe-cleanup failure and bounded host return, then release
the held pump. Retain ownership without an unbounded destructor join.

## Reviewed without another confirmed finding

- Encode and verification wrap every ordinary post-spawn return in
  `finish_owned_result`, preserve a primary error, require membership evidence,
  and reject pump-panic success.
- Partial process setup installs the child owner before fallible pipe and thread
  setup. Setup failures explicitly finish owned work and distinguish unconfirmed
  cleanup from errors that started no child or completed teardown.
- Group failures survive checked leader fallback; failed reaping marks ownership
  consumed before waiting and is not retried against a possibly recycled PID.
- The render workflow lease has explicit release. Dropping an unreleased lease
  leaves the same writer session fenced. The owner guard compares both live
  session identities and checks revocation without reading media.
- `EncodedRenderError::Render` currently enters the encoded host through pure
  document hashing, so its default cleanup classification does not hide a live
  raw-render child in the reviewed paths.
- Native close, switch, and shutdown use `can_release_writer`, which includes the
  explicit worker Release acknowledgement, rather than only cleanup status.

This was a source review of the named boundaries, not runtime qualification.
