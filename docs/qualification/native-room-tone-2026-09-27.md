# Native room-tone qualification, 2026-09-27

The native workspace can now choose, audition and apply room tone to an
ordinary selected Hold. It retains one pinned Original and changes the pause's
sound in one undoable transaction. No product requirement or release gate is
closed by this subset. Base commit:
`938ab5ee32a64924e749204af249537e71e5f1e7`.

## Authored workflow

Copy an Original range with `v`, motion and `y`, return to Your edit, select a
Hold and open `:room-tone`. The service resolves measured picture PTS directly
onto the original audio sample clock. In rounds upward, Out downward, and the
range intersects measured contiguous audio. Empty selections fail. The sheet
shows these actual sample endpoints, source rate and seconds; changing a sample
field revokes the prepared descriptor until the new range is prepared.

The draft captures Hold, ordinary Sequence scope, cursor, session, revision,
source and qualification. Captured absence remains an error. Both successful
preparations and failures carry ticket/session/revision identities. Older
results cannot consume a newer pending request. Only Apply sends `SetHoldAudio`;
preparation, source audition and Cancel leave history unchanged. The writer and
store revalidate the context and qualified source contract before committing.

Reopening uses the Hold's saved range. `Use copied Original range` explicitly
replaces the draft from its captured copy. `:hold-silence` changes only the
policy; Undo restores it. Picture, exact duration, framing, marks and retained
audio clocks keep their existing contracts. Explicit sound permissions remain
separate, including the setter's reversible removal of obsolete silence gates.

## Source audition

The new `AudioRange` descriptor admits qualified Original A/V audio without
weakening the audio-only catalog `Sound` contract. Construction runs on the
project service worker. Playback checks the captured source contract and uses
verified Original bytes. Its temporary Source view retains the complete source
with exact affine phase and a separate selected mask. Its local clock starts
at zero and stops at the selected duration rounded once to the mix clock,
excluding the enclosing project frame's slack. Cache identity includes range.

Source preview uses no implicit lead/follow context and leaves the retained
picture, editor cursors and selected beat intact. Delivery clocks remain
monotonic through loops. Field changes, Cancel, context changes and faults revoke
the run and resume state. Source preview hears the range; committed Sequence
audition hears the crossfaded Hold recipe. Neither claims speech classification
or loudness normalization.

## Review and corrections

Independent reviews covered native target capture, source admission, service
replies, audio context, keyboard ownership and the rendered design. Review found
that an old copied range could supersede a saved room-tone range on reopen;
saved policy now wins and replacing from copy is explicit. It also found that
an untagged prior service error could consume a newer preparation ticket. The
dedicated failure envelope and delayed-failure regression close that race.

The first base-app compile caught the current egui API requiring an explicit
theme when obtaining style. The first strict feature lint caught a collapsible
inspector condition. Both were corrected without suppressions. An invalid-range
test's reversed literal was changed to variable endpoints during review.

The first painted replay passed 67 checks, then found that pointer Cancel left
egui's prior modal layer active for the next input frame. A following colon lost
command-field focus, so Enter tried ordinary group navigation. Closing now
requests a layout pass without the modal before the next key. The same replay
then passed its immediate-command regression. Development failures remain in
the evidence; no timeout or quiet compilation triggered a restart.

## Visual comparison and limits

The saved [ImageGen board](../design/boards/room-tone-board-v2.png) and its
[exact prompt](../design/prompts/room-tone-board-v2.txt) supply the target.
The implementation preserves the dark palette, lavender range/action emphasis,
explicit draft state, sample fields and visible local keys. The source clock
starts at zero and remains distinct from Original/Edit clocks behind the sheet.
The code adds an explicit Prepare step after manual field edits and a separate
replace-from-copy action to avoid silently reusing stale source data.

The 960×640 and 1280×820 sheet captures were inspected at full size, along with
actual paint clips. All fields, speech-selection guidance and Apply/Cancel
actions fit. The wording is denser than the board but explains source audition
versus the final pause and explicit application. No blocking aesthetic finding
remained in the sheet review. The final saved-state captures at both sizes also
show the source, exact sample range and room-tone/silence actions within their
paint clips. Waveform visualization and richer thumbnails are still targets.
The fixture is a small numbered test video, not the board's
illustrative interview or evidence of quiet ambience.

The final HTML contact-sheet inspection could not run: delegation to the
required browser role returned `agent type is currently not available`.
No alternate browser route was used. This check remains outstanding; the
full-size PNG and actual paint-clip reviews above did complete.

The replay uses production widgets, routers, project storage, real decoded media
and Metal. Its audio delivery updates are explicitly simulated; separate decoded
PCM and fake-device tests exercise actual audio preparation and callback output.
Neither simulated IME nor an accessible tree establishes physical input, OS IME,
VoiceOver, physical display or acoustic quality. Native Repeat-gap/fragment
authoring, representative ambience listening, full effects, mixing and export
remain open.

## Verification

The final integrated checks all exited successfully on source manifest
`79ba9ff21c74a2e363f9d8ea2878ad903dd03f9a613953463a5a0eff9e417ba2`.
The source remained frozen across these runs. The
[retained evidence](../../tools/ui-feedback/evidence/2026-09-27-native-room-tone/README.md)
contains exact commands, source manifests, terminal records, logs, compressed
reports and six final captures. The manifest verifies all 48 retained files.

| Check | Observed result |
| --- | --- |
| `cargo fmt --all --check` | Passed. |
| Strict workspace Clippy, all targets and locked dependencies | Passed. |
| `cargo test --workspace --locked` | 1,941 passed across 153 result groups; none failed or ignored. Includes 241 base-app tests and two headless-entry tests. |
| Strict app Clippy with `ui-harness`, all targets | Passed. |
| App tests with `ui-harness` and locked dependencies | 270 app tests and two headless-entry tests passed; none failed or ignored. |
| Full visual replay | 909 checks passed across 16 UI scenarios plus the shortcut audit; the new room-tone scenario has 90 checks. |
| Full release performance replay | 1,879 checks passed across the same scenarios and audit, with no findings. |
| Kestrel production-router audit | 62 reserved bindings, 4,464 cases, zero conflicts; the live source matches the retained fixture. |

The focused media and playback development runs each passed three new tests.
Their coverage includes inward measured-PTS selection, signed/44.1 kHz native
samples, source admission, full resampling context, cold seeks, exact terminal
device prefixes, loops, monitor gain and range-sensitive caching. The final
workspace gate also covers the service's nonauthoring preparation, stale target
and reply rejection, atomic Apply, undo/redo/reopen, field validation and modal
key ownership. The painted replay adds actual controls, native text and
simulated IME, cancellation, explicit copy replacement, saved inspector clips,
and committed Sequence playback routing.

Visual replay retained two intermediate screenshot-quota warnings, in
sound-placement and room-tone. Semantic checks and named captures continued.
The first development visual build overlapped final source edits, so its launch
manifest alone does not identify that executable's source; the report retains
the actual binary hash. Final full runs use the frozen manifest above.

### Release responsiveness

The release replay took 3.57 seconds after compilation, with screenshot readback
excluded. Room-tone measurements were:

| Measurement | Samples | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| UI frame CPU | 186 | 0.188 ms | 0.676 ms | 2.913 ms |
| Input to state | 110 | 0.181 ms | 0.689 ms | 0.924 ms |
| Prepared source range observed | 9 | 0.567 ms | 0.788 ms | 0.788 ms |
| Input to committed revision | 5 | 1.217 ms | 49.536 ms | 49.536 ms |
| Input to completed picture | 14 | 1.672 ms | 55.651 ms | 55.651 ms |

The general edit-latency scenario recorded p95 commit 0.912 ms over 89 samples
and p95 completed picture 2.116 ms over 90 samples. These are small-fixture
offscreen measurements on an uncontrolled shared host, with small sample counts
for this new workflow. They do not establish full-size media performance,
physical presentation or audio-device latency. Raw samples and outliers remain
in the reports; no result was rerun to obtain a passing measurement.

Hardware: Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned LGPL FFmpeg
8.0.3 developer prefix. All Cargo and GPU runs have one serial owner.
