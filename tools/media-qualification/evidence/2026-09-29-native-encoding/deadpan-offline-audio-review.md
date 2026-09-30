# Offline audio review

Read-only source review of `crates/deadpan-cli/src/audio.rs`,
`audio/offline.rs`, and `tests/offline_audio.rs`, with the shared limiter and
source preparation callees inspected. No build, formatter, compiler, native
execution, or tests were run.

## Finding

- **P2: Cold-source index admission drops the offline deadline during its large Rust loops.**
  `crates/deadpan-cli/src/audio.rs:556` passes only `cancelled` into
  `PreparedSource::new`. That constructor compares the complete index and then
  serializes the complete index into its provenance digest
  (`crates/deadpan-audio/src/session.rs:47`, `:55`). Their existing cooperative
  checks inspect only the atomic cancellation token, including every index
  chunk and serialization write (`session.rs:143`, `:191`, `:201`). A source
  admitted near the end of the shared budget can therefore continue all of
  this work after expiration, for up to the allowed 1,000,000 index frames,
  before `audio.rs:557` finally notices. Late output is correctly rejected,
  but the worker cannot cooperatively stop this controllable Rust work at the
  requested deadline. Thread the fixed deadline or an existing control closure
  into index verification and provenance hashing, and test expiration during
  those loops. The current deadline test at `tests/offline_audio.rs:454` checks
  expiration before a read and does not cover this path.

## Otherwise checked

- Endpoints use `B(end) - B(start)`, preserving the 1601-sample nonzero-origin
  30000/1001 frame, rather than re-rounding its duration at zero.
- Limiter tiles and required support remain on the full absolute project grid;
  the selected range only bounds returned PCM.
- Cold source snapshots and native PCM opening receive recalculated remaining
  time from the same fixed budget. Cache hits recheck control and captured
  source contracts.
- The retained document, render plan, revision-bound receipt lookup, and private
  PCM cache preserve the captured revision across writer edits, undo/redo, and
  warm linked-path loss. No authored state is changed.
- Partition, historical-revision, range, cancellation, and deadline tests make
  concrete assertions. Their actual execution remains the parent's check.
