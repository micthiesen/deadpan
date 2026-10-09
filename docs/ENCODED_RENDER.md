# Isolated committed SDR encoding

`deadpan_cli::encoded_render::encode` captures an explicit committed revision and
range, then streams its pictures and canonical PCM through the native encoder in
one supervised process. It returns a private encoded candidate after clean
teardown and independent hash admission. It does not publish a movie or establish
that the finished file passed media verification.

## Immutable input and exact clocks

The host derives the [picture contract](EXPORT_PICTURES.md) and hashes the complete
validated document. The child independently opens picture and offline audio
readers at that revision and range. Both complete documents must match the host's
hash before GPU or output allocation. Project/revision labels alone are not
sufficient evidence when a package path can be replaced.

Picture ordinal `n` uses PTS `nD`, duration `D`, and time base `1/N` for project
rate `N/D`. Audio is read on the absolute project grid `[B(start), B(end))` and
rebased to output sample zero only at the encoder input. Its count is
`B(end)-B(start)`, preserving nonzero-origin rounding phase. Limiter/source context
remains available beyond the selected range; no monitoring gain or extra tail is
added. Unsupported authored processing fails instead of being omitted.

The [native encoder](NATIVE_ENCODING.md) chooses the next input by exact relative
clocks. The child retains one completed I420 picture or one bounded stereo audio
block, consumes it, and releases it. It never writes an uncompressed movie spool.
Explicit hardware/software and B-frame choices belong to the trusted host's
engineering policy. The separate [automatic admission boundary](AUTOMATIC_ENCODER_ADMISSION.md)
can supply them by consuming a fresh `QualifiedEncoder`. A failed attempt cannot
switch its encoder or revise its captured document.

## Protocol and ownership

Private dispatch is `--render-encode-worker ABSOLUTE_PROJECT`, including through
the app's existing `--headless` path. It is absent from ordinary command help.
The host selects the executable, arguments and environment. Authored data never
selects a program.

The encoded protocol is distinct from raw-picture preparation. It uses strict
versioned length-framed JSON and the shared process supervisor. Prepare binds
request/attempt identity, cancellation token, full document hash, exact output
contract, explicit encoder choice, budgets and timeout. Private protocol 3 also
requires a nullable `binding` field in Prepare and Completed: explicit
engineering attempts send `null`, while automatic encoding carries the exact
qualified runtime and resolved SDR controls. Omission is invalid, and completion
must echo the exact binding. Progress counts accepted picture and audio inputs.
Complete input progress is not codec drain, verified media or published output.

The protocol retains a strict typed failure boundary separately from its bounded
diagnostic: control, contract, source, picture, audio, output or native encoder.
Native encoder failures retain specific kinds for missing video encoders and
observed invalid packet timing, distinct from input, resource, I/O and generic
driver failures. Protocol-1 and protocol-2 messages are rejected; retained manifests
and database rows are unchanged. The host keeps typed failures through process
supervision and records their stable diagnostic codes in the workflow journal.
No diagnostic text selects a fallback. A failure kind grants neither fallback
permission nor cleanup evidence; unconfirmed teardown still takes precedence.

The automatic consumer requires the same request/attempt identity and
cancellation token as its fresh admission and rechecks current helper facts
before and after encoding. Before opening project media, the worker captures
the actual mapped helper and Avcodec, Avformat, Avutil and Swscale images and
compares their complete fingerprint with the probe's. It hashes matching
descriptors under cancellation, deadline and byte bounds, retains them through
work, then rehashes and revalidates them and the platform facts before completion.
The [runtime contract](AUTOMATIC_ENCODER_ADMISSION.md#loaded-runtime-identity)
binds macOS mapped vnodes and loaded Mach-O UUIDs under trusted installed code;
it does not attest resident memory, OS frameworks or drivers.

The binding fixes policy version 1, raster, rational rate, encoder choice and
resolved bitrate/GOP/B-frame/timescale controls. Native reconstruction uses
`EncodeContract::new_v1`; `new` stays an alias of that frozen policy. Future
policies require separate constructors. Runtime binding is carried by the
private protocol and decision, without changing historical `EncodedManifest`
bytes or checkpoint interpretation. Probe protocol 2 and report schema 2 are
separate from this project-encode protocol.

The current native ABI and finished-file verification policy are version 2
for [AAC preroll](NATIVE_ENCODING.md#exact-input-and-policy). SDR/HDR video
policy constructors are unchanged. Retained ABI-1 manifests and decisions can
still be read as historical data, but cannot grant fresh encoding or current
finished-file verification authority. A prior-ABI candidate needs a fresh
render to meet the current output contract.

The raw worker retains its 512 MiB/100,000-frame qualification limits. Encoded
requests reconstruct the native contract and enforce native frame, sample,
packet, geometry, duration and byte limits. Serializing/deserializing an encoder
report creates claims only. The trusted native contract remains constructor-only.

Output is exclusive `output/movie.mp4` under a host-created private workspace.
Descriptor-relative opens reject symlink components and existing outputs. The
encoder owns a private read/write descriptor through drain, trailer, fast-start
and synchronization. The child hashes the completed bytes through that same
descriptor, with bounded reads and the original deadline.

The control reader shares the raw worker's bounded nonblocking implementation.
It remains responsive during native work, drains queued controls and joins before
terminal output. A queued cancellation or malformed control prevents completion.
The host requires clean leader, group and pipe teardown before freezing the file
through the contained artifact reader. Hash, exact extent and protocol claims are
checked again at admission. Cancellation or any earlier failure prevents return
of a candidate.

The candidate exposes metadata, reads up to 64 KiB and a controlled copy into a
caller-owned sink. It exposes no worker pathname or writable descriptor. These
methods choose no destination and grant no publication authority. An interrupted
copy leaves a private prefix for its caller to handle.

## Deadlines and remaining product work

One host monotonic deadline covers capture, child work, teardown and snapshotting.
Native driver/filesystem calls are cooperative; the shared supervisor owns hard
termination and checked reaping. This is process isolation, not an OS sandbox.
Host work and callbacks belong off the UI/audio threads; callbacks must be cheap
and nonblocking.

The separate [finished-file verifier](FINISHED_FILE_VERIFICATION.md) checks actual
MP4 tables, every decoded picture, fresh GOPs and manual/ordinary AAC presentation
before returning a private verified candidate. The [publication host](RENDER_PUBLICATION.md)
then checks an exact destination copy and publishes the local report and MP4 under
exclusive names, with explicit outcomes for failures after the movie rename.
The [durable journal](RENDER_PUBLICATION.md#durable-publication-journal) supports
explicit recovery after restart. Native Render and public headless render commands
remain required, as does durable automatic-decision storage planned for database
schema 42. The in-memory automatic consumer returns a candidate and its decision;
it cannot bypass finished-file verification or publication. Full audio/effects,
HDR, durable automatic policy integration, performance and release hardware/OS
coverage remain open. Fixture content and platform qualification remain separate
from per-file structural/decode admission.
