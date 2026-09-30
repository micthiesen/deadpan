# Automatic SDR encoder admission

`encoded_render::admission::qualify` runs deterministic probes on the current
helper at the requested output raster and rational frame rate. It returns an
owned `QualifiedEncoder` containing the selected probe's private bytes and the
complete decision record. It does not change a project or durable Render job.
Public native/headless Render remains open.

## Probe and selection

Probe recipe 1 uses `3 * GOP + 1` frames, one origin-based ties-to-even audio
endpoint, visible frame-ordinal bits, a moving colored block and color regions.
Its stereo events have distinct signed amplitudes and sample coordinates near
the opening, middle and end. Input generation retains one I420 frame and one
1024-sample block per channel. It opens no media, device or project.

Admission policy 1 tries hardware H.264 with requested B-frames first. A GOP
target of one or two frames starts without B-frames because that interval cannot
demonstrate the requested reorder run. Each attempt has a fresh identity:

- Exact native `VideoTimestampOrder` on a B-frame attempt permits a new attempt
  in the same mode without B-frames.
- Exact `EncoderUnavailable` on hardware, or `VideoTimestampOrder` on hardware
  without B-frames, permits a separate OS software probe.
- The same B-frame timing rejection on software permits software without
  B-frames. Exhausting these choices fails preflight.

Missing the named VideoToolbox codec does not prove software availability. The
software attempt must independently pass. Source, picture, audio, output, I/O,
capacity, deadline, cancellation, protocol and generic native errors stop the
search. Diagnostic text never chooses a path. A later supervision fault
invalidates a typed native claim. Unconfirmed cleanup takes precedence and
cannot become a successful cancellation or an eligible rejection.

## Verification and ownership

Each probe has a fresh supervised process. The native encoder is consumed and
dropped before any decoder opens. The worker reopens the same owned file read
only through checked directory descriptors, verifies its full metadata identity,
and runs the unchanged finished-file verifier with fresh decoder sessions.
That checks MP4 tables, every frame, independent GOP decoding, and ordinary and
manual AAC presentation at fixed authored coordinates.

A separate content pass compares every image plane with the deterministic
input, enforces both per-frame and whole-probe error bounds, and checks all six
signed stereo events at their exact expected samples. It never realigns PCM.
Each plane permits at most 48 code values of absolute pixel error, 1.5 mean
absolute error and 16 mean squared error. Every signed audio peak must reach
0.15 at its exact sample; unexpected peaks outside the marker windows must stay
at or below 0.15. The reports retain measured errors and event coordinates.
The worker hashes before and after inspection. After clean process/group/pipe
teardown, the parent snapshots and hashes the contained output again.

`QualifiedEncoder` has private fields and no deserializer. Its probe cannot
become an `EncodedCandidate` or `VerifiedCandidate` for project publication.
The serialized report is historical evidence. `copy_probe_to` only copies probe
bytes into a caller-owned private evidence sink.

## Bounds and remaining integration

The shared deadline is at most 120 seconds across all attempts. There are at most
four attempts, 121 frames, eight seconds of content, 1024 packets and 512 MiB of
output per probe. Smaller caller limits may fail explicitly. Admission requires
at least 14x16 pixels to distinguish the ordinal strip and color regions;
this recipe bound does not establish platform availability. Actual 14x16, 16x16
and 64x64 runs failed the decoder geometry guard on the qualified host; their
diagnostics need actual dimensions before any bound changes. Smaller geometries
and very low frame rates remain unqualified.

The decision records helper SHA-256/length, kernel build/release, architecture,
recipe identity, ordered rejected probes, actual native codec observations and
complete verification/content results. It hashes the helper before and after
admission. It does not yet fingerprint loaded dynamic-library bytes or supply
a runtime-bound project-encode consumer. Do not cache this result as authority
across encoding attempts or runtime changes.

The existing policy-1 finished-file verifier still rejects requested B-frames
when none are observed. That rejection is fatal here. A future typed absence
result must complete all other file and content checks before it can authorize
a different path. Legitimate all-I/P project content also needs explicitly
versioned verification semantics, without weakening legacy checkpoint admission.

Next, pin automatic algorithm policy per durable job and the resolved encoder,
runtime and probe evidence per encoding attempt. Cold encoding retries must
qualify again; checkpoint verification and publication reconciliation must retain
the original decision. Freeze legacy policy resolvers so future bitrate/GOP
changes cannot reinterpret historical bytes. Then connect native and public
headless Render to that shared workflow. Full mastering/effects, HDR, quality
policy and the supported hardware/OS release matrix remain open.

See [qualification](qualification/encoder-admission-2026-09-30.md).
