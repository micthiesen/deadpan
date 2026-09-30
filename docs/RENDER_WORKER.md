# Isolated committed picture preparation

`deadpan_cli::render_worker::prepare` runs the committed SDR picture producer in
a supervised child process. The host selects a known executable and captures an
explicit revision and half-open range. The child owns media decoding, Metal
composition, readback and a bounded raw I420 artifact. This implements a process
boundary for Sections 18.1 and 18.2; it is not the product Render operation.

## Capture and protocol

The host opens a read-only historical picture session and derives the trusted
[encoder picture contract](EXPORT_PICTURES.md). It hashes the complete validated
authored document with bounded streaming serialization. Project and revision
labels alone do not bind a package that might be replaced before the child opens
it. The child reconstructs the same contract and document hash before allocating
the GPU or creating output.

The private CLI dispatch is `--render-picture-worker ABSOLUTE_PROJECT`. The native
app can use its known executable with the `--headless` prefix. This dispatch is
absent from the ordinary command/help registry. Only the trusted host chooses the
executable, arguments and environment; project data never chooses a program.

Protocol 1 has strict, length-framed JSON messages with the existing 256 KiB
frame bound. Prepare captures request/attempt IDs, a cancellation token, document
hash, every output contract field, fixed output scope, byte budget and timeout.
Progress carries bounded frame counts. Completed carries the same contract and
document hash, exact byte length, SHA-256 and fixed I420 policy. Unknown fields,
stale identities, changed timing or geometry, unexpected paths and malformed
messages fail. Serialized claims do not construct a trusted picture contract.

The current raw range is limited to 100,000 frames and 512 MiB, with a lower
caller budget allowed. This is a bounded encoder-input qualification boundary.
The product encoder must consume the shared frames inside the worker rather
than spool an uncompressed full-length movie.

## Ownership, cancellation and admission

The shared `deadpan_jobs::process::SupervisedProcess` owns bounded pipes, process
groups, cancellation, deadlines and sole reaping. Generation retains its own
protocol adapter and existing wire vocabulary. All launches and group cleanup
use the checked native process adapter. A completion stays private until clean
leader, descendant and pipe teardown. This is process isolation, not an OS
sandbox; escaped processes are outside the owned-group guarantee.

One caller deadline covers host capture, child execution, teardown and artifact
validation. Host preparation belongs on a job service, never the UI or audio
thread. The progress callback must be cheap and nonblocking. Native calls remain
cooperative inside the child; the supervisor provides process termination.

The child requires a bounded initial handshake and keeps a separate control
reader while preparing pictures. It drains already available control bytes
before reporting completion, honors matching cancellation and rejects malformed
or partial controls. Each frame is prepared, checked, written in bounded chunks
and released before requesting another. Only native Metal is admitted.

Output uses an exclusive descriptor-relative `output/pictures.i420` in a fresh
host workspace, with regular-file, owner, device and link checks. A cancelled or
failed range cannot return a prepared result. After clean process exit, the host
takes an independent contained snapshot, verifies hash and exact byte geometry,
and checks every Y/Cb/Cr code against the declared limited-range policy. The
returned private snapshot survives removal of the worker workspace. Reads use
the captured exact output clock and require a correctly sized frame buffer.

## Remaining product work

Durable render jobs and restart recovery, priority scheduling, full picture and
audio graphs, qualified native H.264/AAC encoding and the approved timing
metadata, emitted-file verification, atomic publication, native Render controls,
HDR and complete preview/export equivalence remain required. Raw I420 admission
does not establish an encoded file or a usable export workflow.

[Qualification](qualification/render-worker-2026-09-29.md) records the real CLI,
actual Metal output, complete comparisons, hostile process/artifact cases,
cancellation/recovery, independent review and full workspace gate.
