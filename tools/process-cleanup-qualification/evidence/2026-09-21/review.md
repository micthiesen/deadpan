# Independent review and disposition

Review base: `origin/main` at
`64ee95f1e03e3be780a88c4eb2e2b004518ff44f`, plus the working changes. The review
covered both worker hosts, the shared process adapter, native/integration tests,
platform dependency scopes and the worker documentation. A general reviewer
and a separate process-ownership/concurrency reviewer inspected the code.

Applied findings and follow-up corrections:

1. A raw `Child::kill` fallback could bypass an ownership error from checked
   group teardown. It was removed from both production destructors.
2. Simply omitting that fallback could strand an owned leader when group
   inspection failed. Both hosts now use a separately checked, bounded leader
   fallback. It has no libproc dependency and cannot convert group failure into
   a successful candidate result. Descendant termination remains unclaimed.
3. A failed `wait` after cached successful cleanup could otherwise be retried
   during destruction. Both hosts mark reaping attempted before the OS call,
   reject a second wait and skip destructor PID operations afterward. Jobs also
   rejects subsequent polling/cancellation through that failed-reap state.
4. Linux's existing direct group-signal path lacked the ownership guard and
   initially lacked the checked fallback. It now uses the common adapter for
   both. Darwin membership inspection remains platform-specific.
5. Linux ESRCH could coexist with an owned leader that moved to another group.
   ESRCH now requires confirmed leader exit to succeed. Even a successful group
   signal is followed by checked leader termination before a blocking reap.
   A real escaped-leader regression exercises this in Linux.
6. Worker documentation implied unconditional destructor cleanup. It now
   distinguishes confirmed cleanup, checked leader fallback, ownership loss,
   incomplete cleanup and the Darwin/Linux membership difference.

The final general review accepted the checked fallback/reap state and requested
the platform wording clarification. The final focused review reported no
remaining findings after the Linux corrections. No actionable code finding was
left deferred. These reviews do not claim operating-system containment or
guarantee recovery from arbitrary signal/inspection failures.

The main session independently confirmed the shell fixture's JSON corruption
through retained raw stderr before isolating its diagnostics. The fixture still
inherits the real control pipe on descriptor 3 and checks delayed survivor
markers. Production JSON parsing remains strict.

The separate limiter archive was checked against its original scratch files,
all 134 archive checksums and 45 audited WAV identities. A numerical consistency
review found the qualification report matched its measurements, with one
provenance limitation: the original reports did not bind the meter executable
hash/revision or M5 Max hardware model. The report now labels those as session
metadata rather than archive-proven identities.
