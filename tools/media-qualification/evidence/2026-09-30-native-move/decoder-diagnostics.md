# Release replay decoder diagnostics

performance-final exited 0 with all 3,314 replay checks passing. Its log includes
`[h264] decode_slice_header error` before place-slice PASS and
`[aac] get_buffer() failed` before room-tone PASS. The earlier performance run
has neither message. The final report has no failed/timed-out timing sample or
application failure associated with either line.

A separate read-only decoder review found no timestamp, request identity, asset
or operation in these raw FFmpeg stderr lines. place-slice uses real qualification,
endpoint decoding and preview requests which can be superseded. H264 cancellation
or preroll is plausible, but the log does not establish a cause.

Room-tone replay uses indexed AudioRange preparation and simulated playback,
not its real audition PCM path. audio_decoder.c's bounded_buffer can return
AVERROR(EINVAL) on cancellation, deadline or allocation limits; preserving a
prior cancellation error does not suppress FFmpeg's own stderr. This is a possible
mechanism during some overlapping qualification/decode, not an attribution.

Both diagnostics remain unexplained by the retained logs. Do not classify them
as proven harmless cancellation or infer failed rendered output from them.
The actual scenario assertions and decoded media tests remain separate evidence.
