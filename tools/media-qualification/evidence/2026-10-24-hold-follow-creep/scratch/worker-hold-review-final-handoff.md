# Hold Follow creep CLI review strengthening

Changed only crates/deadpan-cli/tests/project_commands/macros/framing/hold_follow_creep.rs.

- Live Hold boundary samples 119, 120, 127 and 128 assert actual provider (Source, live Hold, live Hold, other Hold), baseline picture identity, exact provider local clock, capture context, and all framing layer fields except the intentionally changed owner pose. The existing loop checks every live scale independently.
- Captured Hold samples 135, 136 and 143 assert other Hold/captured Hold/captured Hold providers and the same baseline invariants. Frame 144 is rejected by both baseline and edited plans. The full 144-frame fixture is retained.
- All eight captured fallback scales use independent positive integer half-even Q32 endpoint and blend rounding. No production FramingPose quantization is called by this scale oracle.

cargo test --locked -p deadpan-cli --test project_commands hold_follow_creep passed one test, exit 0, terminal session 96347. Exact invocation, environment, streams and all runtime source hashes are in worker-cli-review-1.json; inputs stayed unchanged. Final source/binary pins are in worker-hold-review-final-freeze.json. No production edit, formatter, Git command or additional Cargo operation in this follow-up. Earlier receipts and failures retained.

CLI fixture's live Hold ends into another Hold at 128; actual Hold-to-Source boundary remains the Root native witness at 52. No claim of that boundary in this CLI test. Shared Cargo target is free; no active worker process or pending check.
