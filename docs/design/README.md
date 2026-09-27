# Interface design targets

These imagegen boards are the visual targets requested by the product owner.
Use them before implementing a screen, then compare the native result against
them. The [full specification](../spec/DEADPAN_SPEC.md) remains the authority for
behavior, timing, keyboard bindings and completion. A concept image does not
establish an implemented feature.

## Boards

| Target | Role | Exact prompt |
| --- | --- | --- |
| [Single-original workspace](boards/single-source-workspace-v1.png) | Primary target: one pinned Original, Your edit, picture, beat cards, conditional inspector, sounds and visible keys. | [Workspace prompt](prompts/single-source-workspace-v1.txt) |
| [Single-original product board](boards/single-source-product-board-v2.png) | Choose one video, full-original baseline, revised edit, time-range reuse, sound/AI and keyboard/library views. | [Initial prompt](prompts/single-source-product-board-v1.txt), [review corrections](prompts/single-source-product-board-v2.txt) |
| [Shape, extend, finish](boards/single-source-workflows-v1.png) | Camera versus temporal Trim, sound placement, explicit AI acceptance, export, recovery and local models. | [Workflow prompt](prompts/single-source-workflows-v1.txt) |
| [Camera and framing](boards/camera-framing-board-v1.png) | Dedicated Camera preview, visible movement and scale keys, manual targets, keyboard region fields and a whole-Hold creep. | [Camera prompt](prompts/camera-framing-board-v1.txt) |
| [Original moment reuse](boards/original-moment-reuse-v1.png) | Temporal Visual selection, copy without editing, explicit paste destination and one-step undo. | [Moment prompt](prompts/original-moment-reuse-v1.txt) |
| [Sound audition](boards/sound-audition-board-v1.png) | Focused sound catalog, independent audition clock, visible play/pause/loop keys and retained edit context. | [Sound prompt](prompts/sound-audition-board-v1.txt) |

The built-in `image_gen.imagegen` tool generated these assets on 2026-09-23 and
the Camera companion on 2026-09-24, the moment-reuse companion on 2026-09-26
and the sound-audition companion on 2026-09-27.
The current boards implement the owner's single-original direction in specification
1.1: begin with the complete video and gradually reshape it, reuse its moments,
add audio effects and accept AI extensions. New projects belong in Documents/Deadpan.
The enlarged workspace controls proportions; companion screens cover intended
interactions. The product-board correction replaces a spatial crop with a temporal
range and clarifies whole-beat shortcuts. The workflow board references the workspace.
[The manifest](manifest.json) retains byte identities, dimensions and prompt
associations. Original generated files remain in the generator's output directory;
these copies and the exact prompts are committed project assets. The pictured
interview is generated representative media, not a required fixture or bundled
user footage. No model identifier was returned by the tool.

## Visual contract

Picture comes first. Use one large fitted viewer, a quiet Original column, a useful
selection inspector, compact structural beat cards and a thin persistent status
bar. Keep the inspector conditional on useful selection. The OS owns window
chrome. Use ordinary native-quality buttons and focus rings, not painted replicas
of controls. The enlarged workspace is the primary target when board proportions
or examples differ.

| Token | Value | Use |
| --- | --- | --- |
| Canvas | `#17191D` | Main background and viewer surround |
| Panel | `#202329` | Controls, cards and supporting panes |
| Primary text | `#E9EBF4` | Names, values and active context |
| Secondary text | `#AAB0BF` | Metadata and supporting instructions |
| Border | `#373D49` | Subtle separation |
| Selection | `#C4B5FD` | Selected outline, focus, restrained filled accents |
| Cursor | `#F6D365` | Exact boundary marker and position |
| Saved | `#A7F3D0` | Confirmed persisted state, paired with text |
| Spacing | `4 / 8 / 16 / 24` pt | Related controls through pane separation |
| Radius | `4 / 6` pt | Controls / cards |
| Text | `13–14` pt body | Compact readable system-style text; monospace units and keys |

Lavender is selective. Do not tint every panel or make controls compete with the
image. A selected beat has an outline, name and textual scope, not just a color.
Pane focus has its own visible label or heading treatment. A selected Original and
selected beat may coexist; their selection outlines alone cannot tell the user
which pane receives `j/k`.
Use short unit-bearing values and visible keycaps. Error and pending states need
words as well as distinct styling. Resize with the window and retain readable
controls, a usable image and a visible status bar at the supported minimum.

Structural cards can have equal readable widths. Their durations and half-open
frame boundaries must be explicit; do not put a continuous seconds ruler above
such cards. A local cursor marker maps within its own card and must not imply
that adjacent equal-width cards have equal duration. Repeat is one compact object
with a total-play count, never an expanded widget per occurrence.

## Interaction contract

Pause insertion adds a visible `,h` action and Choose pause duration entry to the
selection inspector. Keep the picture dominant and reuse the existing command
field for exact duration input; show the resolved frame count, freeze/silence
policy and insertion boundary there. A committed pause selects its own card and
shows its duration and policies. Prefix `3,` teaches `h` and the half-second count
unit. The interaction keeps one Original and one reversible edit in view.

Ordinary Sequence navigation keeps this same workspace hierarchy. The beat
header adds compact breadcrumbs and a visible Backspace action; Enter opens the
selected group. Keep the picture dominant, group selection distinct from pane
focus, and group-relative position visibly separate from the absolute edit
clock. Long breadcrumbs scroll without covering the cards. See the implemented
[navigation contract](../GROUP_NAVIGATION.md) and its remaining occurrence scope.

| State | Visible information | Behavior |
| --- | --- | --- |
| Normal | Mode, Original/Your edit context, pane, selected beat and scope | `h/l` move frames; `j/k` move the focused list; counts precede motions; `Tab`/`Shift-Tab` cycle visible panes. In Your edit, beat actions require an editable selection. |
| Operator pending | Exact typed prefix and current scope | No timeout. `3rr` means three total plays. Escape cancels the pending input. |
| Command/text | Focused field, command reference and units | macOS editing and IME own keystrokes. Text never dispatches structural shortcuts. Same-frame text is processed before submission. |
| Parameter entry | Selected node, current value and command | Current native setters use the shared command entry. Return commits a validated command; Escape cancels entry. This is not a live parameter preview. |
| Temporary preview | Camera mode, root-beat scope, unsaved draft and actual resulting picture | Camera: Enter commits once; Escape restores entry framing. Numeric fields retain native editing. Trim and other live parameter previews remain required. |
| Candidate | Committed provider beside candidate status | Ready does not change the edit. Explicit acceptance is undoable; Escape never rolls back a previously committed edit. |

Original is the Source context; Your edit is the Sequence context. Original
browsing remains non-destructive. Sequence actions show their current group
scope explicitly. Camera edits selected beats at that depth. Original Visual
selection and copied-moment paste follow the companion board. Full selector,
occurrence, saved-target, Trim, Visual replacement and macro
workflows remain required and must use the same typed command boundary as the
implemented actions. Discoverable controls refer to actual bindings. Standard
macOS focus, copy/paste, composition and logical-key behavior take precedence
while text is active.

## Reviewed concept corrections

Generated visual content is a composition reference. The following generated
details are intentionally not implementation instructions:

- The sound-audition board separates catalog selection, pane focus and the
  selected beat. Its violet Original outline must not imply that Original is
  also the active audition target. Sound elapsed/total values use their own
  sample-derived seconds clock. Space toggles audition and Shift+Space requests
  a whole-sound loop; leaving Sources or changing sound discards resume state.
  Keep the stopped picture, edit cursor and authored selection intact. The
  waveform, sound placement and per-event effects remain separate requirements;
  this board does not authorize controls for unfinished behavior. Generated
  mini-state labels describe intended interactions, not test evidence.
- The moment-reuse board is the target for native Original selection and paste.
  Its temporal range, context/focus separation and visible paste destination
  extend the workspace rather than replacing it. Copy does not insert or change
  history. `y` currently copies a ready Original range. The general Normal-mode yank
  operator remains required; do not advertise it as implemented. `v` finishes a range without committing a sequence edit. `p` and
  `P` use the selected beat, while an explicit Visual replacement owns its range.
  The pictured 24 fps Original makes ordinal and project durations coincide;
  VFR media must use measured PTS and the actual project rate. At the Out boundary,
  the displayed right-hand frame is excluded; at source end, show the final frame
  without moving the boundary backward. The illustrated selection width is not
  a measurement: the coded range bar must map exact source time and distinguish
  endpoint labels from approximate pixels. Its thumbnails remain conditional on
  a real bounded service, and its sound entries do not prove placed sound events.
- The Camera board is a target; [native qualification](../qualification/framing-2026-09-24.md)
  records the implemented subset. Saved targets and regions remain required.
  Its large crop boundary illustrates source context; the coded main viewer must
  show the actual draft output, with source context confined to an explicit
  overview. Never overlay a purported source crop rectangle on already cropped
  output. Show numbered choices only in the target picker so digits can remain
  counts during ordinary Camera adjustment. Its source thumbnail and illustrated
  sound entries require real supporting data. The board's Mug example depicts
  manual targeting, not detection or tracking.
- Use the implemented binding table and normative grammar. `?` and `:help` open
  actual help. The product board's Original-view footer still includes `rr`/`dd`;
  the coded Original view must instead teach browsing, return to Your edit and
  explicit reuse. Structural edits never target the immutable Original.
- The enlarged workspace's `y` reuse label describes future range copying.
  Current whole-original reuse uses `,i`; no inactive range-copy control
  should imply that `y` already works. New starts with the original already inserted.
- Thumbnails, filmstrips, waveform selection, recent projects, YouTube
  acquisition and advanced workflow actions require actual supporting data and
  behavior. Use truthful media-type tiles until a bounded thumbnail service exists.
  [Original/edit audition](../PLAYBACK.md) has Space Play/Pause and Shift+Space selection loops beside the picture,
  explicit preparation/cancellation and independent monitor volume. Label its
  current limited edge-faded bus; full voice processing, group mixing and
  mastered export remain required.
- Sound import registers audio; the focused catalog also auditions it through
  the shared playback service. The Place a sound panel is the target
  for a real anchored overlay, never an audio-only sequential beat with blank picture.
- The workflow board shows live Camera preview and intended Trim preview. Current Hold and
  Repeat setters are validated command entry; do not claim Escape restores a
  committed edit or that a text field previews on every keystroke.
- Ready AI candidates keep the current provider unchanged until explicit
  acceptance. Model installation and candidate UI require real app integration,
  measured disk needs and licenses; illustrative states are not measured results.
- Export values are illustrative. Derive them from the immutable export revision
  and source-based policy, and show Verified only after emitted-file verification.
- Recovery distinguishes an initialized project with unavailable original bytes
  from a project whose original never finished preparation. Locate matching original
  must verify identity; Choose original is only valid before initialization.

Original duration and edit duration use project frames. Original browsing uses
measured video-frame ordinals, which can differ for VFR or offset media. Show the
clock explicitly. The example has 240 original project frames and 299 edited
frames: 132 + 11 + (3 × 24) + 84. Its local cursor 137 lies inside the Hold.

## Superseded explorations

The earlier [product board](boards/product-board-v1.png),
[workspace](boards/workspace-target-v1.png),
[workflow v1](boards/workflows-board-v1.png) and
[workflow v2](boards/workflows-board-v2.png) retain the previous broader editor
exploration for provenance only. They are not current product targets. The
[single-original product v1](boards/single-source-product-board-v1.png) is retained
as the input to its corrected v2. Their exact prompts and hashes remain in the
manifest. Preserve useful styling and backend capabilities without reviving the
old multi-video creation flow.

## Implementation and review

The [2026-09-27 layout review](../qualification/workspace-layout-2026-09-27.md)
compares actual Metal captures with these targets. Compact status and beat rows
give the default viewer about 36% more height. Frame navigation shares the
Original / Your edit header; Camera and pause actions precede inspector details.
Sound transport stays below the scrolling catalog, with measured text and
buttons that fit on the first resize frame. Errors likewise reserve their actual
wrapped height immediately. These changes improve hierarchy and discoverability;
the boards' thumbnails and unfinished editorial workflows remain required.

The initial native root-editing baseline is recorded in
[its qualification](../qualification/native-editing-2026-09-23.md). The current
design pass applies the single-Original workspace target to existing capabilities.
Its [qualification](../qualification/single-original-2026-09-23.md) records native
comparison, keyboard checks and discovered layout defects. Companion
screens remain future targets until their requirements have implementation and
evidence in [the tracker](../REQUIREMENTS.md).

Use the [UI feedback loop](../UI_FEEDBACK.md) while changing the interface. Add or
update a replay scenario through the real application and inspect its contact
sheet and affected full-size frames against these boards. Include pointer and
wheel behavior, keyboard use, focus, selection, readable values and transitions.
An unclipped screen can still have poor hierarchy or confusing feedback; review
those directly rather than treating geometry assertions as a design verdict.
Keep mode transitions, inspector descriptions, exact marker positions and command
routing testable. Measure responsiveness separately from screenshot capture.
Use native review for OS focus/IME, physical input, accessibility and physical
display behavior. Record build identity, window size, fixture, observed deviations
and checks; a screenshot alone does not establish interaction correctness.

[The Split review](../qualification/structural-split-2026-09-23.md) extends this
workspace with visible `s` hints, an inspector action and truthful Fragment cards.
The key reference supports `j/k`, arrows, page keys and Home/End, with a persistent
scrolling hint. Preserve these discoverable actions as the larger editing grammar
arrives. The saved image boards remain the composition target; this small command
addition does not replace their single-Original layout.
