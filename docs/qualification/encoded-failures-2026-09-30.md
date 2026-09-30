# Typed encoder failure qualification

This qualifies failure classification through the real encoder and supervised
worker. It advances automatic Render admission without choosing a product
encoder policy or enabling public Render controls.

## Boundary

The native adapter reports exact error kinds separately from diagnostic text.
Missing video codecs are distinct from missing AAC, resource exhaustion and
generic codec-open failures. Actual video PTS before DTS is rejected before
packet admission and muxing; neither timestamp is rewritten.

Private encode protocol 2 carries the failed boundary and native kind. Older
worker messages, unknown kinds and fields, and stale identities are rejected.
The host retains the diagnostic but invalidates a typed report if supervision
later fails. A validated failure terminal permits exit code 1; malformed tails,
duplicate messages, other failing exit codes, signals and pump faults remain
failures. Explicit cleanup still gates any terminal operation.

Database 41, core 33, retained manifests and engineering policies are unchanged.
Verification retries still inspect the original checkpoint and encoder choice.

## Real native evidence

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and pinned
LGPL FFmpeg 8.0.3. The retained report records compiler, SDK, helper and library
identities. This run uses normal builds; sanitizers were not rerun.

- A real committed project range `[0,90)` reproduces the hardware B-frame
  failure as `Encoder(VideoTimestampOrder)` with confirmed process cleanup.
  The retained diagnostic records PTS 1001 and DTS 2002. No candidate returns.
- A separate explicit hardware/no-B attempt encodes all 90 frames and passes
  the independent production verifier. Its retained movie hash matches the
  verified bytes. Project content, revision history, cursor and redo rows are
  unchanged; a SQLite backup preserves the same authoring digest.
- The synthetic native matrix passes ten complete encodes, 872 pictures,
  forty fresh GOP boundaries and 2,432 decoded suffix pictures. Hardware
  without B-frames and OS software with/without requested B-frames pass at
  30000/1001 and 60 fps, including edge and one-frame fixtures.
- FFmpeg ordinary/manual and AVFoundation readers place all 180 channel/event
  coordinates at their authored samples. Observed error is zero samples.
  No event-based alignment, packet dropping or AAC-block tolerance is used.
- Three hardware B-frame attempts retain the specific timing rejection. Six
  deliberate byte-limit, cancellation and invalid/incomplete-input cases retain
  their expected non-capability kinds. All 162 final file admissions pass, with
  no process faults.

The native archive contains 308 files, 36,604,739 uncompressed bytes and
2,154,377 compressed bytes. Every archived member was independently rehashed.
Its SHA-256 is
`82652b7462b5bceae31de4d4d7d3ff095718fbc0a92d01583082c64cc799cebd`.

## Review and checks

Independent review found two host paths that could preserve a typed failure
after a later polling error or deadline. Both now invalidate the report. The
first focused run also reproduced a clean-exit regression: the supervisor
classified expected exit code 1 as a fault. An explicit failed-terminal kind
fixes that without changing other protocols. The corrected focused run passes.

The new native qualification helper initially failed to compile because it
called a private capture method and formatted SHA-256 bytes with an unsupported
trait. It now uses the public committed basis for a conservative two-GOP bound
and explicit hexadecimal bytes; the corrected native build and runs pass.
All initial logs are retained alongside the corrected results.

The final locked workspace passes 2,359 tests across 168 result groups, with
zero failures or ignored tests. Strict all-target workspace Clippy and formatting
pass on the same source inventory as the native runs. Eleven Python native
oracle tests pass. The macOS smoke test initializes Metal and completes the
window shutdown callback. The qualified source inventory is
`50c41c44faff1d2cc9419aa59ae56fdf38f52e787ee393b1602591bf13b9bb30`.

[Evidence](../../tools/media-qualification/evidence/2026-09-30-encoded-failures/)
includes native reports and files, source inventories, command journals and the
independent review. No UI code changed, so painted interaction, optional app
feature replay and GUI performance checks were not repeated. Physical listening,
broader hardware/OS qualification, automatic policy, public Render, full effects
and mastering, HDR and release gates remain open.
