# Captions

A caption is a line of text drawn over part of a beat (specification §8.2
"Delayed caption", §3 attachments). It is optional, editable and changes no
timing, picture provider or sound.

## Model

`BeatNode.captions` holds up to 16 [`Caption`](../crates/deadpan-core/src/caption.rs)
records on a Source or Hold, sorted by start:

| Field | Meaning |
| --- | --- |
| `range` | Half-open project frames in the host's own output clock. A caption longer than its host is clipped to it. |
| `text` | One line, 1–120 characters, no control characters or surrounding whitespace. |
| `placement` | `bottom` (default, omitted on the wire), `top` or `center`. |
| `reveal` | Optional first play position, from one, of the innermost enclosing Repeat that shows it: per-play reveal. It counts plays in their current order, not stable play identities, so reordering or resizing the Repeat changes which play reaches it; "from the third time" stays the third time. |

Captions at one placement on one host do not overlap; a top and a bottom
caption may. Like [cutaways](CUTAWAYS.md), captions live on the host whose
local clock is its content's clock, so moving, copying, splitting, occurrence
isolation and deleting the host carry them, and Source Trim and Roll shift them
with the content when the physical Source gains a prefix. `SetCaptions { node,
captions }` replaces a host's list as one reversible edit; it is admitted
alongside beat-owned sounds and compound transactions. A caption that starts
after the beat's first frame is a delayed caption. The field is additive
within core schema 46.

## Plan and rendering

`RenderPlan::picture` collects, for every visited host, the captions whose
range holds the picture center and whose reveal is reached in the play being
shown (`PictureSample.captions`). Picture identity, framing and decoding are
unchanged, so caches and cutaways are unaffected.

[`CaptionOverlay::rasterize`](../crates/deadpan-render/src/caption.rs) turns
the lines into fill and outline coverage at the exact render-target raster:
glyph outlines from the bundled Inter variable font (OFL,
`assets/brand/source/Inter-Variable.ttf`) at weight 700, drawn with exact
signed-area accumulation in a fixed order. The text is 6% of the canvas height,
centered, 6% from the top or bottom edge (or centered vertically), shrunk to fit
90% of the canvas width, inside the canvas as fitted into the target. The
outline is the fill dilated by 8% of the text size. There is no wrapping,
kerning or complex-script shaping; characters the font lacks draw its
missing-glyph box. Style identity: `inter-4.001-wght700-6pct-white-on-dark-outline-v1`.

`PictureRenderer::render_composed_captioned` and
`render_background_captioned` add one composite pass after framing and before
both the display transform and any working readback: premultiplied white fill
over a black outline in linear working light, keeping the working alpha at 1.
The native viewer, the export picture session (and therefore the render worker
and `verify-export`'s reference) use the same function, picture sample and
target raster rule. A captioned Background or Blank pause renders through the
pass too; an uncaptioned one still needs no target. Thumbnails and Splice
junction previews do not draw captions.

A caption usually spans many frames. Export keeps the last raster
(`deadpan_cli::picture::CaptionMemo`, keyed by lines, canvas and target) and
rasterizes again only when one changes, and each overlay carries an identity
of those inputs so `PictureRenderer` uploads its pixels only when that
identity differs from the texture it already holds. Results are identical;
the font is shipped with its OFL notice in the bundle notices
(`inter/OFL.txt`) and as an `OFL-1.1` file component in the SBOM.

## Commands

`:caption TEXT [at=bottom|top|center] [delay=12f] [reveal=3]` captions the
selected beat, or the Edit range inside it (then `delay=` is refused). Quoted
text keeps option-like words; unquoted text is its words joined by single
spaces before any trailing options. A Split fragment places the caption on its
Source in the Source's clock; groups, Repeats and speed changes are refused
with guidance. `:caption clear` removes the captions overlapping the range or
beat. The inspector lists a beat's captions, including a fragment's Source
captions over its own frames. Recording a macro records a whole-beat caption as
`SetCaption { text, placement, delay, reveal }`, which resolves the selected
beat when replayed; a ranged caption refuses while recording.

## Tests and evidence

- Core: reveal and range rules, prefix shift, validation of text, order and
  overlap, wire form; `SetCaption` on the selected beat after its delay,
  overlap, late-delay and unselected refusals, and invalid text.
- Plan: a delayed caption in both plays of a Repeat and a revealed one only in
  the second, with every picture and framing unchanged.
- Render: placement, outline around fill, determinism, fitted scaling,
  long-line shrink and the missing glyph; on Metal, white fill (luma 233+) and
  dark outline (luma 18-) in the encoder pixels over a gray picture and over
  black, the picture untouched elsewhere, and a mismatched raster refused.
- Replay `captions`: `:caption Are we done? at=top delay=4f` on a split
  fragment, the inspector row, no caption before the delay, white text with a
  dark outline read back from the actual viewer target after it (compared with
  the same frame after `:caption clear`), Undo twice, and a caption recorded in
  macro `c` and replayed on another beat.
- Export (release): `delayed-caption` captions an Original beat after a delay
  and a black pause; the movie matches its preview, and verifying it against a
  later revision without the first caption flags exactly the captioned frame.

## Remaining

Captions over groups or across beats, multi-line wrapping and styling, kerning
and complex-script shaping, captions in thumbnails, an occurrence-specific
caption made inside one Repeat play from the native scope, and pointer editing.
