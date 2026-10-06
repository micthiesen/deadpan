# Analysis corrections

Recognized words and detected pauses are rebuildable proposals
([transcription](TRANSCRIPTION.md), [speech activity](SPEECH_ACTIVITY.md)).
Section 19.2 of the [specification](spec/DEADPAN_SPEC.md) requires manual
corrections to be kept separately from them, and Section 11.2 that manually
corrected timing stays pinned and that silence intervals can be edited by
hand. This records the implemented boundary.

## Pieces

| Piece | Responsibility |
| --- | --- |
| [`deadpan_analysis::corrections`](../crates/deadpan-analysis/src/corrections.rs) | Pure `Corrections`: corrected word and pause regions, the `deadpan-corrections-1` overlay rule, the operations (edit text, split, join, remove, move edges, add, remove and resize pauses) and `deadpan-edges-1` measured edges. |
| [`deadpan-store`](../crates/deadpan-store/src/analysis_corrections.rs) | Database schema 66 `analysis_corrections` and `analysis_correction_steps`: the current value, a version and bounded Undo/Redo stacks. |
| [`deadpan_cli::speech`](../crates/deadpan-cli/src/speech.rs) | `stored_words`, `stored_transcript` and `corrected_pauses`: the corrected analyses every consumer reads. |
| [Project service](../crates/deadpan-app/src/project/service/transcripts.rs) | `ProjectRequest::ChangeCorrections`: one versioned change, published with the corrected transcript and pauses. |
| [`preview::corrections`](../crates/deadpan-app/src/preview/corrections.rs) | The `:correct` sheet. |

## Overlay rule

Corrections are regions of the analysis clock (the Original audio sample where
the analysis PCM begins, and the Original's audio sample rate). A word region
`[start, end)` in centiseconds owns its words; a pause region in 16 kHz
analysis samples owns the pause time inside it. Applying them to a proposal
(`deadpan-corrections-1`):

1. A recognized word conflicts with a word region when it overlaps it; a
   zero-length word occupies its one centisecond. Every conflicting recognized
   word is replaced by the region's words. After a new transcription, a
   recognized word that straddles a corrected edge is therefore dropped and
   counted as replaced: the person's words win over the whole region, and the
   recognizer's words outside it are used unchanged.
2. A corrected word takes the sentence (segment) of the first recognized word
   its region replaced, or else of the word before it, and probability 1.
3. Detected pauses are clipped to outside every pause region; a clipped
   remainder shorter than the rule's shortest pause (150 ms) is dropped. The
   regions' pauses are added and touching pauses merge, so a detected pause
   that continues past a corrected region joins the corrected pause.
4. Corrections apply only to analyses with the same analysis clock. A region
   beyond the analysed audio, or every region of another clock, is skipped and
   counted; nothing is reinterpreted.

Every operation is computed against the corrected analysis a person sees and
stored as one region: the hull of the changed items, grown until no other
region or visible item straddles it. Items it swallows are pinned. A property
test checks that reapplying the stored corrections to the recognized words
reproduces the edited words. Corrections are keyed by the Original's content
identity and audio stream only, never by model, so they survive transcribing
or detecting again with another model.

Operations: editing a word's text (empty text removes it; several
whitespace-separated words split its time in proportion to their letters,
each split moved to the nearest measured edge within 80 ms inside the word);
joining a word with the next (text concatenated without a space, both words'
time); moving a word's edges (each edge stops at the neighbour's far edge and
shortens the neighbour instead of overlapping it); removing a pause; adding a
pause (joining pauses it overlaps or touches); and moving a pause's edges
(stopping at its neighbours; touching joins).

## Measured edges

`deadpan-edges-1`: every boundary between 10 ms energy frames of the stored
[speech activity](SPEECH_ACTIVITY.md) where the energy changes by at least
9 dB, plus every detected or corrected pause edge. One energy frame is exactly
one centisecond of the same analysis PCM, so edges convert exactly to word
centiseconds when the transcript and activity share the analysis clock;
otherwise words have no edges to snap to.

## Storage and Undo

Database schema 66 adds `analysis_corrections` (content, audio stream,
version, value) and `analysis_correction_steps` (Undo and Redo stacks of
complete earlier values with labels, at most 64 each). Corrections are
annotations, not edits: changing them never creates a document revision or an
edit-history step, and edit Undo never changes them. They have their own Undo
and Redo, durable across reopening. Every change names the version it expects
and is refused without writing when the stored version differs, so a stale
view cannot overwrite a newer correction. A new change clears Redo. Values are
revalidated on every read, and validation rechecks sizes and stack bounds; at
most 16 corrected audio streams and 8 MiB per value are kept.

Explicit validation (`validate`, `project migrate`) parses every stored value,
current, Undo and Redo, and fails on any unreadable one. Opening a project
tolerates them so they can be repaired: the workspace carries the read error,
including an unreadable Undo or Redo step, and the change is refused until
the person discards them (Shift+D in the sheet,
`CorrectionChange::DiscardUnreadable`). Discarding removes only unreadable
values; an unreadable current value becomes no corrections, and readable Undo
steps remain, so Undo still reaches the last readable state.

Corrections are never silently replaced by the proposal. When they are
unreadable, or some regions no longer apply (another analysis clock after a
new transcription, or beyond the analysed audio), the rail and the sheet show
why, and word or pause operators, motions, objects and macros (app and
headless) refuse with that reason rather than using uncorrected words. Shift+D
then drops the regions that no longer apply, as one undoable correction.

Every change carries the transcript and activity the person saw, by identity;
the service refuses a change computed against an analysis that has since been
replaced, and reports a failed read-back instead of clearing the transcript.

Why not authored history: Section 11.1 says analysis services never change an
edit, and corrections describe the Original's audio, not the arrangement.
Keeping them out of the document means a correction never invalidates render
fragments, AI requests or edit Undo positions. Words reach the Edit clock by
projection, so a correction changes word motions and objects in every
revision at once.

## Consumers

`stored_transcript` (macros and headless commands), the app workspace's
`OriginalTranscript.transcript` (rail, search, `w b e W B`, `iw aw is as`,
`/` and `n N`) and its `OriginalActivity.pauses` (`]p [p ip ap`, pause bands)
are the corrected analyses. `OriginalTranscript.proposal` keeps the
recognizer's words. The app's speech projection cache is keyed by the
corrections version as well as the analysis keys. `transcript` prints the
corrected words with `corrected_words` and `replaced_recognized_words`;
`pauses` marks each pause `corrected` and reports `detected_pauses`.

## In the app

`:correct`, or Correct words and pauses… below the rail transcript, opens a
modal sheet with the words and pauses around the current word in time order;
pauses show as `‖ 0.21 s`. Corrected words carry a thin lavender underline,
here and in the rail. The sheet owns its keys (none reach the editor):

| Keys | Action |
| --- | --- |
| `h` `l`, ← → | Previous / next word or pause |
| `c`, Enter | Edit the word's text in a field (a space splits; empty removes); Enter applies, Escape discards |
| Shift+`J` | Join the word with the next |
| `x` | Remove the word or pause (refused while a text or edge draft is open; Delete and Backspace never remove) |
| Shift+`D` | Discard unreadable corrections, or drop those that no longer apply |
| `b`, `e` | Choose the start or end edge; then `h` `l` move it 10 ms, Shift+`H` `L` jump to the previous or next measured edge, Enter applies, Escape discards |
| `p` | Add a pause after the word: its gap to the next word, or 200 ms |
| `u`, Shift+`U` (Cmd+Z, Cmd+Shift+Z, Cmd+R) | Undo / redo the last correction |
| Escape | Close (after discarding any draft) |

Edge moves are an unsaved draft, shown with UNSAVED and the old and new
times, until Enter, so several nudges are one correction and one Undo step.
While a text draft is open, Enter and Escape act on it even if a click moved
focus away. Each change is one versioned project-service request; the sheet
waits for it, then follows the changed item. Buttons offer the same actions.
The sheet's word/pause list and counts, the Edit strip's pause bands and shot
ticks, and the Original bar's marks are cached per published analysis
(held by identity), not rebuilt each frame; the Original bar joins marks
closer than a pixel.

## Remaining

Headless correction commands (the CLI reads and prints corrections but does
not change them), audition of the selected word or pause from the sheet,
correcting sentence boundaries, and a local alignment pass that proposes word
edges (corrections snap to measured energy edges only).
