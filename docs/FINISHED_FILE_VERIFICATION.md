# Independent finished-file SDR verification

`deadpan_cli::encoded_render::verification::verify` inspects a private completed
[encoded candidate](ENCODED_RENDER.md) in a separate supervised process. It
returns a `VerifiedCandidate` only after complete inspection and clean process
teardown. Every failure returns the original candidate for a new verification
attempt. It neither selects a destination nor publishes a file.

## Ownership and protocol

The host copies its retained candidate into exclusive `input/movie.mp4` in a
private workspace. Descriptor-relative opens reject symlinks, nonregular files,
hardlinks, changed owners and cross-device components. The child opens only this
fixed input. It has no project store, encoder or destination path.

Private dispatch is `--render-verify-worker`, also available through app
`--headless`. Strict versioned framed messages bind request/attempt identity,
cancellation token, complete encoded contract, document SHA-256, movie SHA-256,
exact byte length and limits. Packet, picture and two audio stages report bounded
progress. Progress cannot regress or change its captured total. Complete progress
does not admit a result.

The child hashes the exact admitted extent before and after inspection and
rechecks descriptor metadata. It drains queued controls and joins the reader
before terminal output. Cancellation, malformed controls, unexpected EOF, decode
failure or changed bytes prevents completion. The host uses the shared checked
process supervisor and requires clean leader/group/pipe teardown before admitting
the report. One monotonic deadline includes staging, native work and teardown.
Native calls remain cooperative; process isolation is not an OS sandbox.

`VerifiedCandidate` retains the original private snapshot, its encoder manifest
and the verification report. It exposes no writable descriptor or worker path.
Copying to a caller-owned sink grants no publication authority. A serialized
report is evidence claimed by a producer, not a constructor for verified bytes.

## Actual file checks

- MP4 headers and sample tables must describe exactly one video and one audio
  track, fast-start ordering, identity transforms, the captured raster, square
  pixels and explicit limited Rec.709 color. Movie and track durations use exact
  rational clocks. Each track has one normal-rate edit explaining only video
  reordering or the measured 1,024-sample AAC priming delay.
- Every packet is traversed under source admission limits. Video decode clocks
  are contiguous; unique presentation timestamps cover every captured CFR
  ordinal. AAC packet clocks retain priming and the exact authored terminal
  duration. Sync-table declarations must agree with actual IDR/VCL NAL headers.
- Continuous software decode reads every full I420 picture and drains EOF.
  Actual dimensions, PTS, duration, High profile, limited Rec.709 interpretation,
  left chroma, square-pixel observations and progressive/corruption flags must
  agree. H.264's internal macroblock padding gets a bounded storage allowance;
  visible dimensions remain exact. Unknown container codec profile is accepted
  only alongside High avcC and actual decoded High profile.
- A second decoder starts with fresh codec state at each IDR. Each complete GOP's
  picture bytes and exact clocks must match continuous decode. The admitted
  demuxer is reused, so this does not repeatedly parse the whole file or decode
  every suffix. Actual GOP length and B-picture runs are checked against policy.
- AAC-LC is decoded twice: manual skip evidence preserves physical priming and
  drain samples; ordinary FFmpeg handling applies its own skip policy. Both
  paths must produce finite 48 kHz stereo PCM at exact contiguous timestamps and
  cover every authored sample. Their PCM hashes must agree at fixed authored
  coordinates. The full physical tail is decoded; no packet dropping,
  event-based alignment or AAC-block timing tolerance is used.

Source decoders preserve existing import behavior. MP4 observations and ordinary
AAC mode are explicit APIs. Fresh video restarts allocate new codec state and
retain cumulative work counters. Tight I420 copies do not round-trip through RGB.

## Bounds and qualification scope

The verifier retains the source guard's 16 MiB aggregate header limit, one million
aggregate samples/table rows and 16 MiB packet limit. Caller limits can narrow
admission further. Encoder-eligible files exceeding these bounds fail explicitly;
large-output capacity is not yet qualified. Packet/NAL traversal, picture storage,
audio blocks, control queues and whole-file decode work remain bounded.

This boundary establishes structural and decode validity for the captured SDR
contract. It does not compare arbitrary lossy output against every original
rendered picture or sound. Fixture content, absolute event synchronization,
AVFoundation compatibility and platform/runtime behavior have separate actual
qualification. Automatic platform policy, durable render jobs/recovery,
destination-side partial-file verification and atomic publication, native Render,
complete mastering/effects, HDR and release coverage remain required.
