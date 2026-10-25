# Original salient-region fixtures

The source artwork and generators are original Deadpan test material, licensed
under this repository's MIT license. No person footage, downloaded image, font,
model output or scripted detector observation is included. CoreGraphics draws an
empty background, a blue mug, and a blue mug beside an orange. The PNGs retain
the source artwork. Detection assertions use the actual Apple Vision worker.

Regenerate from the repository with:

    python native/deadpan-track/tests/generate_salient_fixtures.py --check --work-dir /tmp/deadpan-salient-reproduction

The work directory must not already exist. It retains the original BGRA rasters,
PCM input, encoded outputs, and exact subprocess arguments and stderr. Byte
reproduction requires the recorded producer versions. salient-regions-fixtures.json
records the full producer build/license text, hashes and measured frame tables.
The producer was Homebrew FFmpeg **9.0.2**, including its GPL-enabled libx264 build.
It was not Deadpan's separately selected qualified LGPL FFmpeg **8.0.3** runtime.
Swift/CoreGraphics produces artwork and Python's wave module writes the WAV.

salient-regions.mkv and salient-regions-sar.mkv are video-only, lossless FFV1
in declared limited-range BT.709 yuv420p, 640 by 360, 120 pictures at nominal
30000/1001. Pictures 0 through 39 are blank, 40 through 79 show the mug, and 80
through 119 show the two objects. Measured Matroska time base is 1/1000:
section starts are 0, 1335 and 2669; last PTS is 3971 with duration 33, terminal
end 4004. The SAR variant declares 4/3, giving exact display aspect 64/27.
Output-side bitexact muxing removes random Matroska container metadata.

salient-regions.mp4 contains the same 120 pictures and sections as video-only
lossless H.264 in declared limited-range BT.709 yuv420p. Its exact CFR time base
is 1/30000, every PTS is 1001 times the ordinal, every duration is 1001, and the
terminal end is 120120 ticks (4.004 seconds). Section starts are 0, 40040 and
80080. This exact-clock variant supports default Original cadence admission.
The existing millisecond-clock MKVs remain explicit-profile detector/index inputs:
the native Original bootstrap rejected their quantized cadence as ambiguous.
That failed replay remains qualification evidence; admission was not relaxed.

salient-audio.wav is a separate original stereo PCM16LE, 48000 Hz, exactly
192192 samples per channel (4.004 seconds). Its quiet 440 Hz tone and sample
48000 through 48031 pulse are generated arithmetically. It is deliberately
separate from the video: an earlier PCM Matroska candidate failed qualified
container admission, and an Opus candidate failed complete stream qualification.
Those failed candidates and their tests were retained in the task's qualification
scratch, rather than changing source admission to accommodate a fixture.

The two short H.264 fixtures contain four lossless-encoded mug pictures. Their
time base is 1/30000, PTS 0, 1001, 2002, 3003 and terminal end 4004. One declares
a clockwise quarter turn, giving 9/16 upright aspect. The other uses the existing
independently checked aperture atom transformer with exact decoded pixel bounds
[left, top, width, height] = [641/4, 81/4, 959/2, 639/2], giving aspect 959/639.
Lossless encoding refers to the generated YUV pictures after BGRA conversion.

tests/picture_proposals.rs binds the exact fixture hashes, decodes every video
picture with the qualified product decoder, verifies all measured PTS/durations,
and reconstructs all WAV PCM bytes exactly. Its actual supervised requests prove
face-zero plus nonempty salient objects for the mug, successful empty results
for blank, two regions for the two-object picture, selected detector presence,
exact request revision 2 for saliency, and upright SAR/rotation/aperture geometry.
A positive drawn-face fixture separately verifies combined requests preserve
the unchanged face detector's default results. Tests compare coarse saliency
bounds with an explicit spatial tolerance; they do not claim exact object masks,
ranked importance, scores, arbitrary-object coverage, or a model benchmark.
The exact-CFR test additionally exercises default create-original admission and
the real three-section detector path without an explicitly authored profile.
