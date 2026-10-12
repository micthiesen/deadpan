# AAC noise substitution and authored sample fidelity, 2026-10-12

The short partial-Repeat export's fidelity failure comes from native AAC
perceptual noise substitution (PNS). The source-projection fix is independent.
The encoder now explicitly disables PNS and checks the resolved option after
opening the codec. AAC-LC stereo, 48 kHz, the normative 384 kbit/s target,
authored PCM, two priming packets, edit timing and verification thresholds stay
unchanged. Encoder ABI 4 identifies this behavior for both SDR and HDR. ABI-3
reports remain historical evidence and cannot authorize a current encoding.

## Reproduction and cause

The previously qualified `0c445fc3` CLI captured all 20,821 canonical limited
stereo samples of the retained partial-Repeat project in consecutive public
256-frame inspection windows. An isolated C probe uses the exact pinned FFmpeg
libraries and the native encoder's settings, including the silent 1024-sample
preroll at PTS -1024. It retains every decoded frame at its codec/packet PTS.
With the original defaults, all 20,821 presented f32 pairs match the failing
published MP4 bit for bit. This establishes that neither the chunked capture
nor project mixing explains the mismatch.

At sample 11,520, the existing 480-sample block has reference level -30.11 dBFS
and only 1.0882855 dB SNR. Its level difference is 0.4120 dB, so a level-only
check would miss the changed content. The full window's offset is verified zero.
At the same bitrate, disabling only PNS gives that block 20.9136229 dB SNR and
raises the minimum across all 33 measured blocks to 18.3042630 dB. Maximum
block level difference falls from 1.5285942 to 0.1521896 dB.

| Codec probe at 384 kbit/s | Minimum block SNR | Blocks below 1.5 dB |
| --- | ---: | ---: |
| Published movie / original defaults | 1.0883 dB | 1 |
| PNS disabled | 18.3043 dB | 0 |
| Mid/side disabled | 1.0883 dB | 1 |
| Temporal noise shaping disabled | 1.0883 dB | 1 |
| Intensity stereo disabled | 1.0883 dB | 1 |
| PNS and mid/side disabled | 18.3043 dB | 0 |
| Fast AAC coder | 22.2000 dB | 0 |

The implementation changes only PNS. The alternate coder and other options are
diagnostic comparisons, not selected policies. No PCM gain, normalization,
event alignment, packet removal or relaxed gate is involved.

## Regression and qualification

The fixed pre-codec input is retained in
[`native/deadpan-encode/tests/fixtures`](../../native/deadpan-encode/tests/fixtures/README.md).
The native regression fails against the old encoder at 1.0882855 dB and passes
the new encoder at 18.3042630 dB. It compares every 480-sample block at or above
the existing -60 dBFS floor, requires SNR at least 1.5 dB and level difference at
most 3 dB, and decodes the complete MP4 through manual and ordinary modes.
The existing opening/transcode and sub-block tests also pass, including sample
zero and the exact terminal sample.

All 65 native encoder tests pass. The deterministic OS software fixture remains
repeatable; disabling PNS intentionally changes its MP4 identity from
`12385:52c7852828da596a` to `12398:e5d085f427d9a92a`. The earlier identity and
failed pin check are retained. No pin bypass was used.

The following checks pass:

- All 65 native encoder tests in both debug and release, none ignored.
- 150 CLI render protocol/verification tests. Four explicitly ignored host
  video/color measurement harnesses were not run.
- All 16 shared job-admission tests, none ignored.
- All 31 release preview/export tests, none ignored. The complete recipe test
  constructed 42 fixtures, including 1080p and 2160p. Four optional accepted-
  generation fixtures were not built because this normal build excludes the
  synthetic worker; this run does not qualify generated-media workflows or AI.
- Strict workspace Clippy, including all targets and `ui-harness`.

The release app/CLI build and formatting pass. After all Cargo work ended, the
final executable verified the original failing movie as a failure, reproduced
all 20,821 canonical input pairs bit for bit, and rendered the same committed
project without changing its authored document. The new movie passes all 13
pictures and 33 audio blocks with zero measured offset, minimum SNR
18.3042629508 dB and maximum level difference 0.1521896472 dB. The same executable
also renders the retained nested-EQ project: all 120 pictures and four audio
windows pass, including 20 compared blocks, zero signal offset and 11.1211725900
dB minimum SNR.

The first complete export run exposed a second ABI allowlist in the shared job
validator. Its 27 export cases failed before publication with cleanup confirmed;
four non-export checks passed. The shared validator now accepts ABI 4 with the
same quality controls and timing as ABI 3, with structural regressions for both
versions, altered controls/priming and rejection of unknown ABI 5. The complete
export suite then passes all 31 tests. The failed first run is retained.

## Scope

The primary agent implemented and self-reviewed this change alone. No live GUI,
device playback or listening check ran. This is qualification on the current
M5 Max with the pinned FFmpeg build, not a claim for other hosts or codecs.
No DP requirement or release gate is newly complete.

[Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-aac-noise-substitution/README.md)
includes source/binary/library identities, the fixed input, codec comparisons,
old and new movies, projects, complete verification reports, successful checks
and the earlier failed runs. The checker verifies hashes and exact input/output
relationships; `--source` additionally checks the current source files.
