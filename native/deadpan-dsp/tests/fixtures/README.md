# Frozen canonical input

`mixed.f32` is the original synthetic 192,192-frame 48 kHz stereo input from
`tools/audio-qualification/probe.cpp::fixture()`. Samples are interleaved
little-endian float32; exact length is 1,537,536 bytes. It contains generated
tones, chirps, impulses, silence and level changes, with no user media.

Input SHA-256:
`80838601094aef41de8d08c40081baa302bc24fff0ac3b96dc7cc5fb98c0a224`.

These bytes were copied from the original qualification run's retained
`/tmp/deadpan-canonical-audio-_j497vww/pcm/input.f32` only after checking its length
and SHA-256 against `tools/audio-qualification/canonical-report.json`. That
committed source report has SHA-256
`959becabe6ca971cbcfcd52ab38f1cddd3f42f10799294292143611b64f0edc0`.
The temporary path is provenance, not a test dependency.

`../canonical-sha256.txt` retains that report's 50 `*-preview.f32` SHA-256 values:
15 mixed cases, 30 short impulses (31 and 1,003 frames), and five unity impulses
of 1, 2, 5,759, 5,760 and 5,761 frames. Columns are input frames, output frames,
integer pitch semitones, and output SHA-256. Output hashes cover interleaved
little-endian f32 samples. No hash was generated from the new Rust bridge.

The first Rust reconstruction of the mixed fixture produced input SHA-256
`d720d8b9563dc42bdc9fb038a596d21cdd3cab9078219539de4c332139c6399c`,
and the test stopped before comparing DSP output. Compiler/libm expression
differences are a possible cause; this was not diagnosed as a DSP failure.
Retaining the old input bytes removes that variable. The short impulse fixtures
use exactly representable authored samples and require no trigonometry.
