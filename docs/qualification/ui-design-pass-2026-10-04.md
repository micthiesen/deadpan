# Workspace design pass, 2026-10-04

A holistic review of the native workspace against the
[design targets](../design/README.md) found inconsistent hierarchy, a small
picture and several visible defects. This record lists what changed, the
evidence and what remains open.

## Findings and changes

| Finding | Change |
| --- | --- |
| An unpainted 8-point band ran under the header on every screen. The header panel sat inside `add_enabled_ui`, whose child allocation added item spacing that nothing painted. | The header disables itself inside its own panel. The canvas is painted behind every pane and the window clear color is the canvas token, so no gap can expose another color. |
| egui's bundled typeface did not read as a macOS application. | Installed SF Pro (text optical size 17) and SF Mono (weight 400) lead their families. They are read at runtime with the existing bounded, validated loader and never bundled; egui's fonts and the CJK fallbacks follow. Titles use SF's variable `wght` axis. |
| The picture was about 27% of a 1280×820 window; the target shows about 47%. Three rows of footer keycaps and an always-present empty Placed sounds panel took the space. | The footer shows as many contextual keys as fit in two rows, in priority order, ending with the Help key; exit keys lead while recording a Macro and `:` command stays early for parameter entry. An empty Placed sounds panel collapses into the beats heading at every height, not only below 700 points; a populated list keeps its panel at ordinary heights. |
| The inspector mixed centered buttons, an off-palette blue hyperlink, read-only values drawn as text fields and labels such as `– · -`. | Sections (GAIN, PAUSE SOUND, EDIT) with uppercase titles, aligned plain label/value columns and full-width command rows showing their keys. Gain buttons read `−3 dB  -` / `+3 dB  +`. Hyperlink, error and warning colors now come from the palette. |
| Pane focus used a tiny `FOCUS` word beside a bold title; breadcrumbs drew the current level as a disabled (greyed) button. | Uppercase section titles; the focused title turns lavender and gains a `FOCUS` pill. The current breadcrumb is plain semibold text and only ancestors are buttons. |
| Header buttons were uniformly boxed; the project title was grey. | Letter-spaced wordmark, frameless File/Render/Renders, semibold centered title, and Undo/Redo/Keys with grey keys. |
| Original / Your edit were two separate buttons beside four boxed navigation buttons. | A segmented control, frameless navigation with grey keys, and the edit delta in a pill. |
| The left rail mixed centered and wrapping buttons and kept a stale "Source ready" line after import. | Full-width command rows under REUSE and SOUND EFFECTS titles; completed imports no longer leave a status line (the status bar already reports them). |
| Help opened on Registers and Semantic Macros, the most advanced material, before the introduction and basic movement. The `menus` replay had been failing on this. | Help opens with the introduction and START & MOVE; `:registers` opens it with the register inventory first. |
| `footer_hints` measured the current row with `available_width()`, which reports the full width inside a wrapping row, so leading content was ignored. | It measures from the cursor position. |

Command buttons render their label and key as one text run, so the painted text
equals the accessible label (`Camera…  ,f`). Replay controls are found by that
label and paint checks inspect single text runs, so this keeps both meaningful.

An independent review of the first version found six issues, all fixed: the
recording footer would have dropped its exit keys first; the nested inspector
lost its Pause sound title and kept old button styles; Original hid a populated
Sounds list in tall windows; untitled copy/cut Help entries read as Macro
entries; `:` command could fall off the footer; and the inspector note repeated
the duration.

## Evidence

Hardware: Apple M5 Max, macOS 26.5 (Darwin 25.5.0). Replay used the pinned LGPL
FFmpeg 8.0.3 development prefix and the real project service and Metal path.

| Run | Checks | Failed |
| --- | --- | --- |
| Baseline full visual replay before changes | 5,239 | 2 (`menus` Help End, `room-tone` 140-point minimum picture) |
| Full visual replay after review fixes, then a focused `menus` rerun for the extended Help markers | 5,388 | 0 after the rerun |

Measured viewer heights from the same scenarios (points):

| Scenario | Before (min–max) | After (min–max) |
| --- | --- | --- |
| original-layout | 135–202 | 143–274 |
| room-tone | 109 | 149–181 |
| sound-placement | 125–247 | 133–255 |
| gain | 144–261 | 152–269 |

The room-tone minimum picture check now passes for the first time. Replays that
encoded the old behavior were updated deliberately: default-size Sounds is a
heading summary, the current breadcrumb is a Label (accessible `value`), and
the Help marker list covers its reordered end. `cargo nextest run -p
deadpan-app` passes 761 tests and 797 with `ui-harness`; Clippy and rustfmt are
clean for both feature sets.

Captured frames were inspected at 1280×820 and 960×640, including the
workspace, nested pause, sound placement, Help and command entry.

## Remaining

Card thumbnails (DP-20) are not part of this pass. Transport buttons keep their
pre-measured text geometry and older `Label  ·  Key` format. Physical VoiceOver,
IME and display-latency acceptance remain open; replay does not establish them.
