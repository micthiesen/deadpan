# Next boundary: isolated verification of the emitted MP4

## Recommendation

Add a separate, supervised verification operation after `encoded_render::encode` returns its private `EncodedCandidate`. Reuse the descriptor-only software decoders and bounded MP4 admission in `deadpan-source`, with a focused export-inspection extension for observations those APIs currently omit. Keep encoding and verification as separate process attempts. Keep publication closed until verification, the selected runtime's timing qualification, and destination-side publication are implemented.

Do not route the movie through `deadpan_media::canonicalize`. Its `converter.c::decode_input` expects full-range GBRP H.264, explicitly discards audio, and `verify_output` verifies a single FFV1/BGR0 Matroska stream against RGB scratch. It changes the artifact and cannot establish H.264/AAC MP4 export correctness. Its descriptor AVIO, external-open denial, pinned-runtime check and clean-process admission are useful patterns.

This recommendation is based on source inspection only. No build, test, native program, browser or network access was run.

## Existing code to reuse

| Responsibility | Exact source/API | Current limit or gap |
|---|---|---|
| Host-owned emitted bytes | `crates/deadpan-cli/src/encoded_render/host.rs::encode`, `EncodedCandidate::{read_at,copy_to}` | A cleanly reaped encoder and hash-checked private snapshot; no media verification. The candidate owns no independently captured reference pixels/PCM or complete retained source inventory. |
| Snapshot containment | `crates/deadpan-jobs/src/artifact.rs::ArtifactWorkspace::snapshot_with_control`, `HashedArtifactSnapshot` | Bounded no-follow descriptor traversal, exact length/hash, before/after file-state checks. Snapshot exposes Read/Seek only, no descriptor handoff. |
| Process transport | `crates/deadpan-jobs/src/process.rs::{WorkerProtocol,SupervisedProcess,ProcessSpec}` | Versioned framed control, cancellation, bounded messages/logs, successful completion held until clean group/leader/pipe teardown. stdin/stdout are reserved control pipes; no extra inherited-file API. |
| Full video decode | `native/deadpan-source/src/lib.rs::SourceDecoder::{open,next_metadata,next_rgba,copy_current_rgba,seek}` | `next_metadata` really decodes every AVFrame. PTS, measured duration, key flag and optional DTS are available. Profile, actual picture type, packet clocks, NAL/IDR status and chroma location are not public observations. RGB copies also cannot serve as exact decoded I420 planes. |
| Full AAC decode | `native/deadpan-source/src/audio.rs::AudioDecoder::{open,open_first,next_metadata,copy_current_interleaved_f32}` | Existing mode uses `AV_CODEC_FLAG2_SKIP_MANUAL`; retains physical PCM, original PTS, duration, discard and skip evidence. No ordinary-skip mode or AVFoundation decoder API. |
| Allocation admission | `native/deadpan-source/src/input.rs::{validate_selection,mp4,movie_box,track_box,edit_box,media_box,sample_description,validate_track}` | Closed bounded MP4 grammar before FFmpeg allocation. Already parses signed elst media time and rate, but discards their values; accepts an optional empty edit for import, which export should reject. Does not require fast-start ordering. |
| Exact audio mapping | `crates/deadpan-media/src/audio_index.rs::AudioIndexSnapshot::new_controlled` | Demonstrates measured duration/manual-skip coverage and rejects gaps in physical decode positions. Do not import its whole retained index/cache requirement into the verifier. |
| Canonical inputs if comparison is required | `ProjectPictureSession::open_revision`, `ExportPictureSession::prepare`, `OfflineAudioSession::{open_revision,read}` | Can regenerate the captured revision/range with bounded frames/blocks and the existing document hash check. Cold source availability is required; missing historical media must fail explicitly. |

The native decoder C implementations already use positional descriptor I/O, bounded allocation, denied secondary opens, disabled MOV external data references, pinned LGPL/network-disabled FFmpeg, explicit corruption errors and one-thread software decode. Preserve these protections. Copying the developer probe's path-opening or broad FFmpeg probing would regress admission.

## Concrete shape

Suggested host API in a focused `encoded_render/verification` module:

```rust
verify_candidate(
    runtime: &VerifierRuntime,
    candidate: EncodedCandidate,
    limits: VerificationLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(VerificationProgress),
) -> Result<VerifiedCandidate, VerificationError>
```

`VerifiedCandidate` must have a private constructor, own the same host snapshot, and retain a versioned report plus the exact contract/document/movie hash bindings. It grants no destination path or write authority. Do not name a metadata-only or partial check result `VerifiedCandidate`; retain the current candidate type until all admitted checks complete.

Use a strict verification protocol with its own request/attempt/token, the full `EncodedRenderContract`, movie length/SHA-256, explicit verifier limits, and remaining timeout. The encoder report is retained as a separate claim to compare against observations, never passed as the authority for observed priming, packet counts, profile or B-frame behavior. Stream inventory and observed media clocks come from the movie.

Prefer a native inspection module within `deadpan-source`, rather than adding decode responsibility to `deadpan-encode` or broadening FFV1 conversion semantics. A safe entry such as `inspect_export(File, ExportInspectionLimits, DecodeControl)` can expose a bounded MP4 summary and actual packet/bitstream observations. Extend the existing video frame observation with the few required native fields, or add a focused verification frame API that retains tight decoded Y/U/V planes. Share the existing header guard and descriptor owner. The child orchestrator performs complete streaming decode and host-contract comparisons in Rust; unsafe remains inside the existing native adapter.

### Input handoff

The smallest route that preserves current supervision is a new private verifier workspace with the single fixed input `input/movie.mp4`. Copy from the host-owned candidate with `copy_to`, checking the same deadline around every chunk. Create directories privately and the leaf exclusively with no-follow semantics. The verifier opens only that fixed relative name, validates owner/type/link count/length, hashes the owned read-only descriptor, and uses descriptor-only decoders. Recheck exact extent and hash/file state when finished. It receives no arbitrary media path from the request and never reopens the original encoder workspace.

Keep the original anonymous candidate snapshot in the host throughout. If staging or verification fails, it remains an unverified candidate; no authored state changes. After the verifier exits cleanly, bind its observations to the original host hash, not just the staging path or the verifier's self-reported checksum.

Direct descriptor transfer could remove this extra compressed-file copy, but is not free with current APIs: `ProcessSpec` always makes stdin/stdout control pipes, and `HashedArtifactSnapshot` hides its file. The converter's `.stdin(Stdio::from(input))` pattern conflicts with framed stdin cancellation. An explicit Unix-socket descriptor transfer would need a separately reviewed transport addition and inheritance tests. Do not clear CLOEXEC globally or add unchecked `pre_exec` tricks for this step. Use the contained copy first.

## Required complete-file checks

Derive all clocks from the trusted picture contract. For rate N/D and F output frames, video ordinal n is nD in time base 1/N and the terminal endpoint is FD/N seconds. Audio length is `A = B(project_end) - B(project_start)`, with output samples `[0,A)`. Recompute both absolute B endpoints, not B(range duration). Video and audio endpoints can legitimately differ by the origin rounding phase, within one output sample. At 30000/1001, `[1,2)` has 1601 audio samples while the picture duration is 1001/30000 seconds. Never force the two edit durations to be equal.

### Container, packets and edits

- Require one ordinary MP4 movie, one H.264 video and one AAC audio track, a complete single bounded media-data extent, and fast-start moov before mdat. Reject extra tracks, fragments, encryption, external references, unqualified metadata and conflicting sample descriptions.
- Inspect mvhd/mdhd/tkhd clocks, durations, matrix and sample tables. The movie scale must equal the checked LCM policy, and each track's exact endpoint must match its own authored interval. Bind tracks by their IDs/handlers, not array ordering.
- For this qualified output path, require exactly one normal-rate media edit per track. Parse v0/v1 signed media time and unsigned segment duration with checked arithmetic. Reject empty, gap, repeated, dwell or speed edits. Keep the source-import allowance for an optional empty edit separate.
- Require audio's media offset to equal independently observed AAC priming, matching negative first packet/frame PTS and leading skip evidence. Confirm the edit selects exactly A presented samples and only the measured codec padding falls outside them. A zero padding field alone is not proof of no padding.
- Require video's edit offset to explain only observed initial decode reordering: first presented picture PTS zero, negative first decode timestamp consistent with that offset, and a bounded whole-frame reorder depth matching the qualified attempt. Preserve negative priming/reorder coordinates in the report.
- Scan every packet, checking bounded size/side data, corruption, exact stream identity, strict per-stream DTS ordering, valid PTS/DTS relations and positive duration. Packet PTS is allowed to reorder for B frames; decoded presentation PTS must follow the exact CFR sequence. Cross-check sample-table counts, packet counts and decoder drain. Missing or conflicting durations fail; the verifier must not repair them from intent.

### Video

- Decode all F pictures through EOF and flush, including the tail after the last input packet. Require exact ordinal PTS and measured positive duration, terminal endpoint, even raster, progressive eight-bit 4:2:0, H.264 High, square pixels, identity display orientation and explicit limited Rec.709 range/matrix/transfer/primaries with left chroma siting. Reject frame-level changes or HDR/ICC interpretation that conflicts with SDR.
- Record actual picture types, key/IDR observations, maximum consecutive B run and GOP intervals. Enforce the qualified choice; a queried encoder `has_b_frames` field or container key flag does not prove the bitstream policy. Very short movies may contain no B pictures; handle that explicit policy separately from a longer requested-B attempt.
- Establish closed GOP behavior with bitstream IDR/dependency checks and fresh decode comparisons. The developer `export_probe.c::export_gops` decodes every suffix from every keyframe, which becomes quadratic for long movies. Production needs a bounded linear-work approach: compare each independently decoded GOP (with explicitly bounded next-GOP reorder context if necessary) to the continuous decode's exact PTS and decoded-plane digest. Fresh context must begin at the asserted random-access packet, not quietly preroll a previous GOP. Keep this a qualification gap until implemented; a key flag alone is insufficient.

### Audio and actual content

- Decode every physical AAC block with manual skip, including priming and final padding, and copy/check finite stereo PCM. Require AAC-LC, exactly 48 kHz FL/FR stereo, exact sample-grid PTS and contiguous physical coverage. Preserve every observed sample and skip/discard record in bounded accumulators or evidence, then prove the selected presentation coverage is exactly `[0,A)` without gaps or overlaps.
- Add a separately qualified ordinary-skip decode observation if it is part of the product's timing contract. The existing source API only provides manual mode. AVFoundation remains a development harness today; it is not a production reader that this implementation can claim to have run.
- Full decode plus exact clock/coverage checks is meaningful production media validation. It does not independently prove that arbitrary content is the correct scene or that AAC transients were not shifted within apparently correct PTS. The encoder's own hash/count report cannot close that gap.
- For per-file content comparison, regenerate the same immutable picture and canonical PCM inputs in bounded blocks, or retain host-bound reference evidence during rendering. Compare decoded output at its observed timestamps to the corresponding output ordinal/sample; never search for an offset, shift PCM, drop packets, crop by detected events, change gain or grant an AAC-block tolerance. The current candidate has no such independent reference capability.
- `tools/media-qualification/compatible/project_encode_oracle.py::{compare_plane,compare_pcm}` demonstrates fixed-coordinate comparisons over complete fixture content. Its four-second/1920x1080 bounds and error thresholds are fixture evidence, not a qualified universal production acceptance policy. General lossy content thresholds and full-source dependencies still need qualification. Silence must remain an explicit case, not a reason to invent marker evidence.

The shipped runtime/encoder path also needs beginning/middle/end impulse and visible-frame-transition qualification in ordinary FFmpeg, manual FFmpeg and the target platform reader. Bind any measured decoder/container tolerance to that runtime/profile and retain the evidence. The maximum tolerance must remain below one output video frame; do not derive a tolerance from the AAC block size. Timestamp checks alone do not replace these event fixtures.

## Bounds and failures to resolve before implementation

Current source admission has a 16 MiB aggregate header bound, one million expanded table items and a 16 MiB packet hard bound. The encoder admits two million total packets, 32 MiB packets, and a conservative moov budget of `1 MiB + 128 * maximum_packets` (about 245 MiB at two million). These APIs are not universally compatible. Start with explicitly narrower verifier-admissible request limits, or qualify a bounded expansion of the source guard. Do not silently skip admission for an encoder-produced file. Video decoder pixel limits are configurable up to 8192 squared and can cover the encoder's 33,554,432-pixel bound; the default 16,777,216 must be raised explicitly when needed.

Use one absolute caller deadline across staging, input hashing, header admission, all decode/packet/GOP passes, reference work, report admission and cleanup. Each source call receives `min(remaining, 60 seconds)` and recomputes remaining before the next call. Keep global byte/frame/packet/GOP-work counters in addition to per-call limits. Do not use `AudioSession`'s whole-file PCM cache for this operation; it adds a 16 GiB cache bound and unnecessary retained PCM. Stream one audio block and one picture at a time.

Fail with a bounded diagnostic for malformed/truncated boxes, overflow, unsupported runtime, budget exhaustion, wrong track/profile/color/layout, missing PTS/duration, corrupt/concealed frame, missing/extra packet/frame/sample, timestamp gap/overlap, bad edit list, open GOP, content comparison failure, hash/extent mutation, cancellation, deadline, worker crash, malformed report or incomplete teardown. Map the host outcome to the specification's `ExportVerificationFailed` family without flattening cancellation/deadline. No failure may return a verified wrapper or mutate project history.

## Qualification and implementation order

1. Expose bounded MP4 observations from the existing parser, with pure malformed-box and exact-clock tests; add the missing native video/packet observations without weakening source admission.
2. Implement a real verifier child that hashes and fully decodes the staged candidate under a strict `WorkerProtocol`, then admits a report only after clean teardown. Include arbitrary range origins, odd-canvas/even-output geometry, one-frame and short-final-AAC cases, both supported encoder choices, and rejected hardware-B capability.
3. Add packet/edit corruption, same-length mutation, missing middle/last frame, missing first audio event, 1024-sample shift, wrong sample rate/layout, extra streams, nonzero starts, long/open GOP and false success followed by crash/late output. Use actual media for decode failures and hostile workers for protocol/teardown cases.
4. Qualify the fixed-coordinate content/timing policy and source retention needed for arbitrary projects. Preserve all-reader failures; a structural decode pass cannot overwrite failed event evidence.
5. Only then add destination-side unique `.partial` writing, full verification of the exact bytes to be published, synchronization and atomic final rename, plus durable job/recovery state and a private local provenance report. Those remain separate unfinished production work under §§18.3 and 22.7.

The report should retain project/revision/range/document hash, exact video/audio clocks, movie SHA-256 and size, actual stream/packet/decode/edit observations, encoder claims separately, verifier/runtime versions, selected qualified timing policy, comparison results and source/artifact provenance. Derive provenance from the captured plan and verified receipts; neither current HEAD nor private URLs belong in public media metadata.
