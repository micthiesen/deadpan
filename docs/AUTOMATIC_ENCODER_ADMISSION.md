# Automatic SDR/HDR encoder admission

`encoded_render::admission::qualify` runs deterministic probes on the current
helper at the requested output raster and rational frame rate. It returns an
owned `QualifiedEncoder` containing the selected probe's private bytes and the
complete decision record. Consuming that capability through
`QualifiedEncoder::encode` runs one project encoding attempt bound to the
qualified runtime and its frozen controls. Admission does not change a project
or durable Render job. `AdmissionRequest.color_policy` names the committed
revision's output branch: SDR runs the H.264 probe below; PQ/HLG run the HEVC
Main10 probe described under [HDR admission](#hdr-admission-automatichdrv1).

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

## HDR admission (AutomaticHdrV1)

A revision whose automatic color branch (`ProjectPictureSession::color_decision`,
`picture::committed_color`) is PQ or HLG renders under `AutomaticHdrV1`. An SDR
branch, including a tone-mapped fallback from an HDR basis, keeps
`AutomaticSdrV1` byte-for-byte. The algorithm is chosen at job creation from
the branch (`render::output_summary`/`committed_output_summary` report it with
the color policy, reason, `hdr_sources`, tone-map peak and PQ mastering volume)
and `capture_intent` refuses an automatic intent whose algorithm does not match
the captured contract's color.

The HDR probe uses `deadpan_encode::probe::HdrEncoderProbe`: the SDR probe's
clocks, GOP-derived length, shapes and audio markers with planar 10-bit
limited-range BT.2020 NCL codes for the transfer. `ProbeSpec.color_policy`
(serde default SDR, omitted when SDR) selects it, so SDR specs, recipe hashes
and retained decisions keep their exact bytes. Each transfer has its own recipe
domain: `deadpan-encoder-admission-probe-hdr-pq-v1` or `...-hdr-hlg-v1`, over
the HDR `ProbeConfig` (version 1, doubled picture bytes). The probe contract
carries the HDR color policy and, for PQ only, `HDR_PROBE_MASTERING`.

The PQ probe declares CTA-861.3 light computed like the project host: exact
per-pixel max(R,G,B) of its input codes (each shape owns whole 2x2 cells, so
nearest chroma is exact), MaxCLL the brightest pixel and MaxFALL the largest
frame mean, rounded up to whole cd/m². Measured: 1005/168 (320x180, 30000/1001),
1005/166 (1920x1080, 30000/1001) and 1005/169 (3840x2160, 60/1). The fixed `HDR_PROBE_CONTENT_LIGHT`
(1000/203) would understate the 1004.19 cd/m² peak, so the worker does not use
it. HLG and SDR pass no light.

The content oracle (schema 2) decodes with `next_yuv420p10` and compares every
10-bit plane with the exact input codes. Its limits are the SDR limits scaled to
10-bit codes: maximum 192 (SDR 48 x 4), MAE 6.000 (1.500 x 4) and MSE 256.000
(16.000 x 16) per plane and per frame. Schema 1 SDR limits are unchanged. The
encode report must show `video_profile` 2 (HEVC Main10) instead of 100;
`abi_version` stays 1. Settings resolve through `RenderSdrSettings::automatic`,
where `automatic_hdr_v1` equals `automatic_sdr_v1` except video bitrate
`(sdr * 5 + 2) / 4`. PQ probe verification reports carry the verifier's
`content_light` evidence; SDR and HLG reports omit it.

The fallback graph is frozen per algorithm, and `AutomaticHdrV1` deliberately
uses the SDR table: hardware TargetTwo, hardware None after an exact PTS<DTS
rejection, then software. A finished-file verification failure, for example
OS software PQ declaring top-left chroma, is an `Output` rejection and never an
eligible fallback. For HEVC the verifier's `maximum_b_run` is the packet reorder
delay, bounded by the same B-frame policy.

Measured on the M5 Max (2026-10-05), see
[the qualification record](qualification/hdr-automatic-admission-2026-10-05.md):
supervised admission selected hardware TargetTwo on the first probe for PQ and
HLG at 320x180 and 1920x1080 (30000/1001) and 3840x2160 (60/1). SDR kept its
measured hardware TargetTwo PTS<DTS rejection followed by hardware None. The
largest 10-bit errors over all PQ/HLG hardware/software and B-frame
combinations were maximum 73 (Y, hardware 320x180), worst-frame MAE 0.640 and
MSE 12.219, well inside the limits.

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

## Loaded runtime identity

Probe protocol 2 and probe report schema 2 carry fresh runtime observations.
Each worker captures a fixed inventory: the loaded helper, Avcodec, Avformat,
Avutil and Swscale. On macOS the native adapter binds each loaded Mach-O UUID to
the kernel-reported mapped vnode's device/inode, extent, generation and exact
modification, change and birth times. It checks both the image header and role
anchor. Paths locate descriptors; a pathname or numerical library version
cannot establish this identity. Missing images and unsupported observations
fail explicitly; Linux runtime capture is currently unsupported.

The worker opens matching regular-file descriptors and hashes their complete
bytes with SHA-256 in 64 KiB blocks, with a 512 MiB cap per image. Cancellation
and the shared deadline apply throughout. It revalidates mapped identity and
descriptor metadata around hashing, retains all five descriptors through work,
then hashes and revalidates them again before completion. The fingerprint also
contains bounded OS build, hardware model and CPU family observations, plus
system, kernel release/build and architecture. Serialized facts contain no
image paths, mapped addresses, hostname or environment values.

The parent checks the selected helper's hash and extent against the worker's
loaded helper and checks its own kernel facts. Every eligible rejected attempt
and the selected attempt must agree on the full loaded-runtime fingerprint;
failure before a valid capture cannot authorize fallback. The helper is hashed
again after admission. Decisions retain ordered failures, their available
runtime observations, the selected runtime, recipe identity, codec observations
and complete verification/content results.

This establishes backing-object provenance under trusted installed code. It
does not hash resident memory or attest the OS, frameworks, kernel or drivers.
A Mach-O UUID is an identifier, not a digest. Private memory patches and in-place
changes predating capture that preserve the UUID remain outside this proof.
Installed-code trust must come from release and installation controls. Native
filesystem and kernel calls remain cooperative even though reads are bounded.

## Consuming a fresh admission

`QualifiedEncoder::encode` consumes the capability once. It requires the same
request/attempt identity and cancellation token, a direct host-selected helper
with no argument or environment overrides, and unchanged current helper facts.
It binds the selected mode, B-frame policy, raster, rational rate and resolved
SDR controls to the selected probe's full runtime fingerprint.

The project worker independently captures and compares that binding before
opening project media. It retains the matching descriptors through encoding and
revalidates their bytes and platform facts after work. The parent also rechecks
the helper after encoding. Any mismatch fails the attempt. The returned
`AutomaticEncodedCandidate` retains the decision alongside the private project
candidate; the ordinary finished-file verification and publication boundaries
still apply. A passed probe cannot establish the validity of a project movie.

SDR control derivation is frozen as `EncodeContract::new_v1`; `new` remains its
alias. Future bitrate/GOP policies require a separate constructor and identity.
The binding records version 1 and exact resolved controls without changing the
historical `EncodedManifest` wire format or reinterpreting retained checkpoints.
Serialized runtime facts and decisions cannot recreate a live capability. Each
new encoding attempt must qualify again.

## Bounds and remaining integration

Admission's shared deadline is at most 120 seconds across all probe attempts.
There are at most four attempts, 121 frames, eight seconds of content, 1024 packets and 512 MiB of
output per probe. Smaller caller limits may fail explicitly. Admission requires
at least 14x16 pixels to distinguish the ordinal strip and color regions;
this recipe bound does not establish platform availability. Actual 14x16, 16x16
and 64x64 runs failed the decoder geometry guard on the qualified host. The
decoder reports a 192x96 coded geometry, exceeding the respective macroblock
pixel budgets of 256, 256 and 4096. The diagnostic now retains the stage,
dimensions and configured limits. Admission bounds are unchanged; supporting
these rasters requires a separately qualified coded-versus-display allocation
policy. Smaller geometries and very low frame rates remain unqualified.

The existing policy-1 finished-file verifier still rejects requested B-frames
when none are observed. That rejection is fatal here. A future typed absence
result must complete all other file and content checks before it can authorize
a different path. Legitimate all-I/P project content also needs explicitly
versioned verification semantics, without weakening legacy checkpoint admission.

The [durable workflow](RENDER_JOBS.md#automatic-admission-and-recovery) stores
automatic policy per job and the resolved encoder, runtime and probe evidence
per original encoding attempt in database schema 42. Qualification remains
worker work while Queued. The writer atomically records the decision with the
Encoding transition before the worker consumes its live capability. Both stages
poll session ownership and drain owned processes when that ownership is revoked.
Cold encoding retries qualify again; checkpoint verification and publication
reconciliation retain the original decision. Full mastering/effects, HDR
listening/viewing qualification, quality policy and the supported hardware/OS
release matrix remain open.

See [probe qualification](qualification/encoder-admission-2026-09-30.md),
[runtime-bound project qualification](qualification/encoder-runtime-2026-09-30.md)
and [durable workflow qualification](qualification/automatic-render-jobs-2026-09-30.md).
