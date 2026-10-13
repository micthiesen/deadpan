# Owned Preserve inputs

Core 55/database 83 adds `DerivedRetimeClock::preserve_input`. It retains the
complete operand of one nonunity Preserve stage while its current child grows
around inserted time. It stores current-tree selectors and exact coordinates,
never old source recipes, detached document snapshots or prepared PCM.

`PreserveInputProjection` partitions the original integral `mapping.duration()`
from zero into positive, contiguous exact runs. Each run maps an input interval
to a source interval. `Current` selects a descendant's current output;
`Intrinsic` selects a descendant Preserve stage's retained intrinsic output
before that stage's separate splice allocation. Both carry an `InstancePath`
relative to the owning stage. Repeat entries must name the actual default,
sparse play override or owned gap branch for that stable iteration.

Admission proves strict descendant ownership, provider existence and source
bounds. A Current target cannot contain another owned projection; callers must
select that stage's Intrinsic output explicitly. This makes the input graph
acyclic through ordinary tree ownership. Inputs require 1–4,096 runs each,
at most 65,536 runs per document, bounded relative paths and at most one million
ancestor steps during ownership validation. Unknown/duplicate wire fields and
unrepresentable run slopes fail. The original Preserve pitch, rate and integral
duration remain authoritative for processing.

`RenderPlan::owned_preserve_projection` resolves the saved selectors into checked
`AudioStageProjection` handles. The compiler reads current recipes from the
immutable plan and memoizes nested stage occurrences. It bounds compilation to
64 stages, depth 64 and the existing projection edge budget. The complete input
tape shares one PointCeil sample grid. Run seams do not concatenate independently
rounded sample counts or introduce new source-filter boundaries. Output policy
uses a separate tape obtained by dividing each input destination by the original
stage rate. Explicit silence is therefore evaluated on the intrinsic output grid
even when the corresponding interval has no input-grid point.

An enclosing stage consumes a nested intrinsic projection, not the nested
stage's longer current allocation. Request-local preparation reuse requires the
same immutable plan identity, definition scope and relative occurrence path.
Rebuilding an equivalent handle reuses PCM after rechecking source admission.
A new plan with changed descendant recipes cannot reuse it, even when project,
revision and stage descriptors match. General caller-built projections retain
their opaque identities.

Snapshot restore, reversible clock patches and SQLite history persist these
inputs. Ordinary reducers and the native workspace still refuse derived-clock
documents. Shift, marks, sound metadata and retained audio bindings/lineage
remain outside derived-clock admission. All Preserve stages in such a document
currently require owned inputs. Prior unused development schemas are refused
under the owner's format-break authorization.

This is intrinsic preparation support. It does not author an insertion or
schedule the resulting timeline output. Structural root/definition audio
queries, flattened queries, independent occurrence voices through a projected
owner and a projected stage's legacy child-input signal
fail explicitly where they would otherwise substitute changed structural input.
The separate output route must retain each occurrence's physical sample clock,
resume index, envelope and silence policy. In particular, root RoundEven
allocation and intrinsic PointCeil sampling cannot share a rounded frame shift.
Later edits still need atomic route/lifecycle transforms, including effects,
copy, isolation, split, trim, deletion, movement and duration changes.

See the [splice design](STRUCTURAL_SPLICE_DESIGN.md#preserve-input-projection),
[input tapes](AUDIO_INPUT_TAPES.md), and
[projected root placement](AUDIO_PROJECTED_ROOT.md).
