# Captured framing for pauses

This work implements the bounded capture path. It does not complete arbitrary
insertion, generated-media preview, saved targets or the complete picture-operation surface.
The [full specification](spec/DEADPAN_SPEC.md) remains authoritative.
The [qualification record](qualification/captured-framing-2026-09-26.md) distinguishes
deterministic verification from the still-blocked Metal and native visual review.

A pause captures the existing picture's exact source frame and the spatial
composition entering the insertion parent. The source remains a `HoldVideo::Freeze`
with its registered asset, measured PTS and time base. Captured geometry belongs
to `HoldRecipe.picture_context`, separately from the video provider and from the
new Hold's ordinary editable framing. No preview pixels, texture handles, paths
or decoder state become authored data.

## Ownership

Insertion captures every sampled scope below its actual Sequence parent. The
shared `insert_time_target` query chooses that parent before native capture.
Root seams remain root insertions; strict interiors can descend into unretimed
Sequence groups. Capture excludes the parent's framing and all its ancestors:
the inserted Hold inherits those live operations once.
An inherited envelope keeps its owner clock and stretches with the owner's new
duration. A captured descendant envelope becomes its static value at the sampled
frame center. This distinction preserves group ownership and avoids applying the
same inherited effect twice. Repeat and Retime ancestor insertion still requires
the separate occurrence and exact derived-clock authoring work.

The new Hold can be reframed independently. Camera and its reset operate on that
new framing; the captured view remains its input. Resetting Camera does not recover
pixels removed by a captured child crop. Provider changes, generated acceptance,
fallback restoration and duration edits preserve the context. Whole-recipe gap
replacement intentionally replaces it along with the rest of that recipe.

Separating context from provider matters for generation: accepted footage replaces
the freeze before editorial framing, and uses the same retained spatial operations.
It must not lose its composition when accepted or double-apply it on reversion.
This data model does not itself qualify app generation or generated-media decoding.

## Spatial recipe

`CapturedFraming` is a bounded, nonrecursive list of captured canvases. Each canvas
records its dimensions, explicit Fit/Fill policy and ordered static framing layers.
Every layer clips its result to that canvas. A `None` layer is an identity clip;
an empty layer list means one identity clip. Explicit identity poses remain distinct
from absent operations so their declared evaluation path is retained.

The first canvas fits the interpreted source. Each later canvas fits the previous
whole canvas, including black areas, then applies its operations and clips. Finally,
the retained last canvas fits into the current project canvas before the new Hold
and ancestor framing. Changing project aspect therefore places the frozen
composition as a whole rather than revealing previously cropped source pixels.
The preview raster does not define any captured coordinates.

Recapturing at the same canvas with Fit concatenates ordered operations after the
already established clip. Adjacent absent clips can coalesce. Recapturing an
unchanged freeze does not grow a chain of snapshots. Nontrivial transforms and
intermediate clips are never flattened away or silently quantized.

The limits are 32 canvases, 512 scopes and 256 nonempty poses per context,
plus 100,000 aggregate records per document. Canvas and pose values retain core
validation. The renderer also checks its qualified dimensions and finite,
nondegenerate cumulative geometry before upload; collection counts alone cannot
prevent floating-point overflow or underflow.

Typed command payloads receive the same checks before copying their recipes,
including inserted subtrees, override subtrees and Repeat gaps. Each incoming
subtree and the validated existing document has a bounded aggregate budget;
final reduction checks the resulting combination so replacement can retire old
context. Patches check their virtual resulting node set before copying, and
occurrence isolation checks each duplication before allocating its recipes.
Isolation permits at most two document budgets in its private intermediate state,
so clearing or replacing copied context can return to the ordinary budget. The
committed result, patches and public validation always enforce 100,000 records.

## Storage and verification

The new core-19/database-25 vocabulary requires a frozen core-18 adapter. Older
Hold grammars reject `picture_context`, including an explicitly null field, in
snapshots, commands, subtrees and patches. Missing context preserves the earlier
meaning and serialized shape. The migration fixture was produced by the actual
core-18/database-24 executable and retains a camera curve, eight revisions and
pending redo. Audio-only retained contexts and source receipts gain no new fields
or dependencies from spatial capture.

Deterministic checks cover ordered crop retention, exact frame identity, inherited
root motion, independent Camera edits, provider/acceptance lifecycle, constant-size
unchanged recapture and strict legacy history. Actual shared CPU/Metal comparison
and native review of the resulting freeze and visible Camera explanation remain
required. This contract is not acceptance evidence.
