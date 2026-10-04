# Local transcription

Deadpan transcribes the Original locally with whisper.cpp. A transcript is an
analysis annotation: it proposes where words were heard and never edits the
project. Section 11 of the [specification](spec/DEADPAN_SPEC.md) is normative;
this records the implemented boundary. The same worker and analysis PCM also
detect [speech activity and pauses](SPEECH_ACTIVITY.md).

## Pieces

| Piece | Responsibility |
| --- | --- |
| [`deadpan-analysis`](../crates/deadpan-analysis/) | Pure `Transcript` type: recognizer tokens become validated words with display text, half-open centisecond bounds, lowest-token probability and segment; exact Original timing; phrase search. |
| [`deadpan_jobs::transcription`](../crates/deadpan-jobs/src/transcription.rs) | Versioned, strictly framed worker protocol and the `TranscriptionProtocol` adapter for the shared `SupervisedProcess`. |
| [`deadpan-transcribe`](../native/deadpan-transcribe/) | Process-isolated worker executable: whisper.cpp 1.8.3 with Metal through `whisper-rs` 0.16. |
| [`deadpan_cli::transcription`](../crates/deadpan-cli/src/transcription.rs) | Host attempt: workspace, analysis PCM, supervision, artifact snapshot, validation. |

## Timing

The recognizer consumes mono 16 kHz PCM beginning at a known Original audio
sample and reports token bounds in centiseconds. A centisecond `t` maps to
Original audio sample `origin + t · rate / 100`, an exact rational with no
accumulated rounding. Recognizer timing itself is approximate: whisper.cpp
describes word timestamps as experimental. Each word keeps its lowest token
probability, and words below 0.6 are presented as approximate. Refinement and
manual correction remain required.

Word construction drops special tokens, starts a word at each token beginning
with a space, and attaches subword pieces and punctuation to the current word.
Punctuation extends display text but never the heard interval or confidence.
Bounds are clamped into their segment and the analysed duration and kept
monotonic, so recognizer output always produces a valid transcript or a typed
error. Stored transcripts are revalidated on deserialization.

## Analysis audio

`prepare_original_audio` decodes the Original's qualified audio stream from its
verified retained bytes, mixes it with the canonical speaker matrix, resamples
it with the project's exact-phase resampler to 16 kHz, and averages left and
right. Analysis sample `k` is source sample `origin + k · rate / 16000`, where
`origin` is the first measured valid sample. Coverage gaps fail rather than
being filled. An explicit registered asset can be analysed instead of the
single-Original default. Averaging cancels opposite-polarity channels: the
B-frame fixture measures -79.2 dB mono against -55.7/-56.9 dB per channel in
both FFmpeg and Deadpan. Choosing a channel for such recordings remains open.

## Worker protocol

The host creates a fresh attempt workspace with `input/` and `output/`, writes
the analysis PCM as little-endian `f32` to `input/analysis.f32`, and sends one
protocol-2 `Transcribe` message with its hash and length, the verified model's absolute
path, SHA-256 and length, the language (`auto` or a two-letter code), the output
scope, a transcript byte budget and a timeout. The worker opens both files
without following symbolic links, checks exact length and SHA-256, rejects
non-finite samples, runs recognition with token timestamps, and writes raw
segments to `output/transcript.json` created exclusively. Messages are bounded
by the shared 256 KiB frame limit; the transcript travels only as a hashed
artifact up to 64 MiB. Analysis audio is limited to three hours.

Progress reports a monotonic percentage. `Completed` must name an artifact
below the output scope, within budget, produced by the requested model hash.
The host admits it only after clean process teardown, a contained hashed
snapshot, bounded parsing and full `Transcript` validation. Cancellation sends
`Cancel`; the worker stops recognition through whisper.cpp's abort callback.

whisper-rs 0.16's `set_abort_callback_safe` instantiates its trampoline for the
closure type while storing a boxed trait object, so every encode aborted. The
worker installs a plain callback over a process-wide flag in a narrowly scoped
`unsafe` adapter instead. The worker's environment is empty; Metal works there
(its optional tensor-API probe logs a compile error with or without an
environment and is disabled).

## Model

The approved `whisper-base-en` pack (version 2, see [model packs](MODEL_PACKS.md))
provides `ggml-base.en.bin` from the whisper.cpp model repository: 147,964,211
bytes, SHA-256 `a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002`,
MIT licensed like whisper.cpp, and the Silero detector for
[speech activity](SPEECH_ACTIVITY.md). Weights are never committed.

## Qualification

`cargo run -p deadpan-cli --example qualify_transcription -- MODEL SHA SPEECH.wav
REPORT.json [phrase]` runs the real worker on a PCM16 mono 16 kHz WAV.
`DEADPAN_QUALIFY_CANCEL_MS` cancels instead. See the
[qualification record](qualification/transcription-2026-10-04.md).

## Storage and headless commands

Database schema 59 adds a `transcripts` table keyed by Original content
identity, audio stream, model SHA-256, language and engine. Saving replaces the
transcript for its key, never creates a revision or Undo step, and is limited to
32 transcripts per project; every read revalidates the stored transcript.

`transcribe <project> --model <ggml.bin> --sha256 <hex> [--vad-model
<ggml-silero.bin> --vad-sha256 <hex>] [--language <auto|xx>] [--asset <id>]`
prepares analysis PCM, runs the worker installed beside the executable and
stores the result, with speech activity of the same PCM when a detector is
installed or given; it needs the project's writer to save.
`transcript <project> [--search <words>] [--asset <id>]` prints stored
transcripts, or phrase matches with exact Original sample bounds.

## In the app

When a single-Original project's Original is ready and the approved pack is
installed, the app prepares analysis PCM from a read-only store connection on
a background thread, runs the worker beside its executable and submits the
validated transcript to the project service, which saves it outside history
and publishes it with the workspace. The workspace loads the stored transcript
when a project opens and carries it across edits without reloading.

Without the pack, the TRANSCRIPT section at the end of the Original rail states
the model's size and license and offers Install model…, which downloads,
verifies, smoke-tests and activates it in the background with progress and
cancel. Transcription progress, saving and failures (with Try again) appear in
the same section.

A ready transcript shows the sentences around the current word, at most four on
each side, as wrapped text runs. The current word (the latest word begun by the
end of the Original cursor's picture, compared as exact rationals) is
highlighted; approximate words are grey italics; search matches are
underlined. Clicking a word or pressing Enter (Shift+Enter for previous) in Find
words moves the Original cursor to the picture presented when that word begins.
Replay uses an empty model directory, so it never downloads or uses an
installed model; the `transcript` scenario saves a synthetic transcript through
the real service and checks display, search, current word and exact jumps.

## Words as motions and objects

`w`, `b` and `e` move to the next word start, the previous word start and the
next word end; `W` and `B` move between sentences (transcript segments). In
Your edit, `iw`, `aw`, `is` and `as` select a word or sentence in Visual mode,
and every motion and object composes after `d`, `y` and `r` (`dw`, `d3e`,
`diw`, `3riw`, `yas`). Counts on motions move further; a count before `r` is
total plays. Macros and dot-repeat record the motions and objects, which
resolve anew against each staged document.

Words reach the Edit clock by projection, never by stored edit positions. The
host compiles the render plan of the document being edited and assigns each
project frame that presents an Original picture the word being spoken during
that picture: the latest word that begins before the picture ends and has not
ended when it starts. Consecutive frames of one word in one occurrence form a
run; a sentence occurrence is consecutive runs of one segment whose words
advance, so a replayed sentence is a second occurrence. Freezes, generated
pictures, stills and gaps carry no words. Speech follows the picture mapping of
linked beats, so a picture-only cutaway reports the cutaway's words.

The core planner (`plan_semantic_with_speech`) asks for this projection at most
once per staged document and only when an instruction needs words, so a macro
that cuts a word and then moves by words sees the edited arrangement. The
around objects add half of each adjoining pause, at most 80 ms per side
(rounded down to whole project frames) and never reaching neighboring speech.
`iw` in a pause reports that the cursor is not on a word. Without a transcript,
word keys explain why words are not ready (model missing, transcribing, failed)
and frame and beat editing continue.

Runs continue across beat boundaries while the source picture does not go
back, so splitting or cutting inside a word keeps one occurrence and a replayed
word starts another. As in Vim, a motion past the last word clamps to the
group edge, so `dw` on the last word cuts to the end of the group, and a
sentence object spans whatever lies between its words, including an inserted
cutaway. `iw` and the other objects replace a Visual range rather than extend
it.

Projection samples one picture per project frame. In a release build on
2026-10-04 it cost 0.5 µs per frame on the 595-frame interview project (about
54 ms for an hour at 30 fps), paid once per revision at the first word key and
once per staged document in a plan. Deep arrangements cost more per picture.

In Original, `w`, `b`, `e`, `W` and `B` move the Original cursor over the
Original's own pictures with the same rule. Original operators with motions
are not yet available; `v` with word motions selects a moment to copy.

## Your edit in the rail

While Your edit is in view, the TRANSCRIPT section lists the edit's words in
playback order around the Edit cursor, from the same projection the motions
use: cut words are absent, repeated words appear at each play, and the caption
counts words kept and plays. Clicking a word moves the Edit cursor to where that
occurrence begins. In Original the rail shows the Original's sentences.

## Search keys

With a transcript, `/` focuses Find words in the Original rail without typing
the slash. Enter and Shift+Enter in the field, and `n` and `N` elsewhere, move
to the next or previous match after the cursor in the current context: in
Original to the picture where the matching word begins, in Your edit to each
occurrence of a matching word in the arrangement, so a repeated or cut word is
found where it actually plays. Both wrap at the ends and report the position.
Without a transcript, `/` finds a sound as before.

## Failure handling

A finished transcript is submitted to the project service without the side
effects of a user command (playback, repeats and pending keys are untouched);
a busy service is retried on a later frame. The service reports each save by
session and attempt, so a rejected or failed save shows as a failure with Try
again. A stored transcript that fails validation is skipped on open, and the
app transcribes again and replaces it. The worker treats end of input from its
host as cancellation, and the app cancels and briefly joins its job thread on
quit, so recognition never continues unowned. An unrequested `cancelled` reply
is a protocol failure.

Known limits: the host and worker each hold analysis PCM twice while
transferring it (about 1.4 GB at the three-hour cap); the worker hashes the
model and whisper.cpp then opens it by path; a script without spaces between
words (for example CJK with a multilingual model, reachable only through the
CLI) can exceed the word length and fail the whole transcript; preparation of
very long Originals is unmeasured and reports no progress; search normalizes
every word per query.

## Remaining

Original operators with word motions, background scheduling by visible range,
word-boundary refinement from speech activity, manual correction, accuracy measurement on real speech, and
a manager listing every pack.
