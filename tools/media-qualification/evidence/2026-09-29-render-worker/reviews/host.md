# Render host lifecycle review

Reviewed `crates/deadpan-cli/src/render_worker/host.rs`, `mod.rs`, their tests, private dispatch in `lib.rs`, and Cargo changes. Read protocol, child, picture capture and artifact snapshot boundaries to verify their contracts. No edits, Cargo, formatter or tests were run. The extracted transport was not re-reviewed.

## Actionable findings

### P2: Preserve the worker's specific failure before its expected nonzero exit

Location: `crates/deadpan-cli/src/render_worker/host.rs:171-174`.

`worker::run_entry` sends a `RenderWorkerMessage::Failed` containing its diagnostic, then intentionally returns a failed process exit. The host first stores the useful diagnostic at line 171. The supervisor subsequently emits `Fault("worker exited unsuccessfully: ...")` for the nonzero exit, and line 174 unconditionally overwrites that stored error. This loses the actual missing-media, changed-document, GPU or write failure reason for ordinary child failures, leaving only an exit status. The same replacement can hide a previously detected backward-progress protocol error after cancellation.

Preserve the first substantive failure while still completing teardown. A deterministic fixture that sends a known `Failed` diagnostic and exits nonzero should assert that `prepare` returns that diagnostic after the child exits. A bounded stderr tail could additionally help with crashes that never send a terminal message, but that is separate from the required fix.

### P2: Normalize interruption errors across preflight and artifact admission

Locations: `crates/deadpan-cli/src/render_worker/host.rs:111`, `host.rs:199-206`; conversion variants in `render_worker/mod.rs:39-46`.

Cancellation before launch, during the main poll loop, or during plane validation returns top-level `RenderWorkerError::Cancelled`. Cancellation while `ProjectPictureSession::open_revision` runs instead uses the automatic conversion to `RenderWorkerError::Picture(ProjectPictureError::Cancelled)`. Cancellation/deadline during `snapshot_with_control` similarly returns `RenderWorkerError::Artifact(ArtifactError::Interrupted(...))` through `?`. The operation's outcome therefore depends on the exact cancellation phase, so callers matching the public Cancelled/Deadline variants can classify an interrupted job as a failure during those phases.

Map these explicit interruption sources to the top-level Cancelled/Deadline variants, preserving other errors. The existing mappings in `deadpan-models/src/qualification.rs` and `conditioning.rs` show the artifact convention. Add deterministic mapping tests for both interruption reasons and a preflight picture cancellation; the current tests cover only controls already set before preflight starts.

## Other reviewed boundaries

The host selects runtime, environment and canonical package path. The child validates the complete serialized document hash and captured revision contract before GPU/output allocation. Completion stays behind clean process teardown, then the host verifies pinned contained bytes, expected length/hash and every limited-range Y/Cb/Cr code before returning a private owned snapshot. Output frame lookup remains contract-bound and independent of removed workspace/package paths. These checks support this raw picture preparation boundary; they do not establish full export, encoded media correctness, a preemptive host filesystem deadline, or OS sandboxing.
