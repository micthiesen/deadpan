# Generated media conversion

`deadpan-media` supplies a safe host boundary for generated-video conversion and
exact interior bridge sampling.
`native/deadpan-media-worker` links the qualified LGPL FFmpeg 8.0.3 libraries in a
private process. This is the first implemented media adapter, with a deliberately
narrow input contract. General source import, playback, export, candidate bundle
validation, and authored acceptance remain separate work.
The [2026-09-21 qualification](qualification/media-host-conversion-2026-09-21.md)
records actual captured-model conversions, sanitizer runs, failures, and limits.
The [bridge sampling qualification](qualification/media-bridge-2026-09-21.md)
checks host-derived frames against an independently captured model result.

## Ownership and execution

1. The host supplies a finite immutable reader, its declared SHA-256 and length,
   exact dimensions/frame count/rational frame rate, and explicit byte/time limits.
2. The host copies and verifies those bytes into an anonymous private snapshot.
   A mismatch fails before launching a codec. No writable descriptor is exposed
   by the source snapshot API.
3. A host-selected helper receives the snapshot and an empty private output file
   as seekable descriptors. One bounded JSON argument carries the contract; one
   bounded JSON result returns on stderr. There is no shell invocation or inherited
   environment. The executable must come from the vetted installation.
4. The helper decodes the input into bounded RGB scratch storage, encodes FFV1,
   then independently decodes the output and compares every RGB byte, frame
   ordinal, timestamp, duration, dimension, and color tag.
5. The host waits for clean exit and process-group cleanup, validates the report
   and output length, hashes the private file with BLAKE3, and returns a read/seek
   object. Failure drops the temporary output. Nothing is published to a project.

`canonicalize_bridge` takes one declared native sequence and the original
`BridgeSamplingMap`. It snapshots the input once, preserves a lossless native
master, and derives the sampled master from that same snapshot. One hard deadline
covers the entire pair, including both helper processes and hashing. The outputs
and map stay together in a private `CanonicalBridge`; no partial pair is returned.
Byte budgets are per file, and each helper's scratch file is discarded before the
next helper starts. The aggregate retained disk usage includes both masters and
the input snapshot.

Protocol 1 remains the unchanged conversion request. Protocol 2 adds
`operation: "sample_bridge"`, a native contract, and the typed sampling map.
For output index `j`, the native position is `(j+1)*(M-1)/(N+1)`. The helper reads
the adjacent native frames and blends encoded sRGB RGB8 channels using integer
half-up rounding. It decodes native frames once into bounded scratch and computes
each output frame on demand. Upsampling increases work/output size without
expanding the raw scratch requirement. A fresh decoder verifies the sampled
FFV1 against those expected pixels. Report version 1 is shared by both operations;
its video contract describes the output. Native and output RGB hashes may differ
for sampling, and must match for plain conversion. The paired host also checks
that both helpers decoded the same native pixels.

Run this synchronous boundary on the job service, outside UI/audio callbacks and
database transactions. Cancellation is checked while copying/hashing; the helper
has cooperative deadline checks and a parent-enforced hard process deadline.
The input reader must be local and finite: this API cannot interrupt an arbitrary
blocking implementation of `Read`. Process groups provide cleanup, not an OS
security sandbox. The helper has no authority to accept a candidate or edit a
document.

The host protocol rejects unknown/duplicate fields, oversized messages, changed
contracts, invalid clocks/profiles, inconsistent pixel hashes, and nonzero exit
after a success report. It kills descendants in the worker group before reaping
the leader to prevent PID reuse during cleanup. Pipes that remain open after
cleanup fail within a bounded grace period.

## Qualified profile

The current input route is lossless RGB H.264 in MP4, as emitted by the development
model adapter, with planar 8-bit GBR, full range, sRGB transfer, and BT.709 primaries.
The exact CFR sequence begins at zero and must match the caller's declared count
and dimensions. Generated audio is explicitly discarded and counted in the report.
Multiple video streams and unsupported stream types or transformations fail.
This converter does not reinterpret untagged input or infer missing timestamps.

Limits are explicit: dimensions at most 4096 per axis, at most 10,000 frames,
reduced frame rates from 1 through 240 fps, file budgets at most 16 GiB each,
and a deadline at most 24 hours. The complete RGB scratch size must fit its budget
before launching the helper. These ceilings are validation bounds, not performance
or memory-use targets. The adapter caps individual FFmpeg allocations at 128 MiB
and separately bounds stream/probe work, output writes, and decoder dimensions.
That allocation cap is not an aggregate resident-memory limit; scheduling under
memory pressure and a shipping process memory budget remain unqualified.

Output is FFV1 version 3 in Matroska, BGR0, slice CRC enabled, one video stream,
and the same full-range RGB/sRGB/BT.709 interpretation. The helper checks the
loaded library versions and configuration against the pinned build. Custom AVIO
uses descriptors only and rejects secondary resource opens. The adapter follows
FFmpeg's [custom AVIO ownership contract](https://ffmpeg.org/doxygen/8.0/avio_read_callback_8c-example.html),
including freeing the current AVIO buffer before freeing its context.

The development prefix and helper executable are trusted host installations.
Version/configuration/license checks detect incompatibility; they do not
authenticate a same-version replacement library. Qualification records the
actual tested library hashes. Signed bundle assembly, installation receipt
verification, and clean-machine distribution remain open release work.

Matroska stores millisecond timestamps. Each frame timestamp is rounded once from
its original exact ordinal; the file's default duration is checked separately.
The original rational frame rate and `BridgeSamplingMap` remain authoritative.
Container timestamps never replace that authored mapping. A complete decode can
succeed after trailer truncation, so input hash/length verification remains
mandatory even when every expected picture decodes.

## Developer verification

[Development](DEVELOPMENT.md) describes the explicit FFmpeg prefix build.
`cargo test --workspace --locked` includes tiny real media conversions with
independently known RGB hashes, negative media cases, resource limits, and host
process/transport failure tests. Fixtures have a reproducible developer generator;
FFmpeg CLI tools are not used by the converter or required to run the committed
fixture tests.

On the reference Mac, instrument and test the C adapter with:

```sh
python3 tools/media-qualification/host/build_sanitized.py \
  --work /tmp/deadpan-media-sanitized
```

The directory must be empty. This uses the selected Clang ASan/UBSan runtime and
an explicit Cargo target, records the command/runtime/binary hashes, and runs the
same real-media tests. Target C dependencies such as BLAKE3 SIMD are instrumented
too. Rust and the separately built FFmpeg libraries remain uninstrumented; this
is adapter evidence, not full-library sanitizer coverage.

`cargo run -p deadpan-media --example convert_generated -- WORKER INPUT REQUEST_JSON INPUT_SHA256 OUTPUT`
exercises the same safe boundary against a captured model result. Use absolute
paths for the vetted worker and fixture files. It creates a new output exclusively
and emits a JSON report containing input, pixel, and output identities. This
developer entrypoint does not mark a candidate Ready or bypass the store's
generated-artifact admission guard.

A protocol-2 request requires a second output path after `OUTPUT`, for the native
master. It exercises the paired API and reports both object identities plus the
original sampling map. These developer outputs are written exclusively in order;
a failure writing the second file can leave the first file. This is qualification
output, not atomic project publication.

Any future promotion caller must recheck cancellation, selected candidate,
request relevance, and retained object identities at its own commit boundary.
Returning a private conversion result cannot make those later decisions atomic.

The model protocol currently declares only its sampled candidate. Before durable
acceptance, it must declare the native sequence and provenance as a complete
hash-verified bundle; the host must derive the sampled master from the native
sequence using the persisted request plan, retain immutable
provenance, persist a qualified selected-Ready receipt, and revalidate relevance
and all objects during explicit acceptance. This converter implements the actual
media boundary needed by that flow. It does not yet implement the flow itself.
