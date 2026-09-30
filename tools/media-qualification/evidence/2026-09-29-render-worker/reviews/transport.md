# Render transport extraction review

Result: no actionable findings in the reviewed changes.

Scope: `crates/deadpan-jobs/src/process.rs`, the generation adapter in
`supervisor.rs`, public framing helpers in `protocol.rs`, module exports, and
generation/independent-protocol supervisor tests, compared with HEAD
`9448ac9`.

Evidence inspected:

- Direct comparison of the previous complete `supervisor.rs` against the new
  `process.rs` shows that process spawn, pipe bounds, cancellation timing,
  exit observation, group teardown, sole reaping ownership, and Drop ordering
  are preserved. The substantive substitutions are protocol-associated request
  and response types, codecs, and response classification.
- `GenerationProtocol` retains the initial GenerateHold/GenerateBridge-only
  restriction, the captured protocol version and complete attempt identity,
  and the exact distinction between progress, immediate failure/cancellation
  messages, and successful completion held until clean exit.
- The public `read_frame`/`write_frame` changes expose existing bounded codecs;
  generation still calls its semantic validators. Both production adapters
  continue to validate their own initial requests and replies.
- A completed response still cannot escape after cancellation, a malformed or
  post-terminal response, unsuccessful exit, group cleanup failure, or a pipe
  shutdown failure. The final event queue check still occurs after all senders
  finish, with completion and Exited delivered together.
- Group cleanup still precedes reaping. Real monotonic cleanup deadlines and
  the checked leader fallback remain intact; `reap_attempted` is set before
  waiting, preventing a later retry of a stale PID.
- Existing generation chaos tests remain intact, including delayed polling,
  descendants retaining pipes, escaped descendants, cancellation, failed
  completion and bounded log/backpressure cases. Added independent-protocol
  cases cover fragmented progress, clean completion ordering, protocol-owned
  cancellation identity, wrong identity, post-terminal messages, failed exit,
  and initial-operation rejection before launch.

Verification limits: read-only source review. No Cargo, formatting, tests,
native execution, repository writes, commits, or pushes were performed.
This review does not establish actual runtime qualification and does not
review the CLI host, picture child, or output pixel semantics.
