# Independent review and retained corrections

The original author was separate from the parent integration owner and
read-only reviewer. On resumption a fresh reviewer inspected all tracked
changes and the untracked picture modules/example. Only the parent ran
Cargo, formatting, tests and Metal.

No material source-receipt, revision, source-clock, decoder-cache, geometry,
shared-adapter or background-rendering defect was found. The reviewer requested
a final whole-run deadline check after history/file work and before marking the
example passed. Both checks are present. The initial report is synced before
work; report rewriting on final completion is not an atomic publication claim.
The pinned SHA-256 type's digest needs explicit byte hex formatting, corrected
after the retained initial compile error.

Actual Metal then passed 31 assertions over 18 frames. The first launcher
attempt collided with its outer journal's report filename and exited before
media work. Distinct inner/outer paths fixed the orchestration only; the exact
same compiled Rust example then passed. Both command receipts and scripts are
retained.

The full workspace run found two incorrect test fixtures, not failures in the
production implementation. The native rotated90 fixture records three clockwise
quarter-turns, as the existing persistent decoder test explicitly asserts; the
new test had expected Clockwise90 rather than Clockwise270. The second fixture
attempted an invalid Source with neither picture nor audio. The reviewed
correction uses a qualified audio-only Source and asserts that its Blank picture
does not retain a decoder. The first correction overlooked its copied independent
video mapping, and the scoped runtime check rejected that too. Resetting the
removed video mapping to FitBeat satisfies core validation. No production code,
assertion tolerance, deadline or decoder interpretation was weakened.

The failed full and first scoped invocations remain failed observations.
After the final test-only correction all eight picture tests pass. Unrelated
successful targets and doctests from the completed full run were preserved.
Only picture/tests.rs changed between that run and the scoped continuation.
The fresh reviewer confirmed the intended test assertions; runtime validation
provided the additional mapping-invariant evidence its review missed.
