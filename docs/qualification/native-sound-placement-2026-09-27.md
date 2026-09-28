# Native sound placement qualification, 2026-09-27

The native workspace now places a complete measured catalog sound over Your
edit, without inserting picture time. `,s` and `:sound-place` use the retained
Edit cursor. A separate Placed sounds pane and inspector provide selection,
exact frame movement, fine 48 kHz sample positioning, gain, endpoint policies,
removal and ordinary durable history. This is the guarded root-event subset;
DP-05, DP-09, DP-15 and every delivery gate remain partial or open.

## Authoring and timing

The project service captures session and revision, resolves the audio-only
catalog receipt, derives its complete measured span at the project rate, and
commits through the existing `SetSound` or `DeleteSound` transaction. It leaves
picture nodes and duration unchanged. Overflow and unsupported routed movement
fail explicitly. Gain and endpoint changes preserve an existing route journal.
The writer rechecks receipt admission; actual source bytes remain subject to
playback's verified provider. No complete-file hashing moves onto the UI or
writer thread.

Frame nudges translate the complete exact mapping and audible selection while
preserving the independent sample offset. Two one-frame nudges equal a counted
two-frame nudge, including 30000/1001 fps and a 32000 fps half-sample boundary.
Inverse nudges restore the same source phase. Explicit fine position entry
chooses a whole sample onset; accepting an unchanged prefilled value preserves
fractional phase and creates no history. Parameter text captures the event and revision,
including absence of an eligible event; late completion cannot supply a target.

## Review corrections

Independent service review found no initial admission or route-preservation
defect. UI review identified accumulated sample rounding, Original-to-Sounds
focus mismatch, and missing command captures that could acquire a later target.
The fixes retain exact mapping coordinates, share pane-entry behavior and require
captured targets. Follow-up review found explicit context commands leaving sound
focus active and `:delete` bypassing sound capture checks. Context commands now
leave event focus; command deletion uses explicit `:sound-delete`.

Production replay exposed egui's deferred Shift-Tab traversal taking the next
Enter to Undo. Explicit pane navigation now cancels that traversal before
requesting focus. Viewer buttons claim picture navigation. A held writer reply
also exposed focus transfer closing active command entry; completion now
preserves text ownership so submission can report the captured stale target.

Compact stopped controls share one row. Preparing and Playing reserve a second
row, and pointer-triggered transitions request a same-frame layout retry so the
first active paint cannot overlap Placed sounds. Replay checks that release
frame directly. A final harness correction uses the pane's accessible focus
label when locating its boundary; the painted heading remains checked separately.

The first placement replay looked up the new catalog controls before their next
paint. An explicit accessibility label and a service-settled check alone did not
resolve it. The scenario now advances the actual next UI paint. These failed
attempts remain recorded. Two compile failures during integration are also
retained: a `json!` recursion limit fixed by assigning new snapshot fields after
construction, and a match arm returning the submission boolean instead of unit.
No test process was restarted or abandoned because its output was quiet.

## Visual target and acceptance limits

The retained [ImageGen board](../design/boards/sound-placement-board-v2.png) and
[exact prompts](../design/README.md) guide the independent catalog, placed-event
list, conditional inspector, lavender selection, explicit Edit destination and
visible keys. The coded interface preserves those roles. Generated custom-silence
controls remain absent because the authored allowance policy is unfinished.

The first complete replay passed 733 checks across fifteen UI scenarios and the
Kestrel audit. Visual inspection then found a 52-point picture at 960×640 with
placed sounds. The compact layout reduces card padding and combines playback
and Monitor controls; command hints describe samples and gain rather than beat
frames. The final focused replay measures 125 points of stopped picture height
at 960×640 and 255 points at 1280×820. Preparing/Playing retains 119 points at
the minimum size with transport, clock and Monitor text inside their paint clips.

Inspected captures preserve the board's hierarchy, separate catalog/event
selection, lavender focus, yellow edit cursor and visible action keys. At the
minimum size, the catalog and inspector require scrolling; command entry reduces
picture height further while keeping its field and unit hint visible. Original
and beat thumbnails, richer sound rows and further spacing polish remain open.
This is a reviewed implementation of the board's current sound workflow, not
full visual completion of the editor.

This increment does not establish full nested sound ownership, Repeat/Retime
transforms, custom silent-Hold allowances, voice treatments, listening quality,
encoded export or preview/export equivalence. Replays use real project/media
services and Metal but do not exercise sound output in the placement scenario.
Physical keyboard delivery, native IME, VoiceOver and physical display appearance
remain separate acceptance work. The fixture is a small 120-frame Original and
8197-sample stereo sound, not a full-size workload.

## Verification

The final source manifest is
`4711a486cea0d098ac4642dcd8aa67d3b5af51ed287b275f39898b85b2bc2ec7`,
against base commit `be30f88de08eafc1f6bc6411b612f156024459ad`.
Core schema 30 and database schema 36 are unchanged by this increment.

- Formatting and strict workspace Clippy passed.
- The complete locked workspace test run passed **1,888 tests**, with zero
  failures or ignored tests across 151 test groups.
- Strict optional-harness Clippy passed. Its app run passed **257 unit tests and
  2 headless integration tests**, with zero failures or ignored tests.
- The final focused production replay passed **146 sound-placement checks** and
  the separate **3,472-case / 62-reservation Kestrel audit**. Its screenshot quota
  warning preserves semantic frames and named checkpoints.
- The final complete visual replay passed **775 checks** across fifteen UI
  scenarios and the shortcut audit. Inspected captures include sound placement,
  Preparing/Playing, sample entry, routed rejection, Camera, the workspace and
  minimum-size large-project cards. The sound-placement quota warning is its only
  reported finding; no failure was hidden or skipped.

These runs used Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned LGPL
FFmpeg 8.0.3 prefix. Command records retain exact arguments, durations and source
identities. Harness Clippy overlapped the workspace run's final doc-test phase;
no process was restarted or killed. Visual and release replay run after these
commands complete. Thermal/power state and OS file cache are uncontrolled.

The [retained evidence](../../tools/ui-feedback/evidence/2026-09-27-sound-placement/README.md)
contains exact command records, source manifests, compressed complete reports,
failed development attempts and the six inspected sound captures.

The separately compiled release replay passed **1,745 checks** across all fifteen
scenarios and the shortcut audit, with no findings. Its timing pass performs no
screenshot readback. Warm samples exclude the documented warm-up cycles:

| Measurement | Samples | p95 | Existing budget |
| --- | ---: | ---: | ---: |
| Warm navigation input CPU | 120 | 0.138 ms | <8 ms |
| Warm navigation to completed picture | 120 | 1.462 ms | <80 ms |
| 10,000-beat navigation CPU | 160 | 0.316 ms | <8 ms |
| Cached Repeat to completed picture | 40 | 1.612 ms | <50 ms |
| Silent freeze insertion to completed picture | 40 | 2.067 ms | <100 ms |

These are offscreen small-fixture measurements, not physical display latency or
full-size editor qualification. The sound scenario also retains mixed input,
commit and picture samples, but does not establish a separate sound-edit latency
budget. No performance comparison or speedup is inferred from earlier runs.
