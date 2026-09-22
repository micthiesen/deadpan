# Worker group cleanup verification, 2026-09-21

The media and model worker hosts now share bounded macOS process-group teardown.
They retain the unreaped leader, repeat SIGKILL while group members remain, and
confirm an exited leader with no other members before reporting cleanup success.
The media host also drains available control bytes before applying its pipe
grace timeout. No audio algorithm, authored state or GUI behavior changes here.

## Failure and diagnosis

[CI run 35678400735](https://github.com/micthiesen/deadpan/actions/runs/35678400735)
failed at source revision `64ee95f1e03e3be780a88c4eb2e2b004518ff44f` in
`successful_leader_exit_cleans_up_descendants_with_inherited_pipes` with
`Protocol("control pipe stayed open after worker exit")`. The same focused
test passed locally before the fix. The log does not distinguish which of the
two defects below caused that particular failure; it contains no group-member
or per-read trace. This is not evidence of a meter regression.

Both hosts treated a successful group signal as completed teardown, then reaped
the leader. On Darwin a group signal operates on a member snapshot: a descendant
can fork after that snapshot and survive that signal. The media retry path also
returned immediately after a successful retry. XNU's implementations of
[group iteration](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_proc.c)
and [group signalling](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_sig.c)
describe the relevant mechanics. Those upstream sources are explanatory, not a
claim to have instrumented this machine's exact kernel binary.

Separately, the media collector read one 4096-byte chunk per poll and then
checked elapsed grace. After delayed scheduling, it could consume the last
buffered bytes and time out before its next read would observe EOF.

## Implementation and invariants

`deadpan_native_process::terminate_owned_group` checks non-reaping `waitid`
ownership before every possible signal. Unknown ownership, including `ECHILD`,
fails without signalling. Successful signals, ESRCH and Darwin EPERM all require
a subsequent membership check. An inaccessible or persistent group fails within
the caller's real monotonic 250 ms cleanup budget. The adapter never reaps.
Both worker hosts on macOS set their stopped flag only after that confirmation.
Their destructors never issue an unchecked `Child::kill` or `wait` after an
ownership or teardown error. If group inspection fails, a separate bounded
leader fallback checks ownership before every signal and confirms exit before
allowing one reap. It does not depend on libproc and does not claim descendant
cleanup. ECHILD or another ownership error prevents signalling or reaping.
The explicit host operation still reports the group failure. A failed fallback
leaves cleanup incomplete. Every wait is marked attempted before entering the
OS call; after a wait error, neither polling nor destruction retries that PID.

The existing bounded libproc adapter remains the only unsafe Rust in this
crate. No new FFI, third-party package or version was added. The existing
process adapter now exposes checked leader cleanup to Linux as well. It
inspects at most two PID slots on Darwin,
failing closed on truncation, malformed lengths or other members. Cleanup is
not a sandbox: a process that deliberately leaves the owned group requires
separate containment policy. Linux retains its group-signal policy without
Darwin membership confirmation, but now checks non-reaping ownership before
that signal and confirms the owned leader's exit before allowing a reap.
ESRCH with a still-running leader is a failure, not successful cleanup. It
shares the checked leader fallback. Both platforms avoid
unchecked PID operations after failed reaping.

The media drain checks cancellation and the overall deadline between reads,
retains the 8192-byte response limit, and evaluates pipe grace only after
WouldBlock. EOF after buffered bytes is a completed reply even if scheduling
consumed the grace interval. A genuinely open writer still produces the original
protocol error.

The first full local gate after these changes exposed a separate fixture defect:
the same media test failed with `trailing characters at line 1 column 535`.
A retained standalone raw-pipe probe reproduced a shell message appended after
valid JSON on trial 12: `Killed: 9               sleep 1`. The protocol parser
correctly rejected this corrupted response. The fixture now duplicates the
inherited control pipe onto descriptor 3 before redirecting each subshell's
diagnostics to `/dev/null`. Its descendants still hold the pipe open and must
be terminated; shell job diagnostics no longer share the JSON channel. The
production parser remains strict. The failed gate and probe are retained.

## Verification scope

Deterministic adapter regressions cover successful signals with remaining
members, repeated ESRCH/EPERM, persistent members at deadline, ownership loss
before the first or a later signal, and unexpected signal errors. A native
test confirms that successful teardown preserves the leader for its owner's
wait and rejects that same child after reaping.
Another native test confirms that the checked leader fallback leaves a separate
group member alive, preserves sole reaping and rejects an already-reaped child.
Both hosts exercise actual ECHILD from an external reap and reject a second
wait through their terminal reap state.

The Linux native-process and jobs suites also ran inside an isolated Linux
aarch64 container with Rust 1.97.1: **75 tests passed**, none failed or ignored.
The official `rust:1.97.1-slim-bookworm` image is pinned in the retained command
record by digest. A real child joins its parent's process group, keeping its
own PID alive while its original group disappears. The regression verifies
ESRCH, checked single-leader termination, one reap and subsequent ECHILD. It
never targets the parent's group. Repository sources were mounted read-only,
and source hashes remained unchanged through the run. The Linux media/FFmpeg
workspace was not built or exercised; this is targeted worker-boundary evidence.

Media regressions distinguish buffered EOF from a genuinely open pipe and
retain budget, cancellation and overall deadline failures. Its existing
32-subshell fixture retains the inherited-pipe and survivor assertions with the
diagnostic isolation described above. The jobs integration suite adds a successful
framed worker with 32 subshells that each start another process, verifies exact
candidate/exit delivery, then checks that no descendant writes its delayed
survivor marker. These are real subprocess checks without opening the app.

The retained [evidence directory](../../tools/process-cleanup-qualification/evidence/2026-09-21/)
contains the failed CI log, source hashes, review record and repository gate.
The final local gate passed formatting, strict workspace Clippy, **794 tests**
with zero failures or ignored tests, the locked workspace build and CLI doctor.
Native startup, GUI, accessibility and device tests are not repeated for these
process-only changes. Full worker chaos, scheduling and containment qualification
remain open under DP-18 and Gate A.
