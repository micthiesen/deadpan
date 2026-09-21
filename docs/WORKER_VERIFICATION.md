# Worker protocol and process boundary

This slice implements a concrete worker boundary for Section 18. It does not
implement inference, a model pack, an application job service, or durable job
storage. DP-18 remains partial; Gates A and E remain open. The native application
is still a welcome shell.

## Typed protocol and state

[`deadpan-jobs`](../crates/deadpan-jobs/) uses a four-byte big-endian payload length
followed by UTF-8 JSON. Protocol version 1 caps a control payload at 256 KiB before
reading or allocating its body. Encoding is capped too. Clean EOF, truncated
header/body, malformed JSON, unsupported versions, oversized messages, and I/O
failures remain distinguishable. Frames, tensors, and audio never travel in JSON.

The request binds a project and original revision, Hold/request version, request
and attempt IDs, cancellation token, context-manifest SHA-256, scoped workspace
references, exact frame count/rate/dimensions, conditioning/motion constraints,
and pack/runtime identity/version/seed. Serde rejects unknown fields. Identifiers,
hashes, dimensions, and paths validate on construction and deserialization.
Workspace references reject absolute paths, empty components, dot/parent
components, backslashes, and NUL. They do not grant filesystem
authority or prove absence of symlinks.

Worker stages cover preflight, runtime/model loading, conditioning, inference,
decode, encode, and worker-side validation. Progress uses exact completed/total
units inside one stage; it does not invent an overall percentage. The pure
lifecycle checks identity, stage/progress order, cancellation, terminal states,
and target relevance. Completion produces an untrusted candidate in `validating`.
Buffered stage/progress events are ignored during cancellation after identity
validation. Once completion arrives, further worker events are rejected while
host validation, cancellation, and failure remain available.
Only a separate host validation call can make that exact candidate `ready`.
Acceptance remains an explicit revision-aware document command.

Relevance compares project, Hold, request version, and context hash. The original
revision remains provenance, so moving a Hold or making an unrelated edit does
not itself stale the generation. The host must change the request version when
generation constraints or provider inputs change. Stale and detached states do
not silently become current again, even if matching values later reappear.

## Native process ownership

The macOS/Linux supervisor runs one attempt in a fresh process group. The host
selects an absolute executable, arguments, an explicit environment, and a real
workspace directory. No shell runs, and the caller's environment is cleared.
Workers cannot select executables through the protocol.

Separate pipe pumps handle input, framed output, and stderr. The event queue
holds eight bounded messages, the control queue holds two, and stderr retains
only its last 64 KiB while continuing to drain. Polling returns a bounded batch
without pipe I/O or thread joins. The job service must poll regularly to enforce
the maximum duration, cooperative cancellation grace, and terminal-exit grace.
Cancellation sends the same attempt identity and token, then escalates to group
termination. A successful process exit without a terminal response is a failure.
The supervisor retains a completed candidate until successful process exit and
complete pipe/event draining, then emits it immediately before `Exited` in the
same batch. A later protocol error, exit failure, stuck pipe, or cancellation
discards it. Stage/progress messages remain available while the worker runs.

The supervisor observes exit without reaping the leader before signalling its
group, preventing that PID from being reused during cleanup. Nonblocking,
cancellable pipe pumps also bound shutdown if a descendant escapes the group
while retaining a pipe. Escaped processes are outside this supervisor's authority;
it is not an operating-system sandbox. An inherited pipe that remains open is a
failure, not a reason to accept output. Dropping the owner terminates and reaps
the owned process group and joins its pumps on the job service.

On this Darwin host, signalling a zombie-only group returns `EPERM`. The
[`deadpan-process` adapter](../native/deadpan-process/src/lib.rs) handles only
this qualified case: `waitid` must prove the leader exited without reaping it,
then a bounded two-PID libproc query must contain no other group member. The
query includes other UIDs and zombies. Unknown membership, inaccessible lists,
invalid lengths, and full buffers fail closed. Thread-local errno is cleared
and captured because libproc returns zero for both errors and empty results.
This prevents an exited leader from disguising an unkillable live descendant.
Other signalling errors are returned to the host. Membership is a snapshot;
processes that escape the group are outside its cleanup guarantee.

The host must wait for process cleanup, validate artifact paths/symlinks, hashes,
media dimensions/duration/color, and provenance, then promote atomically. None of
those media-validation or promotion steps is claimed by this crate.

## Verification

The [protocol tests](../crates/deadpan-jobs/tests/protocol.rs) and
[lifecycle tests](../crates/deadpan-jobs/tests/lifecycle.rs) exercise fragmentation,
oversized/malformed frames, strict values/fields, wrong identities, invalid state
transitions, progress regressions, cancellation races, host validation, and stale
or detached results. The [subprocess tests](../crates/deadpan-jobs/tests/supervisor.rs)
compile a small Rust fixture and exercise actual stdio and process behavior.
These fixtures test the boundary; they do not simulate a passing AI benchmark.

The native tests cover exact message ordering before exit, explicit environment
and working directory, stderr flooding, cooperative and ignored cancellation,
hard deadlines, malformed output, wrong attempt IDs, messages after completion,
nonzero exit, missing completion, and descendants retaining pipes. Tests also
reject workspace symlinks and invalid executable/deadline configurations.

The native adapter tests use actual process groups to reject a live leader and
an exited leader with another member, accept the sole unreaped exited leader,
and reject a reaped identity. Pure checks cover truncated or malformed process
lists. The only unsafe call is isolated in this adapter, with an explicit ABI,
buffer, errno, and lifetime argument; jobs and core retain `unsafe_code = forbid`.

On 2026-09-20, the complete repository gate passed on Apple M5 Max, arm64,
128 GiB RAM, macOS 26.5.2 (25F84), Rust 1.97.1:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`: 187 passed, none failed or ignored.
  This includes 7 protocol, 10 lifecycle, 11 actual subprocess, and 2 native
  adapter tests added here.
- `cargo build --workspace --locked`
- `cargo run -p deadpan-cli -- doctor`: schema 4, SQLite 3.53.2, exact timing
  probe, and truthful development-foundation capability report.
- The 20 Python audio-measurement regression tests also passed.

Native startup smoke, computer use, and GUI interaction were skipped because no
app startup, view, focus, keyboard, or application lifecycle code changed. The
actual subprocess tests provide the relevant native evidence. Linux and macOS
15 were not locally exercised; a cfg target is not platform qualification.

Independent protocol/lifecycle, supervisor, and native-adapter reviews covered
the new boundary. Review led to two safeguards: proving group membership before
accepting Darwin's zombie-only `EPERM`, and retaining completed candidates until
clean teardown. Regression assertions prove that a completion followed by a
nonzero exit, protocol violation, or terminal-exit timeout exposes no candidate.
Parent review also fixed buffered cancellation progress and duplicate stages
after completion. The original exit-before-deadline and escaped-pipe failures
now have native regression coverage.

A proposed global frame/pixel allocation cap was not added to protocol metadata:
this layer does not allocate media from those fields, and supported work limits
depend on the provider. Provider-specific preflight remains required before
decode or inference; bounded JSON alone does not meet that requirement.

## Remaining work

Persist jobs/attempts and reconcile interrupted workers after restart. Connect a
bounded priority scheduler and resource budgets to application lifecycle, sleep,
playback, and power state. Implement artifact containment, hash/media validation,
atomic promotion, history pinning, and the explicit acceptance command. Connect
the pinned private runtime and actual LTX candidates, then measure generated
holds and preview interference on the required corpus. None of the protocol
fixtures qualifies model quality, GPU latency, packaging, or GUI interaction.
The typed video metadata is not an allocation budget. Before any decode or
inference, provider preflight must validate supported frame counts, dimensions,
rates, and checked work/memory estimates against measured host capacity.
