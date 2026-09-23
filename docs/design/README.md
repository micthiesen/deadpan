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

The built-in `image_gen.imagegen` tool generated these assets on 2026-09-23.
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

| State | Visible information | Behavior |
| --- | --- | --- |
| Normal | Mode, Original/Your edit context, pane, selected beat and scope | `h/l` move frames; `j/k` move the focused list; counts precede motions; `Tab`/`Shift-Tab` cycle visible panes. In Your edit, beat actions require an editable selection. |
| Operator pending | Exact typed prefix and current scope | No timeout. `3rr` means three total plays. Escape cancels the pending input. |
| Command/text | Focused field, command reference and units | macOS editing and IME own keystrokes. Text never dispatches structural shortcuts. Same-frame text is processed before submission. |
| Parameter entry | Selected node, current value and command | Current native setters use the shared command entry. Return commits a validated command; Escape cancels entry. This is not a live parameter preview. |
| Temporary preview | Proposed change clearly identified | Intended Camera/Trim contract: Enter commits once; Escape restores entry state. Remains unimplemented until preview transactions exist. |
| Candidate | Committed provider beside candidate status | Ready does not change the edit. Explicit acceptance is undoable; Escape never rolls back a previously committed edit. |

Original is the Source context; Your edit is the Sequence context. Original
browsing remains non-destructive. Sequence root-beat actions show their
scope explicitly. Full selector, occurrence, Camera, Trim, Visual and macro
workflows remain required and must use the same typed command boundary as the
implemented actions. Discoverable controls refer to actual bindings. Standard
macOS focus, copy/paste, composition and logical-key behavior take precedence
while text is active.

## Reviewed concept corrections

Generated visual content is a composition reference. The following generated
details are intentionally not implementation instructions:

- Use the implemented binding table and normative grammar. `?` and `:help` open
  actual help. The product board's Original-view footer still includes `rr`/`dd`;
  the coded Original view must instead teach browsing, return to Your edit and
  explicit reuse. Structural edits never target the immutable Original.
- The enlarged workspace's `y` reuse label describes future range copying.
  Current whole-original reuse uses `⌘Return`; no inactive range-copy control
  should imply that `y` already works. New starts with the original already inserted.
- Thumbnails, filmstrips, waveform selection, playback, recent projects, YouTube
  acquisition and advanced workflow actions require actual supporting data and
  behavior. Use truthful media-type tiles until a bounded thumbnail service exists.
  Do not turn the stopped-frame viewer into a pretend transport.
- Sound import only registers audio today. The Place a sound panel is the target
  for a real anchored overlay, never an audio-only sequential beat with blank picture.
- The workflow board shows intended live Camera/Trim preview. Current Hold and
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

The initial native root-editing baseline is recorded in
[its qualification](../qualification/native-editing-2026-09-23.md). The current
design pass applies the single-Original workspace target to existing capabilities.
Its [qualification](../qualification/single-original-2026-09-23.md) records native
comparison, keyboard checks and discovered layout defects. Companion
screens remain future targets until their requirements have implementation and
evidence in [the tracker](../REQUIREMENTS.md).

Keep mode transitions, inspector descriptions, exact local marker positions,
visible-pane cycling and command routing testable headlessly. Use live native
review for visual comparison, focus/IME, keyboard navigation and accessibility.
Record the build identity, window size, fixture, observed deviations and actual
checks; a screenshot alone does not establish interaction correctness.
