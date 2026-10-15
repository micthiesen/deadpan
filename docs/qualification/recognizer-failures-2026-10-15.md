# Recognizer failure check, 2026-10-15

Scope: the `deadpan-recognition-1` check of [local transcription](../TRANSCRIPTION.md#recognizer-failures)
and explicit re-recognition of a flagged passage (DP-10), measured on all 87
LibriSpeech test-clean chapters through the real headless CLI and the app's
analysis-PCM path. This calibrates the check on read English speech only.

## Summary

| Chapter | Failure (whole-file pass) | Flag | After `--again` |
| --- | --- | --- | --- |
| `4446-2275` (207.5 s) | from 109 s, "I didn't know when I was here in the morning." 61 times; WER 100.3% | loop, period 10, 61 copies, 109.1–207.5 s | WER 2.8%, nothing flagged |
| `5142-36600` (23.2 s) | only the heading; WER 90.6% | 19.5 s of speech after the last word | WER 3.1%, nothing flagged |
| `1284-134647` (118.1 s) | a 23 s sentence's 47 words timed into 69.0–76.0 s; WER 2.1% | 9.7 s of speech without words, 76.0–86.0 s | words retimed through 85.8 s, WER 2.4%, nothing flagged |
| 84 other chapters | none | none | not run |

No good chapter was flagged. In those 84 chapters the longest repetition spans
10 words (period 5, 2 copies, `2830-3980`), the most copies of any repetition
is 3 (single words), and the longest stretch of detected speech without a
recognized word is 2.07 s (`8555-292519`; median of the per-chapter maxima
0.93 s, next largest 1.64 s). The third flagged chapter is not in the
2026-10-05 record's failures: its words are correct but timed up to 10 s too
early, which matters for word motions and cuts.

## Environment and inputs

Apple M5 Max, 128 GB, macOS 26.5.2 (25F84). Source: `ac5dd61a` plus this
change only (a `git archive` copy, because concurrent sessions' uncommitted
work in the shared tree did not compile), built `--release --locked` with
`DEADPAN_FFMPEG_PREFIX=$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix`.
Final binaries: `deadpan-cli` SHA-256
`894354133841b215fb4a843a56da1322f22d59e91bdf606bb4941961b25f830a`,
`deadpan-transcribe` `c9d136a4652dc5c84cadbb69f2ac8a6f0f5173ac19d46c2ddccc7b8460a5a6ae`
(whisper.cpp 1.8.3, Metal). The 87 `transcribe` runs used an earlier
`deadpan-cli` build of the same worker and detector rule; every `transcript`
report below, and every `--again` run, used the final binary.

- Corpus: `https://www.openslr.org/resources/12/test-clean.tar.gz`,
  346,663,984 bytes, SHA-256
  `39fde525e59672dc6d1551919b1478f724438a95aa55f874b576be21967e6c23`
  (identical to the 2026-10-05 record), CC BY 4.0. Downloaded to a scratch
  directory outside the repository.
- Recognizer: the installed `whisper-base-en` pack's `ggml-base.en.bin`,
  147,964,211 bytes, SHA-256
  `a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002`
  (rehashed before use).
- Detector: `ggml-silero-v6.2.0.bin` from the pack manifest's pinned URL,
  885,098 bytes, SHA-256
  `2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`
  (verified after download), passed with `--vad-model`/`--vad-sha256`.

## Method

Each chapter was built as in the 2026-10-05 record: utterances decoded with
FFmpeg to PCM16 mono 16 kHz and joined in transcript order with 0.5 s of
digital silence. Unlike that record, the result became a real Original: AAC
(192 kb/s, mono, 48 kHz) under black 320x240 H.264 pictures at 25 fps, tagged
BT.709 SDR, in MP4. Per chapter:

```sh
deadpan-cli project create-original CH.deadpan CH.mp4
deadpan-cli transcribe CH.deadpan --model ggml-base.en.bin --sha256 a037… \
    --vad-model ggml-silero-v6.2.0.bin --vad-sha256 2aa2…
deadpan-cli transcript CH.deadpan
```

So decoding, the canonical speaker matrix, the exact-phase 16 kHz resampler,
speech detection and storage are the app's own. Worker time for all 87
chapters (5.76 h of audio) was 237.0 s recognition and 231.5 s detection.
The check itself takes milliseconds: `transcript` on `4446-2275` (916 words)
ran in 0.04 s including opening the project.

WER here uses lowercase, straight apostrophes, hyphen splitting and
punctuation removal, without the earlier record's digit and title expansion:
5.40% over all chapters (the earlier record's strict figure was 5.39%), 4.25%
without the three flagged ones. It identifies failures; it is not a new
accuracy result.

## Thresholds

- Loops: at least 4 whole copies and at least 24 words, periods up to 16
  words. Observed normal maxima were 3 copies and 10 words; the measured loop
  had 61 copies and 610 words. Four copies of a phrase of six or more words is
  well beyond read speech, and below the loop by more than an order of
  magnitude in both measures. Ordinary emphasis (`no no no no no no`, 6
  words) and a three-times refrain stay below it (unit tests).
- Speech without words: at least 5 s of detected speech in one stretch between
  recognized words. Normal stretches reached 2.07 s; the failures left 9.67 s
  and 19.47 s. 5 s is about the geometric mean of 2.07 and 9.67, a factor of
  about 2.4 above the largest normal stretch and 1.9 below the smallest failure.

Repetition is compared on normalized words, so punctuation and case differences
between copies do not hide a loop. In this app-path run only 30 of the loop's
610 words were approximate (median probability 0.97), so word confidence still
does not reveal it.

## Recognizing again

`deadpan-cli transcribe CH.deadpan --model … --sha256 … --again START:END`
with each chapter's reported `again_argument`:

| Chapter | Passage | Region used | Passes | Words replaced / recognized | Worker |
| --- | --- | --- | --- | --- | --- |
| `4446-2275` | 108.85–207.49 s | same | 1 | 606 / 261 | 0.86 s |
| `5142-36600` | 3.00–23.21 s | same | 1 | 1 / 57 | 0.38 s |
| `1284-134647` | 75.98–86.00 s | 69.00–86.00 s | 2 | 47 / 48 | 0.86 s |

For `1284-134647` the first pass over 75.98–86.00 s began "that all the
bishops of Europe…", words already present just before the passage. A merge
would have duplicated them (observed with an earlier build that lacked the
guard: "…portion of the that all the bishops…"). The guard detected the
repetition and ran one pass from the start of that sentence (69.00 s), which
spread its words over the actual speech and ended with an inserted `act.`
before the kept `African`. A second `--again` on each passage was refused
("is not a flagged passage"), and each transcript's `recognized_again` records
the region. Cropping the WAV directly and running the
`qualify_transcription` example gave the same recovered text, confirming that
a whole-file pass's context, not the audio, caused the failures.

## Other verification

- Unit tests in `deadpan-analysis` (loops at their shortest period, ordinary
  repetition, both loop thresholds, punctuation, trailing and interior
  speech, pauses, zero-length words, mismatched or disordered activity,
  passage merging and bounds, recognizing again once, stored provenance,
  repeated-words guard and sentence starts).
- The `transcript` replay saves a looping synthetic transcript through the
  real project service and checks the painted warning, passage link, the
  Recognize again offer, unchanged words and revision, and that clicking the
  passage moves the Original cursor to its first word's picture (55 checks
  passed).

## Limits

- Read audiobook English only. Conversational speech, singing, music and
  deliberate repetition (chants, choruses) could produce false loop flags or
  speech-without-words flags; none were measured.
- Loops whose copies differ slightly, loops shorter than 24 words, and
  truncations leaving under 5 s of speech are not flagged. Wrong words with
  plausible timing are not detected at all.
- Coverage needs speech activity of the same audio stream; without it only
  loops are checked and the report says so.
- Re-recognition uses the same model and decoding parameters on a shorter
  input. It succeeded on all three measured failures but is not guaranteed;
  words at the passage edges can differ, and a passage is recognized again at
  most once. The app's Recognize again link uses the same function but was not
  run with a real model here (replay never loads a model).
