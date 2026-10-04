# Speech activity and pauses

Deadpan detects where the Original's speaker pauses with the Silero voice
activity detector run by whisper.cpp, refined by energy measured from the same
audio. Section 11 of the [specification](spec/DEADPAN_SPEC.md) requires
silence intervals derived from VAD plus measured energy; this records the
implemented boundary. Pauses are proposals: they never edit the project.
[Shot detection](SHOT_DETECTION.md) is the picture-side analysis, stored and
published the same way.

## Pieces

| Piece | Responsibility |
| --- | --- |
| [`deadpan_analysis::activity`](../crates/deadpan-analysis/src/activity.rs) | Pure `SpeechActivity`: quantized probabilities (one per 512 analysis samples) and 10 ms energies; the `deadpan-silence-1` pause rule; exact Original timing. |
| [`deadpan_jobs::transcription`](../crates/deadpan-jobs/src/transcription.rs) | Protocol 2 adds `DetectSpeech` and its `SpeechDetected` completion beside `Transcribe`. |
| [`deadpan-transcribe`](../native/deadpan-transcribe/) | The same isolated worker runs `WhisperVadContext::detect_speech` (whisper.cpp 1.8.3, CPU). |
| [`deadpan_cli::activity`](../crates/deadpan-cli/src/activity.rs) | Host attempt, `stored_activity`, `pauses` command. |
| [`deadpan-store`](../crates/deadpan-store/src/speech_activity.rs) | Database schema 60 `speech_activity` table. |

## Detection

The host reuses the [analysis PCM](TRANSCRIPTION.md#analysis-audio) prepared for
transcription: mono 16 kHz beginning at a known Original audio sample. It sends
`DetectSpeech` with the PCM artifact, the verified Silero model and a byte
budget of exactly `ceil(samples / 512) · 4`. The worker opens and hashes both
inputs exactly as for transcription, runs detection, requires
`ceil(samples / 512)` finite probabilities in `0..=1`, and writes them as
little-endian `f32` to `output/activity.f32`, created exclusively. The
protocol adapter admits only the completion that matches the request kind
(`Completed` for `Transcribe`, `SpeechDetected` for `DetectSpeech`), within the
output scope and budget, from the requested model hash. The host then requires
the exact length again after clean teardown and a hashed snapshot, and builds
`SpeechActivity::measure` from the probabilities and the same PCM.

Detection resets the detector's state for each call and processes 32 ms hops
sequentially; it is not cancellable mid-run, so cancellation is checked before
and after it. On the 18 s interview it took 177 ms in the worker, including
model hashing and loading. whisper.cpp stops at a failed hop but still reports
success; the range check above is the only guard against such output.

## Storage

Database schema 60 adds `speech_activity`, keyed by Original content identity,
audio stream, model SHA-256 and engine. It stores the analysis origin, source
sample rate, analysis length and both quantized byte vectors as BLOBs. Saving
replaces the row for its key, never creates a revision or Undo step, and is
limited to 32 rows per project. Every read revalidates through
`SpeechActivity::new`; `stored_activity` prefers an approved pack's detector and
skips an unreadable row. Schema 59 packages are upgraded in place by their next
writer (see [development formats](DEVELOPMENT_FORMATS.md)).

## Commands

`transcribe` also detects speech, on the same prepared PCM, when the installed
pack provides the detector or `--vad-model <ggml-silero.bin> --vad-sha256 <hex>`
is given. Its report adds `speech_activity` with the key, rule, worker time and
pause count. `pauses <project> [--asset <id>]` prints the stored activity's
pauses under `deadpan-silence-1`: analysis sample bounds, exact Original sample
bounds (`origin + k · rate / 16000` as a rational) and display seconds.

## In the app

The automatic transcription job prepares PCM once, detects speech, then
transcribes. The validated activity is submitted to the project service, which
saves it outside history and publishes it as `Workspace::speech_activity`; it
is loaded when a project opens and carried across edits. An Original that
already has a transcript but no activity gets a detection-only job. Detection,
saving failures (with Try again) and a missing detector appear below the
TRANSCRIPT section. With only the version 1 pack installed, the section offers
Update model…; installing version 2 copies the identical recognizer file from
the installed version and downloads only the 885,098-byte detector.

## Pauses as motions and objects

`]p` and `[p` move to the start of the next or previous pause, in Original and
Your edit; counts move further and a count past the last pause stops there.
With no pause in that direction nothing moves and the footer says so. `ip`
selects the pause at the Edit cursor in Visual mode and `ap` adds a little of
the speech on each side (half of it, at most 80 ms, like the word handles).
Both compose after `d`, `y` and `r` (`d]p`, `dip`, `3rap`), and macros and
dot-repeat record them as semantic motions and objects.

Pauses reach the Edit clock by the same projection as words
([`deadpan_cli::speech`](../crates/deadpan-cli/src/speech.rs)). A frame is quiet
when it presents an Original picture lying wholly inside a detected pause, or a
freeze, generated picture, blank or background, which carry no Original speech.
Consecutive quiet frames form one pause, so a silent Hold inserted after a
sentence lengthens the pause there, and cutting part of a pause shortens it.
Other assets' pictures and stills are never quiet. In Original, picture `k` is
quiet when it lies wholly inside a pause.

Because a picture must lie wholly inside a pause, a pause shorter than about
two picture durations can hold no whole picture and so does not appear on a
picture clock (at 24 fps, a 150–170 ms pause straddling three pictures). A
Hold counts as quiet even when it freezes a picture mid-word, so `dip` on such
a Hold removes the Hold. As with words, a pause clipped by the current group
starts at the group edge.

Bracket keys need Option or AltGr on some non-US layouts; `pause.next` and
`pause.previous` can be remapped in the [keymap](KEYMAP.md). Physical non-US
delivery of the default paths is not yet qualified.

Words and pauses are independent analyses: the core `SpeechTimeline` carries
each with its own reason when it is missing, so pause keys work before a
transcript exists and word keys explain themselves without detection.

## Real speech, 2026-10-04

A copy of the 18 s `interview.deadpan` project (48 kHz, synthesized interview
speech) was transcribed with `ggml-base.en.bin` and `ggml-silero-v6.2.0.bin`.
Silero's quiet runs (probability below 0.35 after speech) matched every
between-phrase silence that the 10 ms energies show below about -75 dBFS:

| Energy silence (s) | Quiet hops (s) | Reported pause (s) |
| --- | --- | --- |
| 1.19–1.41 | 1.248–1.440 | 1.22–1.41 |
| 4.49–4.71 | 4.544–4.736 | 4.51–4.71 |
| 5.31–5.50 | 5.344–5.504 | 5.33–5.50 |
| 6.52–6.74 | 6.560–6.752 | 6.56–6.74 |
| 7.81–8.03 | 7.840–8.032 | 7.82–8.02 |
| 9.73–9.92 | 9.760–9.952 | 9.75–9.92 |
| 11.55–11.74 | 11.584–11.744 | 11.57–11.74 |
| 13.36–13.58 | 13.408–13.600 | 13.39–13.57 |
| 15.69–15.88 | 15.744–15.904 | 15.71–15.88 |

The speaker's pauses are 180–220 ms. Silero's quiet run starts about 30–50 ms
after the energy drop, so a first version of the rule (200 ms minimum, edges
advancing at most 30 ms) reported only the 7.82–8.02 s pause. The current rule
lets an edge advance 60 ms over near-floor frames and accepts pauses from
150 ms; it reports all nine, each within 10–30 ms of the energy silence. The
transcript's word gaps (for example 5.25–5.61 and 6.55–6.89) are wider and less
precise than both.

## Remaining

Pause display in the rail, a rule qualified on varied real recordings (one
synthesized speaker so far), room-tone and breath classification,
and detection progress for very long Originals.
