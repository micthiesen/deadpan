# Playback race follow-up review

## Verdict

The test-only correction resolves the stale-update race. No remaining source-level issue found in the revised wait.

## Diagnosis

`Engine::poll()` takes the latest asynchronous update, while the old helper returned as soon as it saw `Phase::Playing`. Since the test had already observed `Playing` before queuing the starvation report, that condition could accept the old sample 0 update before the controller drained the report. The prior failure was test synchronization sensitivity, not a delivery-clock defect.

## Correction reviewed

At `crates/deadpan-playback/src/tests.rs:658-676`, the test now polls within the existing bounded wait until it sees either `Failed` or `Playing` at sample 256. It explicitly asserts the latter, so premature failure is surfaced. The fake stream clock remains fixed at 15,333,333 ns during this wait; the controller must consume and observe the queued report before it can publish the matching sample. The subsequent 15,666,667 ns update still checks terminal sample 272 and the no-resume condition.

This change is confined to the playback test. No production changes, builds, or tests were performed in this review; the parent owns execution.
