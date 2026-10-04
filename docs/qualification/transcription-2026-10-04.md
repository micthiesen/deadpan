# Local transcription worker, 2026-10-04

Scope: the first transcription boundary in [local transcription](../TRANSCRIPTION.md).
This is not DP-10 completion; Original PCM preparation, storage, UI and
accuracy measurement remain open.

Environment: Apple M5 Max, macOS 26.5 (Darwin 25.5.0), Rust 1.97.1, whisper.cpp
1.8.3 vendored by whisper-rs-sys 0.15.0, whisper-rs 0.16.0 with Metal, debug
build of the worker.

Fixture: speech synthesized with macOS `say -v Samantha` ("The interview, made
weird. Absolutely. I think the answer is absolutely not. Let me say that again,
absolutely."), converted with `afconvert` to PCM16 mono 16 kHz: 7.28 s. Model
`ggml-base.en.bin`, SHA-256 `a03779c8…6d002`. Synthetic speech is not an
accuracy benchmark.

## Results

| Check | Observed |
| --- | --- |
| Supervised end-to-end run | Completed; runtime `whisper.cpp 1.8.3`, Metal; worker 946 ms including model hash verification and load, host 1,386 ms including workspace setup and admission. |
| Transcript | `the interview, made weird, absolutely, I think the answer is absolutely not, let me say that again, absolutely.` — 18 words, one below the 0.6 approximate threshold. |
| Phrase search | `absolutely not` → words `[10..12)`. |
| Progress | `0`, `100` (short input). |
| Cancellation | Cancel at 1,500 ms during a 291 s input; host returned `transcription cancelled` at 1,731 ms; no worker process remained. |
| Abort wrapper | whisper-rs `set_abort_callback_safe` caused `failed to encode` (error -6) on every run in a standalone probe; the raw-callback adapter fixed it. |
| Unit tests | `deadpan-analysis` 7 (including a property test over arbitrary recognizer output), protocol 4; `cargo nextest run -p deadpan-analysis -p deadpan-jobs -p deadpan-cli` passes 475. |

The recorded whisper tokens from the standalone probe are the `recorded()`
fixture in the transcript tests.

## Project run

A generic project created with `project create`, `project retain-original` and
`project register-source` holds a 320×180 H.264/AAC video of the same speech
(`ffmpeg` from Homebrew with explicit BT.709 bitstream colour description; a
first encode without it was correctly refused as an unqualified transfer).
`transcribe proj.deadpan --model ggml-base.en.bin --sha256 a03779c8… --asset
speech` decoded the AAC stream from retained bytes, prepared 7.28 s of analysis
PCM with the canonical resampler, transcribed 18 words (one approximate) in
961 ms inside the worker, 3.7 s in total in a debug build, and stored the result
under the Original's content identity. `transcript … --search "absolutely not"`
returned words `[10..12)` at 382–477 cs, Original audio samples 183,360–228,960
at 48 kHz.

`cargo nextest run -p deadpan-store --test transcripts`, `-p deadpan-cli --test
transcription` and `-p deadpan-transcribe` pass 3, 2 and 3 tests: storage
replacement, reopening, cap, read-only refusal and tamper detection; exact
analysis length, origin and an independently measured -79.2 dB mono level from
the B-frame fixture; command errors; and real-executable model hash, size,
load, missing-file and deadline failures.
