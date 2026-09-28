# Native gain editing and draft comparison

This increment adds native gain authoring to the persisted treatments and PCM
path qualified in [authored gain](authored-gain-2026-09-28.md). It does not
complete DP-09 or qualify the full editor.

## Implemented boundary

Normal `+`/`-` applies counted 3 dB steps to the captured selected beat. Placed
sounds retain their separate target; Original and catalog focus cannot change
a retained picture beat. `:gain <dB>` sets exact trim and `:gain-mute` toggles
true mute. `:gain` opens an unsaved editor for trim, mute, exact owner-output
envelope ranges, points and curves, and half-open mute ranges.

The writer service validates proposals through ordinary command preview without
publishing a workspace or writing history. Proposed snapshots retain committed
source receipts and explicit draft/change content identities. Playback caches,
device updates and resume state compare that identity. Before/Draft share one
captured Sequence window and retain the heard content sample across loop laps.
Apply uses one normal reversible transaction; cancellation creates no edit.

The panel keeps Apply/Cancel and comparison controls outside its scroller,
confines Tab to enabled draft controls and preserves native text/button ownership.
The accepted picture remains visible through preparation and cancellation.
The graph displays authored curves, without an invented waveform.

## Review corrections

Independent source review identified an obsolete proposal error reaching the
untagged project-error slot. Preparation failures now travel only in the tagged
proposal result. Additional review found that invalid field text disabled Pause
and that Cancel cleared the accepted picture unnecessarily. Pause remains
available during playback, Restart requires valid applied fields, and Cancel
requests its entry picture without clearing the displayed target.

The parent also confined native Tab to the draft controls while retaining the
workspace picture at full opacity. Reselecting the active Before/Draft tab now
leaves its running generation and exact position intact; Restart is explicit.
The inspector displays the draft recipe and names its unsaved state.

The first painted replay exposed a 50-point picture in the draft layout.
Gain now uses a compact owner row, retains empty structural panel IDs and omits
inactive moment/sound controls. A later minimum-size assertion found a redundant
comparison hint still consuming picture space; that hint was removed. The
960-point layout also exposed a curve menu extending outside its column.
Narrow windows now stack the graph and exact fields. Graph key labels remain
single-line, the owner heading aligns left and range help fits its row.

Populated forward/reverse Tab replay exposed offscreen focused fields even
though pointer scrolling could reveal them. Newly focused controls now request
immediate visibility; text fields reveal their entire label/input pair. Explicit
heading/Cancel boundary wrapping removes an empty native Tab stop. The scroller
reserves enough height for the complete graph at the minimum window, and exact
quarter-duration ticks make its owner clock readable. No text/control clipping
assertion was relaxed.

Two separate replay defects were corrected: ComboBox selected text appears as
an accessibility value, and a newly opened popup first exposes disabled,
provisional geometry. The harness resolves that value and waits a bounded number
of paints for an enabled, fully clipped menu item. It does not bypass disabled
controls or accept viewport-only geometry as proof of visibility. Failed runs
remain retained. Independent source review found no further concrete issue in
the revised panel IDs, focus boundaries, proposal admission or playback caches.

The full replay's two additional findings have separate fixes. Picture
containment permits at most 0.01 physical pixel of floating-point residue,
checked in widened arithmetic with finite positive geometry; regression tests
reject quarter-pixel clipping on every edge and invalid scales. Text, controls
and graph clipping retain their strict checks. The inspector now places Hold
audio, group entry and node-specific parameters before general gain controls.
Gain buttons reveal themselves on actual accessibility focus. Normal Tab still
cycles panes as specified; the gain draft separately uses native Tab traversal.
Independent review found no further concrete issue in these corrections.

## Verification

The [retained evidence](../../tools/audio-qualification/evidence/2026-09-28-native-gain/README.md)
contains command records, source inventories, complete compressed logs/replays,
selected real captures and native review notes. Scratch execution used
`/tmp/deadpan-native-gain-u891jpt2`.
The complete base workspace command, `cargo test --workspace --locked
--no-fail-fast`, exited 0 in 1,335.53 seconds. It passed **2,029 tests with zero
failures and zero ignored**: 2,027 native unit/integration tests and two doctests.
This includes all 45 playback tests, real Before/Draft PCM, admission failures,
exact recipe models, captured writer proposals, one-step Apply/undo and durable
reopen. Its source manifest is
`1e4d91ea1a8f17b8e8aef47156f4911db812fdfeb0ff681cf9c28f7590ae95b4`.

Only ten native app/UI source files changed after that workspace run; backend
and playback sources remained unchanged. Final normal-app tests pass **263**
(261 unit and two headless integration); optional app/harness tests pass **298**
(296 unit and two headless integration). Both have zero failures and ignored
tests. Final format and strict workspace/all-target Clippy pass in both feature
configurations. These gates use source manifest
`533439a4ece260aa1ee56751987ff765d8cd3601c52eafa0f323743a29b41dbe`.
The evidence identifies exact Cargo executables and the earlier 294-test runs;
these overlapping populations must not be added together.

Focused Metal replay `gain-visual-09` and its final corrected follow-up each
pass 266 gain checks plus the Kestrel audit, which covers 5,456 routing cases
against 62 live reservations without drift.
Four populated keyboard circuits visit 34 controls each with complete visible
labels, values and hit rectangles. Picture heights are 145 points at 960×640
and 270.1875 points at 1280×820. Both complete graph captures were inspected
against the imagegen gain board: picture hierarchy, unsaved owner, exact fields,
graph ticks and fixed comparison actions match the implemented design boundary.
Measured waveforms remain absent explicitly. Final default/minimum Gain and
saved Hold inspector captures were inspected again after the corrections.

Layout and popup fixes have also passed strict workspace/all-target feature
lint. The complete visual invocation ended with two failed scenarios after
754.32 seconds; all other scenarios passed, including 265 gain checks. Workspace
picture containment rejected a 0.00001526-point rounding residue after a 2×
resize, with the mesh exactly matching its fitted canvas. Room-tone inspection
exposed real clipping because the new general gain controls preceded the Hold's
specific audio recipe. Both original failures are retained. Final focused
follow-ups pass workspace 81, room tone 103, gain 266 and Retime 16 checks,
each with the shortcut audit. Twelve additional room-tone checks cover real
accessibility focus, full gain-button paint/hit clipping, keyboard entry into
the captured Hold's gain editor and unchanged cancellation. Other completed
full-run scenario results remain retained. The original full invocation is
still recorded as failed, rather than relabeled by the passing follow-ups.

The parent inspected the exact normal-app binary in a private native macOS
bundle. Native `:gain`, Shift+Tab/Tab wrapping, Cmd+A text editing, Set trim,
keyboard envelope creation and key update, offscreen field reveal and Escape
cancellation passed. Shortcut characters remained inside the text field. The
project dump after closing is byte-identical to its pre-open dump, SHA-256
`b128bf0fe5d6ec9143681ad7e04c9429cabb1ac4cab41d64c4d6033856007ff9`.
The [native record](../../tools/audio-qualification/evidence/2026-09-28-native-gain/native/review.md)
retains the sequence, exact executable hash and limits. No device playback or
acoustic judgment was attempted in this window pass.

The locked release workspace build passed in 1,072.66 seconds. The complete
release performance invocation passed **2,156 checks** across 17 scenarios and
the shortcut audit in 26.88 seconds on the same final source. No scenario failed.
The measured host was an Apple M5 Max (Mac17,7), 128 GiB, macOS 26.5.2 (25F84),
with Rust 1.97.1 and the selected FFmpeg prefix. These results use the small
qualified fixture and offscreen Metal completion:

| Workload | Samples | p50 ms | p95 ms |
| --- | ---: | ---: | ---: |
| Warm navigation input CPU | 120 | 0.694 | 0.854 |
| Warm navigation to completed picture | 120 | 4.545 | 6.356 |
| Cached Repeat to completed picture | 40 | 7.809 | 9.438 |
| Silent Hold fallback to completed picture | 40 | 9.591 | 10.769 |
| 10,000-beat navigation CPU | 160 | 1.447 | 1.792 |

Each population has zero failed or timed-out samples and passes its unchanged
budget. Complete timing populations, including cold initialization and unrelated
frame timings, remain in the raw replay. These warm subsets do not establish
native display latency, acoustic behavior or full-size media performance.

One owner executes Cargo and GPU checks serially. Compiler yields are treated as
observation boundaries; running processes are preserved until their terminal
result. The initial base lint found seven style/API lints. The initial optional
UI build found one harness value/reference mismatch. The new picture regression
tests also needed a scoped import and numeric-literal grouping correction.
All failures are retained and corrected without suppressions or relaxed budgets.

## Remaining acceptance

Measured waveforms, physical keyboard layouts and IME delivery, VoiceOver,
acoustic quality and native display timing remain separate qualifications.
The production replay uses real writer/media/GPU paths with explicitly injected
audio delivery; playback tests separately exercise real qualified PCM. Neither
fixture alone proves a complete native listening session, full-size performance,
preview/export equivalence or all audio operations.
Outside the gain draft, the minimum normal workspace with a copied Original
range still compresses the picture substantially. That broader layout finding
remains separate work; the 145-point minimum above qualifies the Gain layout.
