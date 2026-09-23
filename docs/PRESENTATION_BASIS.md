# Automatic presentation basis

New automatic projects start with a provisional 1920×1080, 30 fps SDR black
canvas. The first primary moving-picture insertion can establish the measured
source cadence and display geometry while the project remains untimed. The host
chooses that rate before deriving source placements and rounding the enclosing
beat. Browsing, retaining, registering and source-clock annotation do not select
the basis. Existing explicit-basis projects retain their chosen values.

## Authored state

Core schema 10 persists `basis_state` beside `presentation_basis`:

- `rate_origin`: `provisional`, `explicit`, `timed_edit` or `primary_source`.
- `geometry_origin`: `default`, `explicit` or `primary_source`.
- `primary`: the first primary picture's asset ID and immutable qualification ID,
  or null before one has been inserted.

The source qualification binding survives removal of its beat. A later reaction
or differently shaped source cannot replace that initial choice. Historical
lookup still uses the selected revision's asset and receipt. Project dump and
validation output expose the state; the native document inspector remains open.

The first actual time-based edit fixes the provisional rate. This includes
inserting audio, a Hold or secondary picture, and setting a mark in project,
local or occurrence coordinates. An initial audio insertion is an authored
time-based edit: later picture preserves its 30 fps clock and sample boundaries.
There is no retrospective audio rebase. Labels, asset registration, source-clock
marks and genuine no-ops do not lock the rate. Deleting all beats does not make a
project provisional again. Undo can restore the actual pre-edit provisional
state, with a fresh store revision as usual.

Document validation rejects a provisional state containing positive-duration
structure or project-time marks, nondefault presentation values, or a primary
source. A source-derived origin requires a matching qualified picture asset.
Default geometry must retain 1920×1080 and an automatic project origin; a
timed-edit rate must retain the locked 30 fps default. Store validation additionally
checks claimed source-derived rates and geometry against measured receipt
candidates across every historical revision.

`DocumentPatch.presentation` carries one before/after pair containing both basis
and state. Its preconditions and inverse apply them atomically with nodes, marks,
assets, history and generation relevance. Failed insertion or database writes
leave the old basis and state intact.

## Primary and secondary source intent

`SourceInsertionRequest.purpose` is `primary` or `secondary`. Normal structural
insertion defaults to primary; supporting/reaction material explicitly selects
secondary. Mere presence of video, registration order, names, or source dimensions
never identify the primary. Audio and still images cannot choose a video cadence.

For the first primary moving picture in a provisional project, the host derives
the qualified cadence and display raster and supplies `PrimarySourceImport::Adopt`
to the shared command path. At a fixed rate it supplies `KeepBasis`: the primary
identity is recorded, while the existing rate and dimensions remain unchanged.
Subsequent insertions preserve the basis and first-primary record.

The media candidate policy preserves rational rates, derives cadence from actual
PTS intervals, applies SAR/rotation, and rounds to legal even dimensions within
the recorded error tolerance. Ambiguous cadence fails automatic selection rather
than inventing a rate. Sources can still be inserted into an explicitly fixed
project rate. See [measured timing](SOURCE_IMPORT_TIMING.md) for its qualified
scope, VFR policy, and high-rate divisor rules.

## Explicit geometry changes

`SetCanvas { width, height }` changes the authored canvas, keeping frame rate,
source mappings, nodes, marks and temporal coordinates fixed. Dimensions must be
positive even values within project limits. An explicit canvas choice in an
otherwise provisional project fixes its existing rate and records explicit
origins, so a later unrelated import cannot replace it.

`ProjectStore::preview_primary_geometry` derives the recorded first primary
source's geometry independently of cadence. `adopt_primary_geometry` commits that
proposal as a separate undoable transaction. It changes neither rate nor time.
The source qualification already retains the interpretation, so geometry can be
previewed when the original is offline; this makes no availability or playback
claim. Current generation requests still require complete host relevance context.

Generic commands cannot introduce source-derived primary choices or claim
qualified primary geometry. The private host admission binds their values to
the receipt. The generic `set_canvas` command remains available for a deliberate
creative canvas choice. [Headless commands](HEADLESS.md#presentation-and-canvas)
document the developer entrypoints and preview protocol.

## Compatibility and remaining work

Database schema 18 migrates schemas 1 through 17 into core schema 12. Schema 15
retains its existing basis state and presentation transactions through frozen
core 10. Projects predating schema 15 become explicit, including empty projects
whose dimensions match the default. Nothing proves they were provisional. Frozen core-9 history preserves
qualified source receipts and imports without accepting the new primary intent,
basis-state, geometry commands or presentation patch fields. Existing source
qualification rows and all branch references survive unchanged.

[Qualification](qualification/presentation-basis-2026-09-21.md) records real-media
insertion, unchanged audio/mark coordinates, CLI previews, rollback and migration
evidence. This is authored/headless behavior. Native project UI, visible preview
of canvas changes, framing-effect reevaluation, the full source format/color
matrix, playback, and rendered-file equivalence remain open.
