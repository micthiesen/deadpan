# Typed encode failure boundary review

Independent source review of the current native/CLI diff. No repository edits,
builds, tests, formatters or native programs were run by this reviewer.

## Findings sent to parent

### Resolved in source: P1 ordinary typed failure exit invalidated by the fault wrapper

Paths: `crates/deadpan-cli/src/encoded_render/host.rs::encode_guarded` and
`crates/deadpan-jobs/src/process.rs::SupervisedProcess::poll`.

The supervisor emits `ProcessEvent::Fault` for every unsuccessful child exit,
including code 1 after a valid failed terminal. Production `worker::run_entry`
returns false for `Failed`, and its launcher and positive fixtures exit 1.
Wrapping every Fault after `WorkerFailure` therefore converts clean typed
failures to `WorkerFault`. The new positive transport cases cannot retain their
expected top-level capability category.

Parent's focused test reproduced this clean-exit-1 regression. The reviewed fix
adds `ResponseKind::Failed`; only a fully classified/identity-admitted Failed
terminal records that exit 1 is expected. Signals, exit 2, malformed/duplicate
tails and timeouts retain faults. Plain Terminal still faults on exit 1; the
generic protocol test now asserts that behavior. No fault prose is parsed.

### Resolved in source: P1 direct supervisor errors and deadline preserving a typed claim

Path: `crates/deadpan-cli/src/encoded_render/host.rs::encode_guarded`.

The direct `process.poll(now)` error arm and deadline arm initially returned
`failure.unwrap_or(...)`. A typed failure delivered in an earlier poll therefore
survived a later supervisor error or expiry. If explicit teardown subsequently
passed, the host still returned a top-level `WorkerFailure`.

One concrete path is a typed terminal followed by malformed trailing output:
`poll` starts reporting a fault but checked group teardown fails transiently, so
the poll returns Err rather than its event vector; a later explicit finish can
successfully stop the group. Another is a typed terminal followed by a worker
that remains alive until the shared deadline, before the host polls the terminal
exit-grace fault. Both must invalidate top-level capability authority while
preserving the original diagnostic. The reviewed correction sends direct poll,
request-cancel and deadline errors through `invalidate_report`, and rechecks
control before returning a typed terminal failure after the loop. A later pump
panic also invalidates the report. Cleanup failure still wraps the whole result
as `CleanupUnconfirmed` and takes precedence in durable diagnostic mapping.

## Reviewed behavior without another confirmed finding

- Native code only emits the video capability categories for the exact named
  video encoder absence condition or an observed video packet with PTS < DTS.
  Missing AAC, generic driver/open failures, unsupported options, malformed
  packet evidence, capacity and I/O remain distinct noncapability outcomes.
- C timestamp comparison occurs before admitting the packet or muxing it, with
  actual PTS/DTS in the diagnostic. The failure poisons the session; it does not
  rewrite timestamps or select a different encoder.
- The Rust native classification is a closed exact-code mapping. Unknown native
  errors remain Native. Diagnostic prose does not affect kind.
- Protocol 2 changes the failure envelope and rejects legacy messages, stale
  identities, missing/extra fields, future stages/kinds and oversized diagnostics.
  The unchanged manifest schema is separate from the wire-version change.
- Worker preparation maps only actual EncoderSession errors through
  `encoder_failure`. Source, document, picture, audio, output hashing and control
  errors have separate stages. Finishing the control pump precedes terminal
  selection, so a queued cancellation or control failure cannot be hidden by a
  completed result or encoder claim.
- Explicit host teardown remains mandatory. Cleanup uncertainty wraps the
  primary error, and workflow classification checks cleanup before mapping typed
  failure codes. The new WorkerFault preserves the diagnostic while removing a
  top-level typed capability when it is applied.
- Transport fixtures leave a FIFO at the candidate path, so accidental artifact
  admission produces a distinguishable failure. Positive cases assert exact
  categories and confirmed cleanup; legacy/future/stale cases assert claim
  rejection. These fixtures prove transport/admission, not native capability.
- The Python native qualification parser requires a failed structured envelope
  and exact kind when judging the measured hardware-B rejection. Misleading
  diagnostic text cannot select the capability case. Unknown kinds cannot pass
  the expected exact-category checks. Successful outputs still take the full
  independent emitted-file path.
- The new `qualify_encoder_failure` example requests two explicit independent
  engineering attempts. It requires a top-level VideoTimestampOrder with
  confirmed cleanup for the first; the second must encode, pass fresh independent
  verification and retain a matching hash/length. It binds worker bytes, revision,
  document and authored history, and does not claim automatic fallback or product
  publication.

## Review status

Final source re-review completed after both host corrections and the qualifier
freeze. No open confirmed finding remains. New hostile fixtures cover malformed
and duplicate trailing terminals, signal death, exit 2 and failure-then-hang;
exact positive typed exit-1 cases remain. Helper tests cover direct supervisor and
control invalidation, and pump-panic coverage uses the real finalization helper.
The parent is compiling/running the corrected focused checks; this review does
not claim those results. All execution and qualification belong to the parent.

No automatic selection/fallback is introduced, and immutable render-job schema
is unchanged. The additional supervisor response classification is scoped to
protocol adapters that explicitly select it.

## Final qualifier correction review

Re-read the corrected probe admission and digest formatting after the initial
example build failed on the private `ExportPictureContract::capture` method and
unsupported SHA-256 LowerHex formatting. No new finding.

The helper now reads the public committed presentation basis and requires
`max(1, ceil(fps / 2)) * 2` frames. This is conservative relative to the current
native policy's rounded half-second GOP target: it cannot admit a shorter probe
than twice that target. Typed positive frame-rate components fit safely in its
u64 arithmetic, and the actual range remains at most 90 frames. This is a probe
length guard, not a measurement of actual emitted GOP lengths. Production encode
still captures and binds the full exact contract. The helper report now records
the basis and minimum probe length rather than an independently synthesized
output contract.

Both file and authoring digests now format each byte as two lowercase hex digits,
preserving the expected 64-character SHA-256 representation and retained-media
hash comparison. Parent reports the corrected native run passed the actual typed
B-frame rejection and a separate 90-frame freshly verified output with unchanged
authoring. That is parent-run evidence; this reviewer ran no verification.
