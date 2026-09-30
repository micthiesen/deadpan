# Discarded H.264 frames

Independent source-only review found no reachable admission gap from the absence
of an explicit `AV_FRAME_FLAG_DISCARD` check in `inspect/pictures.rs`.

Pinned FFmpeg 8.0.3 H.264 uses `FF_CODEC_DECODE_CB(h264_decode_frame)` at
`libavcodec/h264dec.c:1117`. The common `decode_simple_internal` turns video
DISCARD results into EAGAIN and unreferences them at `libavcodec/decode.c:449-459`.
The alternate receive callback filters them too at lines 629-631. Packet DISCARD
can propagate to frame DISCARD at lines 1471-1472, but that result is filtered.

The adapter disables threading and returns only successful `avcodec_receive_frame`
results. An omitted picture fails the verifier's exact count, PTS or endpoint.
No source change was needed. This conclusion is scoped to the pinned decoder.
