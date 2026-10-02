# Configurable editor bindings, 2026-10-01

Normal and timeline Visual bindings now load from an optional user keymap.
Action labels, prefix guidance and the production router share the admitted map.
Logical and physical matching are explicit. An invalid candidate retains every
shipped binding and a persistent diagnostic; it cannot partly install overrides.
Settings stay outside project content and are read once before native startup.

This increment starts at `df5e16d47f214c775cfa0df9e41ecec1fafeeee2`.
Core schema 43 and database 52 are unchanged. The remaining editor grammar,
mode maps, named registers, semantic dot-repeat and macros remain open. All
product requirements and Gates A through G remain open or partial. See
[the configuration contract](../KEYMAP.md) and
[retained evidence](../../tools/media-qualification/evidence/2026-10-01-configurable-bindings/README.md).

## Verification

The full locked workspace suite passed 3,497 unit/integration tests and both
documentation tests, with none ignored. That run used source inventory
`f640b47e47465e0d8591c11e101d3d7d3a03289c713e667ea4dcf35d85ab1b38`.
Afterward, only four app files changed: `preview.rs`, `harness/keymap.rs`,
`harness/nested_pause.rs` and `trim/waveform.rs`. The final base app suite passed
579 app tests and three headless tests. Other crate sources are byte-identical
to the full run.
The final `ui-harness` app suite passed 615 app tests and the same three headless
tests. The focused production replays passed 56 custom-map checks and 13 fallback
checks, each with a separate 130,696-case Kestrel audit over 62 reservations.
The live Kestrel source matched the reviewed registry digest.

The final release replay passed all 25 ordinary scenarios and their 3,087
checks, plus the separate Kestrel audit. Its binary SHA-256 is
`a22cb2eb8effcf9de13fe8f9eadb6ffe3fd2ffa00d4ba99233f1f14e826178f1`.
The generated-picture scenario remains skipped because this run did not supply
its real accepted-bundle fixture. The report retains every scenario's narrower
input, playback and media limits.

The final release images were inspected at 960×640 and 1280×820. Custom prefix
continuations and complete key hints fit; fallback errors remain readable;
the header stays compact through held-opener and ordered-command transitions.
The [retained images](../../tools/media-qualification/evidence/2026-10-01-configurable-bindings/images/sources.json)
identify their exact replay steps.

Final source inventory:
`79f3568cd19f1a3f644deb12fcd2163a03a47a9ee867c523050a5927f2efa5bd`.
Formatting, workspace/all-target Clippy and app-feature/all-target Clippy pass
on that inventory, with warnings denied for both lint commands.
Every retained command records its source inventory before and after execution.
Overlapping app/workspace suites must not be added together. The existing
nonfatal linker warning about the 16 MB `__eh_frame` limit remains in the logs.

## Review and retained failures

Independent general review found no actionable issue in startup admission,
configuration compilation, physical reservations, routing or semantic target
capture. Focused input review and its follow-up identified three corrections:

1. Releasing Shift while holding the Command opener could change Colon into
   Semicolon and leak its text into the field. The pure regression fails before
   the correction. The gate now retains physical identity until release or an
   explicit native-context revocation. It handles egui reporting the changed
   logical identity as a fresh press.
2. Text after Enter or Escape in one batch could alter the closing field. The
   field now gets only input before its first admitted submit/cancel. The suffix
   resumes before new input on the next outer frame. The production replay
   submits two distinct commands in order, across three layout passes each.
3. Closing the field before releasing its opener exposed the next frame's
   changed logical key to editor shortcuts. A production witness binds
   Semicolon to Help and fails exactly when that held key opens Help after
   submit. The final gate runs before modal routing and retains ownership after
   the field closes. The failing report is retained.

The review also proposed an in-batch suffix variant of the third issue. It was
withdrawn after tracing the existing whole-batch gate filter; only the verified
cross-frame case is treated as a finding.
Final focused source review accepted the correction and found no remaining
actionable input-ownership issue. Reviewers did not run tests or native UI.

Root image review found another layout defect: the right header's centered,
wrapping row inherited its previous height. Repeated field transitions grew the
header and reduced the picture. The production red witness places the Keys
button at y=66..94 instead of the first row. An explicit one-row starting
allocation now grows only to fit actual controls. Independent source review
accepted this correction; the replay retains a compact-header assertion.

The first full release replay exposed a separate first-paint header problem:
after resizing to 960×640, Working wrapped below the previous panel clip and
appeared only on the next frame. The header now compares its measured controls
with that inherited clip and requests a same-frame layout retry. The unchanged
rapid-input assertion passes. Independent review confirmed the cause and fix.
That run also caught a stale Backspace literal in the group-endpoint test and
a waveform sample label extending 0.0113 points below its fractional clip.
The test now expects the neutral parent-scope guidance, and waveform endpoint
labels sit one point inside the graph. All three focused production replays
pass. The first failing full report remains in the evidence.

Other retained failures include the macOS-unavailable `rustix::mkfifoat` test
helper, a changed hint return type, default-feature modal probes that used a
test-only method, an existing mark-suffix behavior, stale literal assertions and
a real compact-heading overflow. The corrected heading anchors both child
rectangles inside its available width. The macOS FIFO test uses the project's
guarded native process launcher. No production warning suppression was added.

The first error replay asserted painted help text on the window's initial
measurement frame. Its next capture already showed the full diagnostic. The
corrected replay explicitly allows that measurement frame before inspecting
paint; both the failing report and its image remain in the evidence.

## Limits

Tests run on Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, using the pinned
FFmpeg development prefix. The real picture fixture is `cfr-bframes.mp4`, SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Rendered replay uses the production project service, SQLite, decoder and
offscreen Metal pipeline. Input and picker events are scripted; composition and
selected playback delivery are injected. This does not qualify physical keyboard
layouts, native OS IME delivery, VoiceOver, audio devices, acoustics or display
color. Helper layout tests cover long paths; inspected custom-map images use
short practical paths.

The minimum-size Original view still needs layout work: its picture is about
50 points high when the Original strip and empty Placed sounds pane are present.
This remains a workspace acceptance issue. The keymap captures make it visible;
passing the input and paint assertions does not complete visual acceptance.

The pinned egui-winit adapter can fall back from an unsupported logical symbol
to its physical position. Strict logical provenance remains open. The existing
conservative whole-batch IME priority is preserved: any composition event
prevents editor submit/cancel in that batch. Physical mode is one map-wide choice;
the remaining draft-mode routers retain their existing fixed bindings.

No ordinary native window was opened for this increment. Short-lived replay
processes exit after each run; the test app stays closed between runs. The final
process inventory contained no Deadpan process. Personal keymaps were neither
read nor written.
