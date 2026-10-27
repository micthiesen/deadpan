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
probability, and words below 0.6 are presented as approximate. People correct
words and their edges by hand ([analysis corrections](ANALYSIS_CORRECTIONS.md));
[Local English refinement](WORD_REFINEMENT.md) now proposes reviewed timing
through the native phoneme pack; cuts and manual bounds stay pinned.

Word construction drops special tokens, starts a word at each token beginning
with a space, and attaches subword pieces and punctuation to the current word.
Punctuation extends display text but never the heard interval or confidence.
Bounds are clamped into their segment and the analysed duration and kept
monotonic, so recognizer output always produces a valid transcript or a typed
error. Stored transcripts are revalidated on deserialization.

`:select search="hello world"` uses corrected words as a semantic Edit selector.
It proves complete forward phrase coverage through the canonical provider
clock, preserving pauses and exact Original timing across continuous Source
splits. Its query survives recording and dot; copy and cut use separate ranges
in one atomic transaction. Proposal replacement and correction Apply/Undo/Redo
are independent of authored revisions, so editing carries an exact-key
proposal/correction guard checked inside Store publication. Native commands
also retain the displayed analysis identity and its absence. See
[transcript-search sets](SEMANTIC_MACROS.md#transcript-search-sets) for bounds,
unsupported authority and current verification status.

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

## Cursor-priority native analysis

Native transcription uses disjoint 24-second source ranges with 2.4 seconds
of model context on either side, prioritizing the latest visible cursor.
Completed ranges resume from the private `AnalysisRanges` cache shared with
speech-activity and shot analysis. Each record is addressed by BLAKE3 of its
exact serialized key; the key includes source, model, detector, worker,
settings, implementation and owned/context ranges, and the record payload is
checked with SHA-256. Partial rail words are labeled incomplete and show only
covered intervals; they support cursor jumps, while search, corrections and
editorial objects continue to use complete Workspace analysis. VAD and
Whisper share admission for each uncached range and release it between
ranges. The initial AudioSession opening still decodes the full stream. The
CLI transcribe command keeps its existing whole-input path. See the
[local qualification and remaining scope](qualification/incremental-ranges-2026-10-26.md).

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

### Accuracy on speech

On 2026-10-05, `ggml-base.en.bin` with this worker scored 5.08% word error rate
on all of LibriSpeech test-clean (5.4 h of human read speech, each chapter
transcribed as one long file), or 3.92% without two failed chapters. One of
those fell into a confident repetition loop and lost half the chapter; the
other stopped after its first sentence. A short JFK excerpt and the synthesized
interview were transcribed without errors. Word timing is much coarser than
word identity: sentence-initial words start a median 315 ms from the energy
onset, and sentence-final words end a median 650 ms after the speech stops.
The approximate flag marks about a third of wrong words. Conversational, noisy
and non-English speech remain unmeasured. See the
[accuracy record](qualification/transcription-accuracy-2026-10-05.md). The
same chapters through the app's own analysis-PCM path (AAC Originals,
`prepare_original_audio`) gave 5.40% with a simple normalization and the same
two failures, which the [recognizer failure check](#recognizer-failures) now
flags and can repair.

## Recognizer failures

`deadpan_analysis::check_recognition` applies the versioned rule
`deadpan-recognition-1` to a transcript and, when speech has been detected,
to the pauses of the same analysis PCM. It is derived on every read from the
stored transcript, speech activity and corrections, like pauses, so it is
never stored or stale. It reports regions only; it never drops, merges or
rewrites words.

- **Repetition loops.** Words are compared by their search normalization. A
  repetition is a maximal run in which each word equals the word one period
  later, for periods of 1 to 16 words, reported at its shortest period. A
  repetition of at least 4 whole copies spanning at least 24 words is a loop.
- **Speech without words.** Detected speech is the analysis clock outside the
  (corrected) pauses. Each stretch between recognized words, and before the
  first and after the last, holding at least 5 s of detected speech is
  flagged. This finds truncation (words ending before the speech does) and
  words timed far too early, which leave later speech uncovered. Zero-length
  words still divide stretches. Without speech activity, or with activity of
  different audio, this part is reported as unchecked with the reason.
- **Passages.** Each flag becomes a passage: a loop from the end of the word
  before it to the start of the word after it, unrecognized speech as its own
  stretch, overlapping regions merged.

Thresholds come from all 87 LibriSpeech test-clean chapters through the real
CLI: in the 84 good chapters the longest repetition was 10 words (3 copies at
most) and the longest stretch of speech without words 2.07 s; the failures
were a 61-copy loop of a 10-word phrase, 19.5 s of speech after the last word
and 9.7 s left uncovered by a sentence whose 47 words were timed into its
first 7 s. All three were flagged and no good chapter was. See the
[qualification record](qualification/recognizer-failures-2026-10-15.md).

### Recognizing a passage again

A flagged passage can be recognized again, explicitly, once.
`transcribe <project> --again <start_cs>:<end_cs>` takes a passage from the
`transcript` report (`again_argument`), refuses anything that is not a
currently flagged passage, one already recognized again, longer than 20
minutes, or a model other than the stored transcript's, then runs the worker
on just that passage's analysis PCM with the stored transcript's model and
language. Words beginning in the passage are replaced by the second pass's
words; every other word is kept, sentences are renumbered, and the passage is
recorded in the transcript's `recognized_again` list (a transcript JSON field
omitted when empty), which the check reports and which prevents another
attempt. When the second pass begins by repeating words the transcript already
has just before the passage (its first 4 words occur among the previous 64),
the earlier words were timed too early rather than missed. Exactly one more
pass then runs from the start of that earlier sentence through the passage and
replaces both; if no earlier sentence start exists nothing is saved. The
whisper.cpp pass hears only the passage, so it is not conditioned on the text
the whole-file pass produced. On the three failures this brought chapter WER
from 100.3%, 90.6% and 2.1% (mistimed) to 2.8%, 3.1% and 2.4%, with none
flagged afterwards. Words at the passage's edges can still differ: the
retimed sentence ended with an inserted `act.` before the kept `African`.

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
transcripts, or phrase matches with exact Original sample bounds. Both forms,
and the `transcribe` report, include `recognition`: the rule, whether anything
was flagged, the loops, unrecognized stretches and passages with exact
Original sample bounds and display seconds, the longest repetition and
stretch even when unflagged, and passages already recognized again.
`transcribe <project> --again <start_cs>:<end_cs>` recognizes one flagged
passage again ([above](#recognizing-a-passage-again)).

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

Below the word caption, flagged passages appear as a warning ("Recognition may
have failed in N passages. The words are kept as recognized…") with up to three
passage links, each moving the Original cursor to where the passage begins,
and a Recognize again link for passages that allow it. Recognizing again runs
as a Transcription job (it waits for the AI model slot), shows its progress
and failures in the section, and saves through the same service request as a
new transcript, refusing a result whose base transcript was replaced
meanwhile. Without detected speech the section says missing words are not yet
checked. The check is recomputed only when the published words or pauses
change. The `transcript` replay saves a looping synthetic transcript and checks
the warning, passage link and offer; it cannot run the recognizer.

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

## Manual correction

`:correct` opens a sheet where words are edited, split (a space in the text),
joined, removed and their edges moved by 10 ms or to measured energy edges.
Corrections are stored apart from the recognizer's transcript, keyed by the
Original and its audio stream rather than the model, so transcribing again
keeps them; corrected words win over any recognized word they overlap. They
have their own Undo and Redo and never create an edit revision. Display,
search, word motions and objects, and macros read the corrected words. See
[analysis corrections](ANALYSIS_CORRECTIONS.md).

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

System sleep interrupts initial recognition and Recognize again. A bounded
watcher signals worker cancellation even without UI or progress callbacks;
wake never resumes the old attempt. The rail explains the interruption and
offers Try again after the worker drains. Existing words and corrections remain
available when Recognize again is interrupted. Late results and queued saves
retain the run's lifecycle epoch: transcript and accompanying speech-activity
transactions reject an invalidated epoch before writing and before commit.
A save admitted at the final guard remains valid if sleep follows it.
See [qualification](qualification/transcription-sleep-2026-10-17.md) for the real
Whisper cancellation test and publication regressions. Standalone speech
activity now applies the same sleep policy through its own run capture;
see [standalone detection qualification](qualification/activity-sleep-2026-10-17.md).

Known limits: the host and worker each hold analysis PCM twice while
transferring it (about 1.4 GB at the three-hour cap); the worker hashes the
model and whisper.cpp then opens it by path; a script without spaces between
words (for example CJK with a multilingual model, reachable only through the
CLI) can exceed the word length and fail the whole transcript; preparation of
very long Originals is unmeasured and reports no progress; search normalizes
every word per query.

## Remaining

Original operators with word motions, background scheduling by visible range,
broader sentence correction and accuracy on conversational and noisy speech.
The local English phoneme pass is implemented as an explicit approximate
proposal; its stricter experimental quality targets remain failed. The
recognizer failure check is calibrated only on read English audiobook speech:
loops with small variations between copies, failures shorter than 5 s of
speech, wrong words with plausible timing, and false flags on music, singing
or deliberate repetition in other material are unmeasured. Recognizing again
uses the same decoding parameters on a shorter input; it is not a temperature
fallback or a different model.
The model-pack manager lists every approved pack, including this one
(`approved_packs()` in `crates/deadpan-models/src/packs.rs`, listed by
`crates/deadpan-app/src/model_packs.rs`).
