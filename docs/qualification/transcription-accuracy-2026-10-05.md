# Transcription accuracy on speech, 2026-10-05

Scope: word accuracy and word timing of the [local transcription](../TRANSCRIPTION.md)
worker on read human speech, one human speech excerpt and the synthesized
interview. This measures the recognizer boundary only. It is not DP-10
completion and does not qualify the app's analysis-PCM preparation, which these
runs replace with FFmpeg resampling.

## Summary

| Sample | Audio | Reference words | WER (normalized) | Approximate words |
| --- | --- | --- | --- | --- |
| LibriSpeech test-clean, all 87 chapters as long-form files | 5.40 h speech (5.76 h with gaps) | 52,576 | **5.08%** (S 1,857, D 250, I 566) | 4.2% |
| Same, excluding the two chapters with catastrophic failures | 5.35 h | 51,936 | **3.92%** (S 1,621, D 192, I 225) | |
| JFK inaugural excerpt (`jfk.wav`) | 11.0 s | 22 | 0.0% | 0 |
| Synthesized interview (`interview.deadpan` Original) | 18.0 s | 52 | 0.0% | 0 |

Two of 87 LibriSpeech chapters failed in ways that matter more than the average:
one fell into a repetition loop and lost the last half of the chapter, and one
stopped after its first sentence. Both are deterministic. Word timing is much
less accurate than word identity: at sentence boundaries the median start error
is about 300 ms and recognized sentence-final words end about 650 ms after the
speech does.

## Environment

Apple M5 Max, 128 GB, macOS 26.5.2 (Darwin 25.5.0), Rust 1.97.1. The build
was from `04d38fd3` plus other agents' uncommitted working-tree changes at the
time (including `deadpan-cli` and `deadpan-jobs` transcription host files);
the worker's recognition parameters are as described below. `deadpan-transcribe` and the `qualify_transcription` example were
built with `--release --locked`, `DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix`.
Runtime reported by the worker: `whisper.cpp 1.8.3`, backend `metal`.

Model: the installed approved pack at
`~/Library/Application Support/Deadpan/Models/whisper-base-en/1/ggml-base.en.bin`,
147,964,211 bytes, SHA-256
`a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002` (rehashed
before the runs). That installation is pack version 1 without the Silero
detector; the example does not use speech activity, so recognition is
identical. The worker uses greedy decoding (`best_of` 1), token timestamps, and
whisper.cpp's other defaults, including conditioning on previous text.

## Sources and references

| Sample | Source | License | Reference transcript |
| --- | --- | --- | --- |
| LibriSpeech test-clean | `https://www.openslr.org/resources/12/test-clean.tar.gz` (346,663,984 bytes, SHA-256 `39fde525e59672dc6d1551919b1478f724438a95aa55f874b576be21967e6c23`): 2,620 utterances, 40 speakers, LibriVox public-domain audiobook readings | CC BY 4.0 (corpus, V. Panayotov 2014); underlying LibriVox recordings public domain | The corpus's own `*.trans.txt` per chapter, produced by the corpus authors from the Project Gutenberg book text. Not written or edited for this record. |
| JFK excerpt | `https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/samples/jfk.wav` (352,078 bytes, SHA-256 `59dfb9a4acb36fe2a2affc14bacbee2920ff435cb13cc314a08c13f66ba7860e`), already PCM16 mono 16 kHz | Speech: 1961 US presidential inaugural address, a US government work in the public domain; file distributed with whisper.cpp (MIT) | Published address text: "And so, my fellow Americans: ask not what your country can do for you—ask what you can do for your country." The excerpt begins and ends on this sentence, so no alignment choice was needed. |
| Synthesized interview | Copy of `~/Documents/Deadpan/interview.deadpan/Media/Originals/blake3-d2f34ea3…db5d7` (copied, SHA-256 `fa4b19f7…6de8`; the package was not opened) | Generated locally | The exact `say -v Samantha` input recovered from the 2026-10-04 session that created the file: "Thanks for having me. So the question was whether the project would ship on time. Honestly, absolutely not. We tried everything. We rewrote the renderer twice, we replaced the audio engine, and then the deadline moved again. I think the answer is absolutely not, and I am comfortable saying that out loud." Its 19.83 s container duration matches that session's output. |

The interview is macOS text-to-speech, not human speech. Clean synthetic speech
is easy for the recognizer, so its 0% says little about real interviews. The JFK
excerpt is real human speech, but only 22 words. LibriSpeech is the main
measurement. No transcript in this record was made by listening; the measuring
agent could not hear the audio.

## Method

Each LibriSpeech chapter's utterances were decoded with FFmpeg to PCM16 mono
16 kHz and joined in transcript order into one WAV, with 0.5 s of digital
silence between utterances (87 files, 19 s to 8.9 min). One file per chapter
tests long-form recognition, which matches how the app transcribes a whole
Original. The interview audio was converted with
`ffmpeg -i interview-original.mp4 -vn -ac 1 -ar 16000 -c:a pcm_s16le interview.wav`.
Every file ran once through the real supervised worker:

```sh
target/release/examples/qualify_transcription MODEL SHA256 SPEECH.wav REPORT.json
```

Two failed chapters were run a second time and produced identical words.

WER is (S + D + I) / reference words from a minimum-edit word alignment.
Normalization: lowercase, curly apostrophes made straight, hyphens and dashes
split words, other punctuation removed, apostrophes inside words kept. The
normalized score also expands digit tokens to words (cardinals, ordinals, and
1100–1999 as years, so `1820` becomes `eighteen twenty`) and maps `mr`, `mrs`,
`dr` to `mister`, `missus`, `doctor`, because LibriSpeech references spell
these out. Without that expansion (strict), LibriSpeech WER is 5.39%. British
and American spellings, `st`/`saint`, and contractions are not normalized and
count as errors (for example `counselled`/`counseled`, `saint`/`st` 10 times).

### Timing

The reference for timing is energy, not a forced alignment. FFmpeg's
`silencedetect` (`n=-40dB:d=0.25`) found the silence covering each inserted
LibriSpeech gap. The reference's first word of each following utterance and
last word of each preceding one were mapped through the WER alignment to the
recognized word; only exact matches count (2,335 onsets and 2,354 offsets of
2,448 gaps; the two failed chapters are excluded). The error is the recognized
word start minus the energy onset, and the recognized word end minus the
energy offset. Because an energy threshold places an edge at a quiet consonant
or breath differently from a listener, errors of about 50 ms are within the
reference's own uncertainty; the errors below are much larger.

For the interview, the nine 180–220 ms pauses found at `n=-60dB:d=0.15` (the
same silences as in [speech activity](../SPEECH_ACTIVITY.md)) were matched in
order to the words that the script places on either side of them.

## Results

### LibriSpeech

| Measure | Value |
| --- | --- |
| WER, all 87 chapters (normalized / strict) | 5.08% / 5.39% |
| WER without the two failed chapters | 3.92% |
| Per-chapter WER, minimum / median / 90th percentile | 0.0% / 3.7% / 7.5% |
| Chapters below 5% | 65 of 87 |
| Worst chapters | `4446-2275` 100.3%, `5142-36600` 89.1%, then 8.6%, 8.4%, 8.0% |
| Recognized words | 52,527 |
| Approximate (probability < 0.6) | 2,183 (4.2%) |
| Zero-length words (start = end) | 2,042 (3.9%) |
| Accuracy of approximate words / confident words | 62.3% / 96.9% correct |
| Wrong words that were flagged approximate | 34.5% of 2,384 |
| Worker time for 20,719 s of audio | 144.3 s, real-time factor 0.0070 (each run includes model hashing and loading) |
| Host time including workspace setup and admission | 170.3 s, real-time factor 0.0082 |

The most frequent errors after the two failures are insertions from the
repetition loop (`i`, `didn't`, `know`, `when`, `here`, `morning`), `and`↔`in`
and `a`↔`the` confusions, dropped short function words (`to`, `and`, `a`) and
rare proper names (`poyser`→`poiser`, `boolooroo`→`bullaroo`,
`montfichet`→`montfiche`, `servadac`→`servedak`, `uncas`→`unkis`,
`phronsie`→`franzie`).

Failures:

- `4446-2275` (207.5 s): correct until the utterance "But why didn't you tell
  me when you were here in the summer" at 103 s. From about 107 s to the end
  the transcript repeats "I didn't know when I was here in the morning" 61
  times, so about 100 s of speech (236 reference words) is lost and 341
  words are invented. All repeated words have high probability (the last one
  0.99), so the approximate flag does not reveal the loop.
- `5142-36600` (23.2 s, 64 words): the transcript is only the heading "Chapter
  7 on the Races of Man", followed by nothing; the 20 s sentence after it is
  missing (57 deletions).

Both are known long-form whisper failure modes. Conditioning on previous text,
or a single long utterance after a short one, are likely causes, but neither
cause nor any fix (for example `no_context`, temperature fallback thresholds,
or segmenting by detected speech) was tested here.

Timing at utterance boundaries (`-40dB`):

| Edge | n | Signed median | Median abs. error | 90th percentile | Within 100 ms | Within 250 ms |
| --- | --- | --- | --- | --- | --- | --- |
| First word start − energy onset | 2,335 | +113 ms | 315 ms | 795 ms | 22% | 42% |
| Last word end − energy offset | 2,354 | +653 ms | 657 ms | 1,174 ms | 7% | 17% |

At `-35dB` and `-45dB` the onset median absolute error is 269 and 380 ms, and
the offset signed median +712 and +597 ms, so the conclusion does not depend on
the threshold. Spot checks show the pattern directly: in chapter `1089-134686`
the speech of "sauce" ends at 10.09 s but the word is reported at 10.74–11.23 s,
inside the next gap, and "Number" begins after a silence ending at 25.25 s but
is reported at 26.12 s. Sentence-final words routinely extend over the
following silence, and following words often start late. A nearest-word
comparison against every detected silence gives smaller numbers (median about
120 ms) but is biased, because it can choose whichever word happens to fall
closest to an edge; it is not used as a result.

### JFK excerpt

All 22 words correct, none approximate, one zero-length word (`for`). Worker
294 ms, host 696 ms for 11.0 s. The recording's reverberation and crowd noise
made the energy edges unreliable, so its timing was not scored.

### Synthesized interview

All 52 words correct, none approximate, three zero-length words (`me.`, `we`,
`I`). Worker 335 ms, host 704 ms for 18.0 s. `absolutely not` was found at
words `[16..18)` and `[42..44)`, at the same times as the 2026-10-04 native run.

| Pause (energy, s) | Previous word end error | Next word start error |
| --- | --- | --- |
| 1.18–1.41 | `me.` +177 ms | `So` +238 ms |
| 4.49–4.72 | `time,` +42 ms | `honestly,` +4 ms |
| 5.30–5.50 | `honestly,` +8 ms | `absolutely` +18 ms |
| 6.52–6.74 | `not.` +22 ms | `We` +146 ms |
| 7.80–8.03 | `everything,` +119 ms | `we` +159 ms |
| 9.73–9.93 | `twice,` −76 ms | `we` −77 ms |
| 11.54–11.74 | `engine,` −291 ms | `and` −163 ms |
| 13.35–13.58 | `again.` −205 ms | `I` +40 ms |
| 15.69–15.88 | `not,` −188 ms | `and` −253 ms |

Median absolute error is 146 ms for starts and 119 ms for ends; the largest is
291 ms. Even clean synthetic speech with near-silent pauses gets word edges
only to within one or two tenths of a second.

## What this means for Deadpan

- Word identity on clean read English is good enough to search and to navigate
  by words: about 4% WER when the recognizer does not fail, which is ordinary
  for `base.en`.
- Long-form failures are the main accuracy risk. In 2 of 87 files a sentence or
  half the recording was silently missing or replaced by a confident loop. The
  app shows no warning for either.
- Word timing is not frame-accurate. Word motions and `iw`/`aw` objects that cut
  at recognizer bounds can be several frames early or late, more so at sentence
  ends. Refining word edges from [speech activity](../SPEECH_ACTIVITY.md) and
  energy, which is already listed as remaining work, is needed before word cuts
  can be trusted without checking.
- The approximate flag catches about a third of wrong words, and about 38% of
  flagged words are wrong. It is a useful hint, not a reliable error marker.

## Limitations

- Read audiobook speech is cleaner and more regular than the interviews,
  overlapping talk, music and room noise that Deadpan targets. No
  conversational or noisy human speech with a published transcript was
  measured. The only spontaneous-style sample is synthetic.
- LibriSpeech is a widely used benchmark and may overlap the recognizer's
  training data, which would make these numbers optimistic.
- Audio went through FFmpeg resampling, not `prepare_original_audio`, and not
  through a project. The app path's decoding, stereo averaging and resampling
  are not covered.
- Inserted 0.5 s digital-silence gaps are artificial, and they make the
  utterance-boundary timing reference unusually clean. Timing inside sentences
  was not measured against any reference, because no forced alignment was
  available.
- Only English and only `ggml-base.en.bin` were tested. Each file ran once
  (twice for the two failures); no run-to-run variation was observed for those.
- Real-time factors are for M5 Max with Metal and are dominated by short files'
  fixed costs; they are not a measurement of the app's end-to-end transcription
  time.
