# Independent finished-file verification

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
  reordering or 2,048 samples of AAC priming: the measured 1,024-sample codec
  delay plus one 1,024-sample silent encoder preroll block.
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

## HDR (PQ and HLG) files

An `EncodedManifest` whose contract color policy is `HdrRec2020Pq` or
`HdrRec2020Hlg` reconstructs `EncodeContract::new_hdr_v1` and gets the same
structural, clock, packet, fresh-GOP and AAC checks as SDR. Only the video
format checks differ:

- The sample description is `hvc1` with `hvcC` profile space 0, general
  profile 2 (Main10), chroma format 1 and 10-bit luma and chroma; there is no
  `avcC`. `colr` is nclx 9/16/9 (PQ) or 9/18/9 (HLG), limited range, zero
  reserved bits. PQ `mdcv` equals the contract mastering volume exactly, or is
  absent when the contract has none; PQ requires a `clli` with MaxFALL <=
  MaxCLL <= 10000. HLG carries neither box. SDR files must carry no `hvcC`,
  `mdcv` or `clli`.
- Every video packet is traversed as HEVC NAL headers. A sync sample must
  contain only IDR IRAP pictures and no other VCL NAL; other samples contain
  no IRAP NAL. A fresh decoder therefore never starts at a CRA with skipped
  leading pictures. The packet scan itself rejects any NAL of unspecified
  type 62 or 63 (VideoToolbox's Dolby Vision RPU is 62; the encoder strips
  it) in every packet from the observed NAL-type set. Source admission
  independently refuses in-band parameter sets that differ from `hvcC`,
  reserved NAL types and NAL types 62/63.
- Pictures are decoded as tight `yuv420p10le` (`next_yuv420p10`). The stream
  must report codec `hevc`, decoder profile 2 (codec parameters -99 or 2),
  limited BT.2020 NCL BT.2020 primaries with the contract transfer, and the
  same static metadata as the boxes; the decoder fails any picture whose
  interpretation or repeated metadata differs. Chroma location must be Left:
  the measured OS-software PQ HEVC stream declares top-left siting for
  left-sited input and is rejected.
- Fresh-GOP comparison hashes the exact 10-bit samples. HEVC slice types do
  not identify reordered pictures (VideoToolbox codes inter pictures as B
  slices in a three-picture pyramid with B-frames requested), so the
  B-policy check uses the packet clock's maximum reorder delay (decode index
  minus presentation index) instead of consecutive B slice types. It must not
  exceed the requested B frames and must be positive when they were requested
  over more than one GOP. The coded-size pixel allowance uses 64x64 CTBs after
  applying the measured 160x64 VideoToolbox minimum. A 96x64 output has a
  160x64 SPS raster on this Mac; its conformance crop and every decoded visible
  picture must still match 96x64 exactly. PQ/HLG regression files cover 32x32,
  96x64, 144x64 and 64x96 outputs.

### Content light (PQ)

The content light check is a sanity lower bound on the `clli` declaration,
not CTA-861.3 verification. A 4:2:0 file cannot reproduce the host's
per-pixel statistics. The host measures max(R,G,B) of clipped linear light
before chroma subsampling. Any per-pixel reconstruction of the decoded
picture combines one pixel's luma with neighbouring chroma, and at saturated
edges it reaches out-of-gamut R'G'B'. In the measurement, 1000 cd/m² content
decoded to 6,800-10,000 cd/m² (nearest chroma) and 10,000 cd/m² (bilinear)
per-pixel maxima. Requiring agreement, or using a decoded per-pixel maximum
as the lower bound, would reject truthful files.

The verifier therefore derives lower bounds. At each chroma site it filters
decoded luma with exactly the encoder boundary's left-sited chroma filter
([1,2,1]/4 on columns x-1..x+1 with edge replication, [1,1]/2 on the two
rows) and combines it with that site's Cb/Cr. The BT.2020 NCL matrix is
linear, so before coding this site R'G'B' is a convex combination of host
pixel R'G'B'; the replicated edge taps still sum to one.

- MaxCLL: the brightest component of a convex combination, through the
  monotone PQ EOTF, cannot exceed the per-pixel maximum. Lossy coding still
  overshoots at dense saturated edges, so the bound is the largest per-frame
  99th percentile of site light (8192-bin histogram, lower bin edge).
- MaxFALL: the EOTF is convex, so a site's light is at most the
  filter-weighted sum of its pixels' light. The site mean equals the frame
  mean only when every pixel carries the same total weight. Interior pixels
  do, but edge replication gives pixel column 0 weight 3/8 (instead of 1/4)
  and the last column 1/8. An unweighted site mean therefore exceeded the
  host frame mean when the left edge was brighter: by about 10 % for a
  4x2 picture with a 300 cd/m² column 0 and 100 cd/m² elsewhere. The meter
  weights chroma column 0 sites by 2/3, which caps every pixel at 1/4. The
  weighted mean cannot exceed the host mean and gives up at most a third of
  one site column (under 0.04 % of the mean at 1920 pixels wide).

A `clli` declaration fails when it lies below a bound by more than
`MAX_CLL_PQ_CODE_TOLERANCE` (32) or `MAX_FALL_PQ_CODE_TOLERANCE` (8)
limited-range 10-bit PQ code values (876 per unit signal). Only the declared
pair must satisfy MaxFALL <= MaxCLL. The decoded MaxFALL bound may exceed
the decoded MaxCLL bound: when fewer than 1 % of chroma sites carry most of
a frame's light (a small highlight on black), the 99th percentile stays
dark while the mean does not.

Measured on 2026-10-05 ([record](qualification/hdr-verification-2026-10-05.md)),
the 99th-percentile bound exceeded the true MaxCLL by up to 20.5 codes on
adversarial dense 1000 cd/m² saturated edges, and by at most 0.2 codes on the
probe and mixed stress content. A decoded per-pixel maximum was measured
as the alternative and rejected: with nearest chroma it exceeded the true
MaxCLL by up to 24.9 codes on the probe (1304.5 cd/m² for 1004.2) and 181.5
to 217.4 codes on 1000 cd/m² edges (6,779-10,000 cd/m²); bilinear reached
10,000 cd/m² on every edge file. Even the site maximum exceeded it by up to
143.5 codes. Either would need a tolerance that makes the check vacuous, so
the 99th percentile stays. The site-mean bound was always 0.3-26 codes
below the true MaxFALL. The check rejects missing or zero ("unknown")
declarations and claims below what most of a frame shows, such as halved
MaxCLL or MaxFALL for the probe. A peak carried by fewer than 1 % of chroma
sites (small highlights) only bounds MaxCLL at the 99th percentile. The check
cannot disprove an overstated declaration. Accurate values remain the encoded worker's
responsibility (`ContentLightAccumulator` over the renderer's exact
statistics).

`VerificationReport.content_light` (`ContentLightEvidence`: declared
MaxCLL/MaxFALL and the decoded bounds in 1/1000 cd/m²) is present exactly
for PQ. It is skipped from serialization otherwise, so SDR and HLG report
bytes are unchanged. `VerificationReport::validate` re-applies the bound
check. Current `policy_version` is 2 for the AAC preroll contract; the HDR
light bound itself is unchanged. The bound is sanity evidence about the
emitted pictures, not CTA-861.3 verification or a display or mastering
qualification. The jobs mirror (`RenderContentLightEvidence`) rechecks only
the PQ range and the declared MaxFALL <= MaxCLL.

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
or HDR contract. It does not compare arbitrary lossy output against every original
rendered picture or sound. Fixture content, absolute event synchronization,
AVFoundation compatibility and platform/runtime behavior have separate actual
qualification. The library [publication host](RENDER_PUBLICATION.md) checks
destination byte identity against the verified candidate, writes the historical
local report and atomically commits the movie without replacing an existing
entry. Automatic platform policy, durable render jobs/recovery, native Render,
public headless render commands, complete mastering/effects, physical HDR
display behavior and release coverage remain required.
