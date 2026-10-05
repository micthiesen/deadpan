# Source decoder admission

`deadpan-source` checks immutable snapshot headers before FFmpeg parses them.
This bounds the admitted container's allocation requests; it does not establish
editorial readiness, complete format support, or a process-wide heap ceiling.
The decoder still validates actual frames and the host builds a measured index.
Authored import remains separate work.

## Admitted containers

The shared MP4 guard walks a closed, nonfragmented grammar. Video selection
requires exactly one `avc1` H.264 or `hvc1` HEVC track (see HDR sources below)
and allows at most 32 AAC audio tracks.
Audio selection uses the same all-track checks before opening its selected AAC
decoder. Declared packet sizes, sample/chunk/time table expansion, external data
references, nested metadata and codec descriptors are checked before demuxing.
The limits include 16 MiB of headers/read work, one million aggregate table rows
and samples, 100,000 atoms, depth 16, and 64 KiB codec configuration records.
Profile 244 AVC configuration is admitted for the existing lossless RGB fixtures.

Video Matroska admission requires one `V_FFV1` track, finite Segment and Cluster
lengths, and a closed element grammar. Every cluster and packet declaration is
checked, including metadata following picture payloads. SeekHead and Cue targets
must resolve to actual structural boundaries, not matching bytes inside a packet.
The scanner caps metadata and header reads at 16 MiB, metadata elements at one
million, depth at 16, strings at 4 KiB and CodecPrivate at 64 KiB. A small bounded
read window avoids charging a large page for each sparse packet header. Lacing,
compression, encryption, attachments, chapters, unknown elements and multiple
tracks remain rejected. SimpleTags must be flat, with at most 1,024 in the file;
this bounds FFmpeg's language-dependent metadata expansion and dictionary work.
The audio adapter still rejects Matroska.

PCM16 RIFF/WAVE admits one plain `fmt16` or one closed extensible `fmt40` before
its aligned nonempty data chunk. The extensible form requires tag `0xfffe`,
`cbSize = 22`, sixteen valid bits, the exact PCM subtype GUID and a nonzero
speaker mask containing only the eighteen canonical WAVE speaker bits, with
one bit per channel. Channel/rate, block alignment, byte rate, file length,
sample count, packet and header budgets remain checked before FFmpeg opening.
Unknown extensions, reserved speaker bits, float and other subtypes stay rejected.
The downstream matrix independently checks whether it supports the declared
layout. Plain WAV channels remain unspecified when the file supplies no layout.
The field contract follows Microsoft's
[WAVEFORMATEXTENSIBLE definition](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible);
Deadpan deliberately admits only the exact PCM16 subset above. See
[source-voice qualification](qualification/source-voices-2026-09-27.md) for real
declared-layout decoding and malformed-header tests. Other codecs and container
grammars remain required product work; rejecting them here does not reduce the spec.

## Codec expansion and controlled decoding

FFV1 v3.4 configuration is decoded with a fixed-memory Rust range parser before
`avcodec_open2`. It validates CRC, quantization context counts, custom probability
transitions and stored initial-state syntax. Admission caps configuration work at
one million binary decisions, slices at 16, quantization tables at eight, and
estimated configuration-controlled state and scratch at 32 MiB. That estimate
uses the pinned FFmpeg 8.0.3 allocation formulas and conservative fixed overhead.
It intentionally rejects some otherwise legal FFV1 configurations. It excludes
frame buffers, packet storage, other decoder allocations and total process memory.
The parser's RFC-derived components retain their Simplified BSD notice in
[the source](../native/deadpan-source/src/video_codec.rs).

Video opening does not call `avformat_find_stream_info` or enable demux parsers.
It opens one software decoder with frame threading disabled, decodes one frame
under controlled callbacks, and retains it for the first caller. H.264's format
callback checks visible and padded coded dimensions before the pinned decoder
allocates macroblock tables. The buffer callback checks picture dimensions again.
Color, depth, interlace, crop, HDR metadata, aspect and timestamp checks remain
mandatory. Opening does not turn container audio observations into decoded audio
readiness.

Every demuxed packet, including unselected audio, is limited to the configured
payload plus side-data byte allowance, at most 16 MiB. H.264 packets have at most
4,096 length-prefixed NAL units. Malformed NAL extents and replacement codec
configuration fail before codec submission. The container guard bounds declared
packet allocations before demux; the packet guard checks the resulting packet.

Header admission and native opening share one cooperative deadline and actual
descriptor-read allowance. The retained first frame counts toward the configured
frame and packet limits. Seeking discards that retained frame and resets the
normal seek counters. Later calls have their own per-call deadlines and I/O
allowances. These are cooperative checks, not preemptive time or OS memory limits.
No process-global allocator setting changes another decoder's behavior.

## HDR sources

One HDR interpretation is admitted: transfer SMPTE ST 2084 (PQ) or ARIB
STD-B67 (HLG), BT.2020 primaries, BT.2020 non-constant matrix, limited range,
decoded ten-bit 4:2:0 (`yuv420p10le`), as HEVC Main10 in `hvc1` MP4 or H.264
High10 in `avc1` MP4. Matroska stays FFV1-only, and FFV1 stream HDR metadata is
still refused before decoding. Eight-bit SDR behavior is unchanged.

Container grammar. `hvc1` requires one `hvcC` (ISO/IEC 14496-15 8.3.3.1) of at
most 64 KiB: configuration version 1, profile space 0, general profile 2
(Main10), chroma format 1, ten-bit luma and chroma, reserved bits set, a 1, 2 or
4 byte NAL length, at most 8 arrays and 64 units, complete nonempty VPS/SPS/PPS
arrays, optional prefix/suffix SEI arrays, and unit headers that match their
array with layer 0 and a nonzero temporal ID. Every SPS is parsed through its
conformance window (emulation prevention removed, at most 512 bytes read):
`pic_width/height_in_luma_samples` must be positive, `chroma_format_idc` 1 and
the window inside the picture, and the coded size, which is what the decoder
allocates, must fit `max_dimension` and `max_pixels` (`resource_limit`) even
when the sample entry declares a smaller picture. `hev1` is refused
(`unsupported_codec`): its parameter sets may exist only in-band and change
between pictures, which is not trivially safe. Visual sample entries may also
carry one progressive `fiel` (`01 00`), one 24-byte `mdcv` and one 4-byte `clli`;
`mdcv` stores primaries G, B, R like the HEVC SEI and is reported R, G, B.
Only its size is container grammar; its values follow the static-metadata
policy below. `sdtp` sample-dependency tables (one byte per
sample) and an empty bitexact `ilst` are admitted as inert. Dolby Vision boxes
and every other child remain rejected by the closed grammar.

Packet guard. HEVC packets use the same 4,096-NAL and extent bounds as H.264.
Headers must be single-layer with a nonzero temporal ID. In-band VPS/SPS/PPS
are admitted only when byte-identical to an `hvcC` unit; any other set fails as
`stream_changed`. NAL types 62/63 (Dolby Vision RPU/EL) fail as
`unsupported_hdr`; reserved VCL (10..=15, 22..=31) and reserved/unspecified
non-VCL (41..=61) types fail as `unsupported_codec`. Non-reference skipping
stays H.264-only. Fresh-keyframe opening and restart work for HEVC: the key
packet must contain only IRAP slices (16..=21) and decode as a key I picture.
A fresh decoder at a CRA or BLA_W_LP sets NoRaslOutputFlag, so FFmpeg silently
drops that IRAP's RASL pictures (they reference the previous GOP). A fresh
start therefore fails `invalid_keyframe` when a RASL NAL (types 8, 9) follows
its CRA/BLA_W_LP before any trailing or IRAP picture; restart at an IDR, or at
a CRA without RASL pictures, is exact. Ordinary seeks are unaffected because
they decode from the preceding key picture.

Decoding. The format callback admits ten-bit `yuv420p10le` in addition to
eight-bit formats; first-picture admission then requires the HDR
interpretation above. A ten-bit SDR picture fails `unsupported_depth`; HDR
transfer with other primaries, matrix or range fails `unsupported_primaries`,
`unsupported_matrix` or `unsupported_range`; another depth or layout fails
`unsupported_depth`/`unsupported_pixel_format`; FFV1 fails `unsupported_codec`;
SDR HEVC fails `unsupported_codec`. HEVC and H.264 infer left chroma siting
when the VUI omits it, as both specifications define; the existing explicit
siting requirement is otherwise unchanged.

Static metadata. Mastering display (SMPTE ST 2086) and content light
(CTA-861.3) are admitted only with PQ or HLG, from stream side data (MP4
`mdcv`/`clli`) and the first picture's SEI. One rule set, owned by
`deadpan_core::MasteringDisplay::check` and `ContentLight::is_valid`, applies
to source admission, stored qualification, render contracts and the encoder:
every chromaticity is a positive CIE xy point with x + y <= 1, the R, G, B
primaries form a counter-clockwise triangle strictly containing the white
point, the peak is 50..=10000 cd/m2 and black at most 50 cd/m2 below it;
MaxCLL is at most 10000 cd/m2 and MaxFALL at most MaxCLL. A declaration that
fails it, lacks primaries or luminance, or does not convert exactly to the
contract units (chromaticity times 50000, luminance times 10000 cd/m2) is
treated as absent with a recorded note (`ColorMetadata::ignored_static`, the
qualification wire's optional `ignored_static` object), never refused and
never passed to the encoder. Stream and first-picture declarations must agree
and a later picture that repeats either must equal it, compared on FFmpeg's
raw rational values whether valid or not, otherwise `stream_changed`; absence
keeps the declaration. SDR with any static metadata,
packet-level static metadata, HDR10+, Dolby Vision, HDR Vivid, ICC and ambient
viewing environment remain refused.

Output. `next_rgba`/`copy_current_rgba` keep producing RGBA8 for every admitted
source; for HDR this is quantized nonlinear PQ/HLG R'G'B' for analysis only.
`next_rgba16`/`copy_current_rgba16` return packed little-endian RGBA64 with
`sample_bits == 16`: chroma is bilinearly interpolated at the decoded siting,
the explicit matrix and range map to full-range nonlinear R'G'B' in double
precision, clamped to [0, 1] and rounded once; transfer and primaries are
unchanged. The clamp is a limitation of the integer full-range output:
limited-range super-white (Y' above 940) and sub-black (below 64) codes, and
matrix results outside the unit cube, are clipped; the ten-bit planes keep
them. Per-frame luma and horizontal chroma-tap tables replace per-pixel
descriptor reads and divisions with bit-identical results. This bypasses libswscale because pinned 8.0.3's ten-bit to RGBA64
path ignored the explicit BT.2020 details (it returned green 0 for a patch whose
expected green is 22205/65535). `next_yuv420p10`/`copy_current_yuv420p10`
return the decoded ten-bit planes unchanged for even-dimension HDR pictures.

## Verification and remaining work

The HDR grammar and decoder are recorded in
[the HDR admission note](qualification/hdr-source-admission-2026-10-05.md).

See [the qualification record](qualification/video-admission-2026-09-21.md) for
actual fixture, hostile-header, native decoder and sanitizer results. Most
verification runs through unit tests and real-media headless integration tests.
This adds no user interface and does not qualify import, playback, export,
full-size performance, unknown container variants or every malformed bitstream.
The separately isolated generated-media conversion worker has its own admission
contract; these limits do not claim to replace that boundary.
