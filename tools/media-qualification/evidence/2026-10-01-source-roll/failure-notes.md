# Roll verification corrections

`roll-first` failed during compilation after 773.511 seconds. The new mark
regression used nonexistent `AnchorLossPolicy::Drop`; the existing typed policy
is `DeleteOwned`. No runtime tests ran. Cargo drained its outstanding jobs before
the one-token test correction. Production source was unchanged. The original
command, diagnostic and stable source manifest remain in `checks/roll-first.*`.

The app linker also emitted its previously recorded `__eh_frame` size warning.

`roll-second` compiled and ran all selected targets: 57 passed, one failed, none
ignored. The failure was the new reverse-Roll PCM oracle at sample 6506. Its
expected range retained the old support start 5983 while the command legitimately
extended it to 4382. The expected phase 6082.6 lies only 99.6 samples after the
old boundary, inside the exact sinc radius of 128; previously zero-extended taps
now contain source content. The first mismatch was right-channel
0.002016793 versus 0.0020168277.

The corrected regression retains that probe and compares it with a literal
expanded-support oracle at the same exact phase, asserting that the old and
expanded oracles differ. A second probe at sample 6606/phase 6182.6 is beyond
the changed halo and must match the old body bit-for-bit. Before runs once at
chunk size 193; after runs at sizes 193 and 239.
Production code is unchanged; no tolerance was added. The failed runtime and
its stable source inventory remain recorded.

`roll-workspace-final` completed the workspace with 3,275 unit/integration
passes, one failure and two documentation passes. The only failure was the CLI
doctor test's old core-schema expectation (40); the production doctor correctly
reported 41. Only `crates/deadpan-cli/tests/project_commands.rs` changed after
that run. The corrected test passed with the same workspace feature graph.
The full invocation remains failed. `source-proof.json` and
`doctor-assertion.diff` preserve the exact test-only correction.
