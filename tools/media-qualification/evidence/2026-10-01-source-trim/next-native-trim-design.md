# Native Trim integration notes, planning only

Read-only inspection after the Ripple backend checkpoint. No native implementation
or qualification is claimed by these notes.

## Existing concrete pieces

- `project/slip.rs` and `project/service/slip.rs` already own captured target
  absence, session/revision/scope, exact request retention, replaceable proposal
  IDs, immutable before/proposed snapshots and saved receipts independent of
  workspace refresh. Reuse these invariants for Trim instead of UI mutation.
- `preview/slip.rs` separates real cursors from temporary inspection, immediately
  revokes pending presentation after every draft change, and gates Apply on the
  current proposed decoded/GPU-submitted frame at the current raster.
- `worker/endpoints.rs` already has a replaceable, cancelable two-picture worker
  for Original and copied selections. `preview/splice/pictures.rs` composes both
  through the shared SDR renderer, retains old targets across failed resize and
  submits only one picture per pass. It needs a committed/proposed snapshot input
  and full content identity to support actual edit-boundary pictures.
- `preview/gain/waveform.rs` uses the playback preparation worker for measured
  bounded stereo waveforms, carries ticket/session/revision/owner checks, and
  labels partial/unknown coverage. Its owner-definition coordinates cannot be
  relabeled as a full mixed edit waveform. A Trim graph must explicitly choose
  and label source/owner or absolute edit clock and handle crop/prefix transforms.
- `preview/splice/comparison.rs` already maps before/proposed edit sites for
  context audition using absolute B(frame) sample boundaries. A dedicated Trim
  comparison can reuse its checked concepts, not assume equal output durations.

## Proposed path after backend Roll/overwrite semantics settle

1. Capture a complete Trim target at `,v` or `:trim`, preserving explicit absence.
   The router owns plain h/l, Shift ten-frame motion and r policy toggle only
   outside text/IME fields. Trim Tab cycles In/Out/Slip/Roll as required; native
   text fields must keep their ordinary editing/focus behavior. Run the full
   Kestrel production-router matrix after adding this context.
2. Resolve exact handles on the project service and use bounded replaceable
   proposals. Keep one retained authoritative request for Apply. A draft has no
   authoring/history until Enter; zero stays preview-only.
3. Show the outgoing and incoming frames at the selected proposed join, with
   complete saved/proposed identity, absolute Edit boundaries and underlying
   Original labels. At document exterior, show an explicit start/end boundary
   state rather than inventing an authored black frame or decoding a neighbor.
   Gate Apply on every available proposed boundary picture being current and
   GPU-submitted; late/stale/partial pairs cannot authorize it.
4. Carry each pair's accepted identity/caption/geometry together. Revoke pending
   work immediately on mode/amount/policy changes, Cancel, save, stale revision
   or session replacement. Preserve the prior accepted pair across decode and
   resize failure. Sequential GPU submissions must not display a mixed pair.
5. Add measured audio with declared coordinates and unknown-coverage handling.
   Add Before/Proposed context audition through immutable playback snapshots;
   stop and invalidate output on draft change. Keep independent root sounds in
   the full edit mix and use separate absolute before/proposed sample windows.
6. Native verification must inspect keyboard-only mode cycling, text/IME focus,
   comparison labels and resize, cancellation with byte-identical SQLite,
   exactly one commit, durable Undo/Redo/reopen, source-bound clamps and faults.
   Open a task-owned app only for these checks, then quit and verify process and
   writer-lock release.

## Design issue to settle before code

Tab is mode selection, but the spec does not explicitly say whether switching
between In/Out/Slip/Roll discards or accumulates pending adjustments. Silently
throwing away a prior edge adjustment would be surprising. Prefer a draft that
retains explicit pending operations and commits them atomically, provided the
combined command has one coherent exact timing/sound/mark transform. Do not
implement it as several store commits or let a generic macro path bypass source
receipt admission. Investigate the existing atomic-command/composite seams and
choose a concrete draft representation before generalizing Slip's single delta.
A first preview of one `:trim` command can remain a clearly scoped intermediate
capability while the complete mode is still open.
