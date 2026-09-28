# Process launch qualification, 2026-09-27

On macOS, Rust 1.97.1 creates a standard-library pipe with `pipe()` and then sets
`FD_CLOEXEC` on each endpoint using separate calls. A concurrent `Command::spawn`
can inherit an endpoint between those operations. This is independent of the
worker's process group. The inherited endpoint can keep a different worker's
stderr open after its leader and group have exited.

The sound-event workspace gate exposed this in the unchanged jobs supervisor:
1,104 tests passed before two supervisor tests failed. The environment worker had
delivered 40 messages, then reported `worker pipes stayed open after process exit:
["deadpan-worker-stderr"]`; the stderr-volume test also faulted. A subsequent
13-test supervisor run passed without production changes. That passing run did
not establish the cause. The deterministic probe below reproduced the actual
pipe inheritance window using the pinned compiler.

`deadpan_native_process::spawn` serializes the complete standard-library spawn
call on macOS, including pipe setup and release of the child-facing endpoints.
The guard is released before returning the child. Jobs workers, media conversion
workers, and the optional app qualification metadata commands use this boundary.
Linux keeps its ordinary spawn path. No worker deadline or fault assertion was
relaxed.

This is a cooperative boundary for participating launches in the same process.
It does not protect against unrelated direct standard-library or foreign-library
spawns. It does not contain escaped descendants or change process-group cleanup.
Commands using this boundary must not install `pre_exec` callbacks: the launch
mutex is held across fork and such callbacks must not reenter the boundary or
perform arbitrary Rust operations. The adapter adds no unsafe code.

## Run the regression witness

From the repository root on macOS with the pinned Rust toolchain and Clang:

```sh
python3 tools/process-qualification/check_pipe_inheritance.py --output /tmp/deadpan-pipe-inheritance-evidence
```

Choose a new output directory. The driver compiles a small dyld interposer and a
standalone Rust probe in a task-specific temporary directory. The Rust probe
includes the real `native/deadpan-process/src/spawn.rs`, rather than a copied
implementation. `result.json` retains tool versions, source hashes, commands,
stdout, stderr, and return codes. Compilation and probe execution are bounded.
Each command runs in a private process group. The driver retains its unreaped
leader, cleans up the group on success, failure, or timeout, and then reaps that
leader. Rust additionally kills and reaps directly owned fixture children during
normal completion and unwinding. Captured output uses files, so a retained pipe
cannot delay driver timeout handling.

Darwin cleanup uses the native adapter's bounded `libproc` membership rule: the
leader must have exited and a valid result must contain no other group member.
The first driver run failed with `EPERM` when signalling the already-exited
`rustc --version` group; no keeper or compiled probe had started. That failed
runner result remains separate from the product regression. The corrected driver
skips signalling only after confirmed membership. An `EPERM` or `ESRCH` after a
signal attempt requires another ownership and membership observation; neither
proves cleanup. A checked leader fallback retains any group-cleanup failure.
Command output, exit status, observation errors, and cleanup errors are written
to evidence before failure is raised.

The driver's failure paths can be checked without compilers or subprocesses:

```sh
python3 tools/process-qualification/test_driver.py
```

At this handoff, all four driver failure-path checks pass; the corrected full
raw/serialized proof passed in the parent's serial runner. The original proof
failure is retained in `spawn-proof-01.log` with its source identity. Its result
must remain recorded alongside the corrected run.

The interposer pauses the actual `pipe()` return before Rust can set either
close-on-exec flag. While that window is held open, a second thread attempts to
launch an unrelated keeper process. The probe checks both paths:

1. Raw launch: the keeper launches before the pipe is released. After the first
   child is reaped, nonblocking stderr returns `WouldBlock`. Reaping the keeper
   makes that same stderr return EOF.
2. Production launch: an earlier failed executable launch releases the guard.
   A same-module `try_lock` proves the real production lock is held during the
   paused pipe return. The keeper launch remains pending for a bounded observation while the
   first launch owns the inheritable pipe. After release, the first child is
   reaped and stderr returns EOF while the keeper is still alive.

The bounded pending observation distinguishes an excluded launch from a merely
fortunate final EOF. There are no repeated timing retries. The C interposition
and raw descriptor inspection exist only in this qualification fixture, outside
the product crates and ordinary Cargo tests.

## Verified result

The corrected witness passed on the Apple M5 Max with Rust 1.97.1 and Apple
Clang 21.0.0. Raw launch retained the pipe until the keeper exited; the production
guard returned EOF with that keeper still alive. All six driver commands recorded
confirmed group cleanup. Formatting and strict affected-crate and optional
app-harness lint passed. The affected jobs/media/process suite passed 146 tests;
the optional app harness passed 246 tests. Production source hashes stayed
unchanged during these Cargo checks. The original workspace failure remains in
the [root-sound qualification](root-sounds-2026-09-27.md).

[Retained evidence](../../tools/process-qualification/evidence/2026-09-27/README.md)
includes compressed logs, command records, source identities and the complete
raw/guarded proof. Independent review found no remaining launch-boundary or
qualification-driver defect. This is macOS process qualification; no new GUI,
physical audio device or Linux execution result is claimed.

## Launch inventory

The production call sites are `jobs::WorkerProcess::spawn`,
`media::conversion::convert_snapshot`, and the app's optional
`ui_harness::command_output`. The last explicitly retains `Command::output`
defaults: null stdin and piped stdout/stderr, followed by `wait_with_output` after
the launch guard is released. Native source decoding and media-worker execution
create no subprocesses themselves.

The jobs supervisor integration binary also compiles its worker fixture with a
raw `rustc` command. Its single `OnceLock` completes before any test obtains the
executable and starts a worker, so it cannot overlap these worker launches. The
fixture's descendant commands run in separate child processes. The jobs and
media library reaping tests have one raw shell launch each but no concurrent
piped worker launches in those library test binaries. The media host-boundary
integration binary routes every subprocess through media conversion. CLI and app
headless tests launch child executables without starting local worker processes;
artifact/original FIFO helpers, store crash tests, native ownership tests, build
scripts, and render qualification commands likewise run in separate binaries or
have no overlapping local worker pipe setup. New in-process process launch sites
must use the shared boundary when they can overlap an owned worker.

Upstream source for the pinned implementation:
[pipe setup](https://github.com/rust-lang/rust/blob/1.97.1/library/std/src/sys/pipe/unix.rs)
and [Unix process launch](https://github.com/rust-lang/rust/blob/1.97.1/library/std/src/sys/process/unix/unix.rs).
