# Encoded render host source review

Reviewed `crates/deadpan-cli/src/encoded_render/host.rs`, its unit tests,
`crates/deadpan-cli/tests/encoded_render.rs`, and the Python fault fixture.
Followed the relevant protocol classification, `SupervisedProcess` completion
and teardown paths, and `ArtifactWorkspace::snapshot_with_control` admission.
Source review only. No builds, tests, native commands, formatters, or repository
edits performed by this reviewer.

## Disposition

No remaining actionable production-host finding in this scope.

- The request binds the committed revision, complete document hash, picture
  contract, encoder choice, and explicit limits before launch. Completion is
  rebound and validated before file admission.
- The supervisor withholds completion until clean exit, owned-group cleanup,
  sole-owner reaping, and bounded pipe draining. Cancellation clears held
  completion. Host failures and deadline returns retain the supervisor's
  checked teardown through RAII before the workspace is removed.
- Cancellation and deadline checks cover capture/hash boundaries, polling,
  snapshot chunks, and candidate read/copy operations. The first worker
  diagnostic is retained when later exit or teardown events also fail.
- Artifact admission uses the pinned workspace descriptor and independently
  checks contained regular singleton files, declared length, byte budget, and
  SHA-256 before creating an owned snapshot. Candidate access uses that snapshot
  after worker paths disappear; no worker pathname is reopened.
- Candidate reads are bounded to 64 KiB and the retained extent. Copy starts
  from byte zero, handles short writes, checks interruption between writes,
  and reports sink failures without granting publication authority.
- Fake-worker cases distinguish protocol/claim rejection from artifact
  rejection with FIFO sentinels. Byte hash, length, symlink, and hardlink cases
  independently require artifact failures. The success fixture deliberately
  contains non-media bytes and claims only transport/admission behavior.

## Resolved test finding

Original location: `crates/deadpan-cli/tests/encoded_render/fixture.py:110-123`.
The video/audio regression and invalid-total/count branches originally exited
without a terminal response. The integration assertions accepted a generic
worker/protocol error, so deleting a progress guard could still pass because
the worker exited without completion.

Parent removed the early exits. Confirmed in the final source: each branch now
continues to the otherwise valid Completed message and clean exit while leaving
the FIFO artifact in place. If a progress guard is bypassed, admission reaches
the FIFO and produces the Artifact error rejected by the test. This resolves
the identified false-pass mechanism. Parent owns the subsequent test run.

The retry fixture exercises recovery after cancellation/deadline, but its
successful retry is not independent evidence of descendant cleanup. The
supervisor implementation and its process-specific tests own that proof.

Review complete and frozen.
