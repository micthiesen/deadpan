# Generated Original container admission

The first release runtime failed during `project create-original`, before any
target ownership query or master comparison. Its QuickTime MOV producer wrote
H.264 `colr` at offset 623 with body length 10 and bytes
`6e636c63000100010001`: nclc. Strict H.264 input requires nclx with length 11;
`native/deadpan-source/src/input.rs` therefore rejected it with `invalid_input:
unsupported fixed box length`. A later audio entry was `sowt`, body length 52;
PCM MP4/QuickTime audio is outside the currently admitted mp4a/Opus grammar.
Changing only color metadata would not have admitted this source.

The corrected test-only producer uses ISO MP4, video stream copy, AAC 192k
audio, explicit 48000 movie timescale and edit lists. Root approved source AAC
because the independent physical oracle reads actual qualified decoded media;
master fidelity limits remain unchanged. The exact deterministic nonperiodic
stereo input WAV is unchanged, SHA256
`6ba8da3e750f96e6f57f47bdc2d4731cb4a5bd4c52a723b46e1b1fe1dbfc541c`.
No source video was reencoded, audio resampled, authored clock shifted, or
production admission changed.

Direct diagnostic producer invocation, with the original retained input WAV:

```text
ffmpeg -nostdin -v error -i native/deadpan-source/tests/fixtures/cfr-bframes.mp4
 -i <failed-producer>/nonzero-original.wav -map 0:v:0 -map 1:a:0
 -c:v copy -c:a aac -b:a 192k -flags:a +bitexact
 -video_track_timescale 30000 -movie_timescale 48000 -use_editlist 1
 -map_metadata -1 -fflags +bitexact -movflags +faststart+write_colr
 -f mp4 <scratch>/producer-admission/nonzero-original.mp4
```

The development producer SHA256 is
`768609fb3f463ff9272bebac61478515f19e43c5343b0c60dde2e6230c40013e`.
The generated diagnostic MP4 SHA256 and complete packet/atom metadata are in
`accepted-media-summary.json`, `accepted-probe.json` and `accepted-atoms.json`.
Its H.264 colr is nclx length 11. Comparison of all 120 video packets proved
identical data hashes, PTS, DTS, durations and sizes to the measured source.
Audio has 189 packets: first PTS −1024 and skip 1024; final PTS 191488,
duration 704, discard_padding 320. Audio elst has media_time 1024 and
segment_duration 192192 on the explicit 48000 movie clock.

The existing release CLI, SHA256
`ef68f69ad242c16cb4222150b5067512164bad42a5fb525266190ff789c15c8d`,
successfully admitted the new file through public `project create-original`.
Its read-only public dump confirmed video [0,120120) at 1/30000,
audio [0,192192) at 1/48000, 120 frames and 30000/1001 presentation rate.
The executed original CLI was rehashed afterward; its private same-byte copy
is retained only in scratch. This admission check is not a master/export pass.

The public dump also exposed an independent pending fixture assertion error:
AssetRecord.content_hash is a `blake3:` identity, while the fixture compares it
with a hexadecimal SHA256. Root was notified to correct that separate build
assertion; this producer-only edit leaves it untouched.
