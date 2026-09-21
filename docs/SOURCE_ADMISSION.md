# Source decoder admission

`deadpan-source` checks immutable snapshot headers before FFmpeg parses them.
This bounds the admitted container's allocation requests; it does not establish
editorial readiness, complete format support, or a process-wide heap ceiling.
The decoder still validates actual frames and the host builds a measured index.
Authored import remains separate work.

## Admitted containers

The shared MP4 guard walks a closed, nonfragmented grammar. Video selection
requires exactly one `avc1` H.264 track and allows at most 32 AAC audio tracks.
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

PCM16 RIFF/WAVE audio admission is unchanged. Other codecs and container grammars
remain required product work; rejecting them here does not reduce the spec.

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

## Verification and remaining work

See [the qualification record](qualification/video-admission-2026-09-21.md) for
actual fixture, hostile-header, native decoder and sanitizer results. Most
verification runs through unit tests and real-media headless integration tests.
This adds no user interface and does not qualify import, playback, export,
full-size performance, unknown container variants or every malformed bitstream.
The separately isolated generated-media conversion worker has its own admission
contract; these limits do not claim to replace that boundary.
