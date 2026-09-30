# Isolated render child review

Reviewed `crates/deadpan-cli/src/render_worker/worker.rs` and
`worker/control.rs`, with read-only inspection of the host, artifact snapshot,
and shared deadline boundaries. No source edits, Cargo, formatter, tests, or
native execution were performed.

## Actionable finding

### P2: Retry interrupted reads before treating a control pipe as drained

`worker/control.rs:72-79` checks the stop flag after `ErrorKind::Interrupted`
and sets `interrupted_for_shutdown`. If no bytes have yet been consumed,
`ControlPump::start` converts this to `ControlEnd::Stopped`, allowing a prepared
manifest to become Completed. An interrupted read establishes neither EOF nor
WouldBlock. If control bytes are already queued, this branch can skip a wrong
token, malformed message, or cancellation during the completion handshake.

Retry Interrupted unconditionally through the existing deadline loop. Honor
the stop flag only after the nonblocking read returns WouldBlock. This keeps
the same bounded shutdown behavior and establishes the documented drain
condition before success.

## Previously suspected issue, already corrected in inspected source

The inspected `ControlReader::read` attempts the descriptor read before checking
the stop flag in its WouldBlock branch. Thus `finish()` setting the flag first
no longer bypasses ordinary queued input. Its byte counter rejects a partial
header or body when the drained stream would block, instead of accepting
Stopped. Host EOF also fails closed.

The existing wrong-token test waits for the cancellation atomic before calling
finish. Add a deterministic test that writes a complete wrong-token or malformed
frame before starting the pump, then immediately calls finish; this specifically
covers the corrected completion race. Include a partial-body case alongside the
existing partial-header test.

## Other inspected boundaries

- Contract equality and the hash of the full committed document are checked
  before GPU allocation and output creation.
- Each frame is checked against the captured ordinal, output timing, raster,
  policy, and byte count. The completed-frame permit is dropped before the next
  request. Output chunks and progress messages are bounded.
- Output uses an exclusive descriptor-relative regular file below the owned
  workspace, with no symlink following. The manifest hash covers the written
  Y/Cb/Cr bytes; the host independently freezes and hashes the artifact after
  clean teardown.
- Child deadlines and cancellation are cooperative around native GPU/filesystem
  calls. Blocking request-device, writes, and sync remain protected by the
  parent process deadline and owned process-group teardown. They do not prove
  preemptive cancellation inside a native call.
- `finish()` joins the control reader before any terminal reply. Valid
  cancellation and invalid control each prevent a completed manifest.

No other actionable defect was found in this scope. This is source review,
not evidence of native GPU output or process-teardown test execution.
