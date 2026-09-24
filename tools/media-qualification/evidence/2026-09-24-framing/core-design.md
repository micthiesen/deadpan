# DP-08 core framing design

Read-only design against `643feccd6760796e34627cc7440293a9f542bf5f`, 2026-09-24. No source changes or tests were run. Repository instructions, the normative specification and handoff were read; ripwire was used before exact source inspection. This proposes an implementation boundary, not completed DP-08 or tracking.

## Implementation decision update

The parent approved and started implementation after this note. Core 18 / database 24 now take only optional per-node framing; the manual-target table and `ManualTargetSnapshot` below remain a follow-up design, not shipped schema. Public `Framing` currently contains only `value`.

The implemented numeric contract supersedes the tentative pose numerator/denominator cap below: authored pose values remain exact bounded ratios; source/owner/segment selection remains exact. Interior values use explicit Q32 round-even interpolation. Segment endpoint denominators are at most 1,000,000. A narrow checked 256-bit helper calculates the within-segment fraction without multiplying large exact time ratios in i128; then bounded i128 Q32 lerps evaluate linear/smoothstep/cubic values. Static values and exact segment endpoints return the authored pose. `FramingPose::quantized()` is an explicit helper for repeated Camera edits. There is no silent clamp or timeline rounding. Twenty-four independent Python Fraction oracle cases are retained in core tests.

The provider's own Source/Hold framing applies before its first canvas clip, so a leaf can reveal source outside the initial Fit/Fill allocation. Ancestor transforms retain intermediate clips. Unframed structural scopes remain explicit in the plan for that purpose.

The core/store implementation includes bounded wire preflight, pre-clone aggregate Subtree/Patch checks, framed-Partition preservation, framed Ungroup refusal, and narrowly admitted nested transparent Partition descent for modern framed InsertTime inputs. Native freeze insertion from an already composed framed picture is temporarily refused rather than losing its composition; a correct frozen-composition snapshot remains owed.

The real old application binary produced `store/tests/fixtures/v23-framing-history.sql`, with retained producer identity and seven revisions/three edits/pending redo. This supplements the generated strict-grammar fixtures. The initial read-only proposal follows; use source types and the update above for implementation details.

## Decision

Add one optional, durable `Framing` value to every `BeatNode`. It acts on that node's incoming composed picture, in normalized canonical canvas coordinates, after descendant framing. The operation is available on Source, Hold, Sequence, Repeat and Retime. It does not mutate source selection, audio, duration, or generation intent. Source interpretation and baseline fit precede framing.

Add a separate bounded table of named manual source targets. Picking a target resolves its source point through the actual current child-picture map and authors numeric framing plus explicit selection provenance. A saved-target reference is not a live tracking link. Editing a saved target or changing canvas aspect does not silently recenter existing effects. Canvas changes preserve the authored normalized composition; users explicitly reapply a source target when they want a fresh source-relative center. This is a coherent manual framing increment. Source-time tracking, automatic reflow of tracked targets, detections, per-play progression and the remaining picture operations stay open.

The normative basis is spec §§5.2/5.4 (scope and effect order), 6.3 (Split and Group/Ungroup preserve output), 7.6 (uncropped-source keyboard steps), 11.3 (source-based targets), 17.2 (shared effect pipeline and explicit curves), 19.3 (zoom does not regenerate footage), and 22.2 (canvas changes preserve time and reevaluate composition).

## Concrete proposed types

```rust
// New core/framing.rs. All persisted types are closed serde grammars and Eq.
pub struct FramingPose {
    pub center_x: ExactRatio,
    pub center_y: ExactRatio,
    pub scale: ExactRatio,
}

pub struct Framing {
    pub value: FramingValue,
    pub selection: Option<ManualTargetSnapshot>,
}

pub enum FramingValue {
    Static { pose: FramingPose },
    Envelope { envelope: FramingEnvelope },
}

pub struct FramingEnvelope {
    pub initial: FramingPose,
    pub segments: Vec<FramingSegment>, // custom bounded decoding
}

pub struct FramingSegment {
    pub end: UnitProgress,
    pub pose: FramingPose,
    pub curve: FramingCurve,
}

pub enum FramingCurve {
    Step,
    Linear,
    Smoothstep,
    Cubic { control1: FramingPose, control2: FramingPose },
}

pub struct ManualTarget {
    pub name: String,
    pub asset: AssetId,
    pub span: SourceSpan,
    pub geometry: ManualTargetGeometry,
}

pub enum ManualTargetGeometry {
    Point { x: UnitProgress, y: UnitProgress },
    Region {
        left: UnitProgress, top: UnitProgress,
        right: UnitProgress, bottom: UnitProgress,
    },
}

pub struct ManualTargetSnapshot {
    pub id: TargetId,
    pub value: ManualTarget,
    // Informational provenance of the application, not an evaluation dependency.
}
```

`UnitProgress` is a validated exact ratio in `[0,1]`. `TargetId` uses the existing identifier pattern in `document.rs`. Use constructors, read-only accessors and `validate()` rather than allowing invalid public construction if that matches the final module style. `ManualTargetSnapshot` copies the selected value, so a later rename/correction/deletion of the catalog target cannot mutate an already committed effect. Its ID is informational; validation checks the snapshot's asset/clock/geometry, not equality with the current catalog entry. Existing effects remain valid after target deletion. The snapshot can be omitted for free numeric framing.

The initial manual target scope is a video asset with a positive valid `SourceSpan`. Do not use `None` to invent a universal clock. Stills can later receive an explicit `Still` target scope if exposed; no need to pretend video tracking is available. A region selects its center for this zoom action. It does not imply automatic fit-to-region scale or tracking.

Preferred APIs:

```rust
impl Framing {
    pub fn validate(&self) -> Result<(), FramingError>;
    pub fn evaluate(
        &self, local: ExactRatio, duration: FrameDuration,
    ) -> Result<FramingPose, FramingError>;
}

Command::SetFraming { node: NodeId, framing: Option<Framing> }
OccurrenceEdit::SetFraming { framing: Option<Framing> }
Command::SetManualTarget { id: TargetId, target: ManualTarget }
Command::DeleteManualTarget { id: TargetId }
```

Core receives resolved numeric framing. It neither fabricates source geometry nor owns GPU/decoder handles. A host/plan resolver uses an immutable revision and the current selected picture identity to turn a selected manual point and source-axis keyboard delta into canvas coordinates. Commit `SetFraming` with the usual revision guard. Camera temporary edits stay in an explicit preview document; Enter submits once, Escape restores entry, and switching target creates a new explicit value/envelope change.

## One exact time host

Use only owner-output progress in this increment, not a general LocalFrames/HostProgress framework. Let `local` be the exact coordinate at the node before descending to its child. Let `D` be that node's current output duration. Evaluate at `u = local / D`. This is frame-edge progress: the node interval is `[0,D)`, stored curve endpoints are at 0 and 1, and ordinary picture samples occur at frame centers. Thus a two-frame creep samples at progress 1/4 and 3/4, rather than inventing a special first/last-frame rule. That convention must appear in tests and inspector documentation. An empty Sequence has no evaluated frames; avoid division by zero by never evaluating its curve.

Segments start at 0 or the previous segment's end, end strictly increasingly, and the final end is exactly 1. Within one segment, normalize the exact local progress to `v` in `[0,1]`. Step holds its starting pose until the next segment boundary, with the new pose selected at that boundary. Linear uses `v`; smoothstep uses `3v²-2v³`. Cubic uses ordinary cubic Bezier value interpolation with the two explicit pose controls and linear time parameter `v`. There are no implicit CSS-style x handles or iterative inverse roots. Evaluate each pose component independently. Bounded positive scale controls keep a cubic inside the allowed convex hull.

Checked `ExactRatio` arithmetic preserves exact selection and interpolation where representable and explicitly reports overflow. Do not approximate timeline coordinates or silently change the curve to recover. Existing exact structural mapping already has this error boundary; add large-ratio tests so the new cubic operations do not panic. A future fixed-precision numeric evaluation scheme would need its own explicit semantics and parity tests, not an unnoticed fallback.

Timing consequences:

- A framing value inside Repeat restarts on every default/override play. A value on the Repeat spans its total current duration, including gaps. It is evaluated before the `layout.locate` branch can terminate in a gap.
- Framing on Retime uses Retime output progress. Child framing uses the exact mapped input coordinate. Source playback mapping does not reanchor owner-local editorial framing to source time.
- Changing a Hold duration or Repeat play count intentionally stretches an envelope spanning that host. Moving it preserves the curve. Wrapping it in a new Repeat leaves its own curve intact and restarts it each play; an effect explicitly put on the new Repeat spans the entire passage.
- A later fixed-time partial envelope requires a separately declared time basis. It is not needed to ship whole-host punch/creep/Camera or to preserve a pure Split.

## Spatial evaluation and clipping

For canvas size `C = (Cw,Ch)` and child canvas position `p`, each layer is:

`q = C/2 + scale * (p - C * center)`.

Identity is center `(1/2,1/2)`, scale 1. Apply leaf first, then ancestors. Different centers make composition noncommutative. Retain each intermediate canvas clip. Multiplying all affines while dropping those clips lets a later zoom-out reveal pixels a descendant crop already excluded. The plan should return a bounded ordered list of evaluated poses and owner identities; the renderer owns the shared forward/inverse geometry and clip intersections, used by preview and future export.

Manual target coordinates are in the upright, SAR-corrected, uncropped source plane, x right/y down. With upright source dimensions `D` and existing baseline Fit factor `k`, an unframed source point maps to `C/2 + k*(D*point-D/2)`. Further descendant transforms must be included when resolving a target for an ancestor. For Camera h/j/k/l, project a 1% source-axis displacement through that same pre-operation child map, not 1% of the current cropped image. Uppercase uses 5%; scale steps use 21/20 or its reciprocal with count semantics. Clamp or reject only by an explicit documented authoring policy, with feedback, not a hidden renderer clamp.

Resolve against the actual selected source frame, including endpoint holding. `Picture::Source.point` may lie outside the selected span while endpoint holding shows a valid final selected frame, so it is not by itself sufficient evidence for a displayed target. Background/no-picture and foreign-asset targets return a useful unavailable result. A target outside the child clip must be visibly unavailable or explicitly acknowledged as offscreen, rather than silently mapped to a different subject. Accepted generated Holds retain numeric framing after provider replacement; the old Original target snapshot remains provenance, not a false claim that the generated asset contains a tracked target.

Canvas edits reevaluate the same normalized affine and the baseline fit under new dimensions. They retain normalized authored composition, not a promise to preserve a previously selected source point. Explicit target reapplication is the supported recentering operation. New Fit/Fill authoring is a separate composition policy; do not silently change baseline Fit merely because `scale > 1`.

## Structural lifecycle

`split.rs::apply` already retains complete child subtrees and exposes them through transparent Partition wrappers. Keep framing on those full retained owners; new Partition wrappers are unframed. A root split must move the old root framing onto its retained full context, leaving the replacement outer Sequence unframed. This preserves every original curve evaluation and avoids inventing clipped cubic keys.

The current refinement branch recognizes every `RetimePurpose::Partition` by kind alone. It must additionally require that the wrapper has no framing. Re-splitting a framed Partition must retain that whole wrapper as the meaningful effect owner and place new partitions outside it. It is acceptable to retain one layer for a real authored effect scope; do not grow layers for repeated splitting of unframed transparent wrappers. Tests must cover a non-root Partition and the Sequence-root special case.

`occurrence_edit.rs::clone_nodes` clones whole BeatNodes and remaps owned structural children. Node-local framing therefore follows Split/isolation/copy with no external effect-owner table. Snapshot target provenance copies by value. Sparse override editing should use the existing `OccurrenceEdit` isolation transaction, not a numeric play ordinal.

`command.rs::prepare_subtree` renews imported Repeat allocations and remaps overrides. Numeric envelopes and value snapshots require no Repeat-ID remapping. Do not add a hidden live target dependency merely to make copying easier. Source assets remain shared immutable inputs.

`Ungroup` currently removes the Sequence and already refuses explicit audio edge choices. It must at minimum refuse a framing-bearing Sequence with a precise explanation; silently discarding its curve violates §6.3. Full ungroup of a framed group is still owed: it needs effect distribution with retained group clock and clipping, which this simple one-layer schema does not automatically supply. Do not claim that refusing it completes the normative Group/Ungroup contract. Ordinary unframed Group/Ungroup remains unchanged.

Review InsertTime's support classifier when framing is present. The existing pure Split machinery can retain Source/Hold picture curves, but a newly inserted Hold has its own numeric framing choice; copying a pose at the insertion boundary versus leaving it unframed must be an explicit command policy. Do not accidentally inherit an outer root effect twice. Arbitrary nested/gapped InsertTime remains owed independently of this feature.

## Proposed quantitative admission limits

These are initial engineering bounds, not measured performance results:

- 4,096 named manual targets; names use the existing bounded label policy.
- 64 segments per envelope; static framing costs one unit, an envelope costs one plus its segment/control records; at most 100,000 aggregate framing/target records per document. Charge before cloning or extending buffers.
- At most 16 active framing owners on an owned root-to-leaf path, including override branches; validate iteratively alongside the existing 256 structural depth bound. This is an explicit additional bound, not an unbounded per-frame stack.
- Pose centers in `[-16,17]`, positive scale in `[1/64,64]`; controls use the same bounds. Saved target points/regions remain inside `[0,1]`, with strict positive region width/height. No NaN/infinity exists in exact persisted numbers.
- Bound authored rational numerator magnitude to 64,000,000 and denominator to 1,000,000. Native numeric entry/resolution should expose its chosen precision rather than sending arbitrary float noise. This is a proposed cap requiring implementation confirmation; checked rational overflow remains an explicit query error.
- Keep the existing 64 MiB document wire cap. Add bounded collection decoding before materializing segment arrays, duplicate-key rejection for targets, and cumulative record admission. Per-vector caps alone are insufficient.

The render author should confirm cumulative matrix bounds and finite f64-to-f32 conversion for these limits. A mathematical per-layer bound is not a tested GPU guarantee. Reject unsupported matrices explicitly, never omit a layer or clamp the transform silently. Before reducing limits for convenience, retain a concrete counterexample or performance measurement.

## File and symbol changes

| Area | Concrete change |
|---|---|
| `core/src/framing.rs` (new) | Closed types, constructors, validation, bounded segment decoding, pure evaluator and target validation helpers. |
| `core/src/document.rs` | `TargetId`, optional `BeatNode.framing`, `ProjectDocument.manual_targets`, strict `DocumentWire`, accessors, defaults, aggregate validation using existing structural durations; constructors default to no framing. |
| `core/src/command.rs` | New setters, `DocumentPatch.manual_targets` before/after map, inverse/apply/diff bounds, descriptions. Framing changes appear in normal node diffs and changed IDs. Do not mutate immutable assets. |
| `core/src/occurrence_edit.rs` | New typed occurrence setter and exhaustive mapping; retain whole-node copy behavior. |
| `core/src/split.rs` | Refinement must not bypass framed wrappers; new wrappers identity, root metadata transferred once. |
| `core/src/lib.rs` | Focused exports plus new frozen legacy adapter registration. |
| `plan/src/plan.rs::RenderPlan::picture` | Capture each owner-local exact coordinate before descent; evaluate framing at Source/Hold/Sequence/Repeat/Retime, including Repeat gaps; reverse the collected layers for leaf-to-root output. Keep compact layout lookup. |
| `plan/src/picture.rs::PictureSample` | Optional/empty-skipped evaluated framing layers with owner identity and exact pose; do not change source frame lookup semantics. |
| `plan/src/plan.rs` compile/inspection | Retain validated framing, report it in inspection, bound work/depth; no media work. |
| `store/src/generation.rs::apply_relevance_plan` and host context resolver | A framing-only edit still supplies complete relevance observations but keeps generation context unchanged. Do not hash the entire newly changed BeatNode as generation input. |

## Strict core 18 / database 24 migration

Current `DOCUMENT_SCHEMA_VERSION` is 17 and `schema::VERSION` is 23. Introduce a frozen `legacy_v17.rs` before widening current types. It must retain today's vocabulary including `InsertTime`, existing audio bindings, generated providers, source mappings and strict nested node grammar, while rejecting all framing fields, manual-target fields and new commands, including explicit empty values. Merely deserializing old nodes through widened `BeatNode` is insufficient.

Upgrade old documents to empty target tables and absent framing. Projection/matching of a modern document or edit into legacy v17 must reject newly authored framing/targets. Existing legacy adapters that construct BeatNode/ProjectDocument need explicit empty fields; their project functions must not accept modern framing by dropping it. Old commands still calculate old behavior when their input has no new data. Do not weaken any adapter's nested provider or Split grammar.

Extend `store/src/validation.rs` with `ReplaySchema::V17`, `StoredDocument::V17`, database 23 dispatch, strict request upgrade and exact edit/snapshot comparison. Extend version admission in `schema.rs` and `migration.rs` to include 23. Use the existing backed-up candidate replay and SQLite backup promotion, preserving every revision branch, history request, cursor, redo stack, workflow profile, allocation reservation, source receipt and operational generation row. There is no need for a new operational SQL table for authored manual targets; the document/history JSON owns them.

## Decisive tests and limits of the first slice

1. Pure evaluator: all four curves, exact segment boundaries, frame-edge convention, one-frame host, zero-duration inert owner, positive cubic hull, bad ordering, duplicate/unknown fields, aggregate bounds and checked overflow.
2. Every node kind including a Repeat gap; default per-play restart versus a Repeat-owner whole-passage creep; sparse override local duration; nested Repeat with huge compact play count and bounded lookup work.
3. Fractional Retime before/after framing; pure Split before/after equality at every picture frame; re-split framed Partition; root split; outside ancestor framing applied once; copied/isolated curves independent.
4. Duration setter deliberately reflows owner-progress creep. Source mapping changes preserve owner editorial clock. Move preserves pose sequence. Ungroup must not lose framing.
5. Target point/region bounds, foreign asset/time-base, actual held endpoint frame, rotated/SAR source steps, offscreen/clipped target, catalog edit/delete not mutating snapshots, canvas aspect change retaining normalized composition, explicit reapply restoring target center.
6. Geometry: identity baseline, two different centers proving ordered composition, inner crop then outer zoom-out proving retained clips, non-square canvas, odd source dimensions, baseline Fit, generated provider replacement retaining numeric framing.
7. Commands: atomic stale rejection, inverse equality, undo/redo fresh revisions, occurrence isolation, target-table patch conflict, no audio mutation and unchanged generation relevance.
8. Migration: real database 23 fixture from the frozen old binary, all snapshots/transactions/redo compared; old grammar rejects framing in documents, patches, inserted subtrees and occurrence commands; failure leaves original and backup intact.

The smallest honest vertical slice is editable static punch and whole-host smoothstep creep, all four supported curve types through the headless setter, keyboard Camera with manual target creation/reapply, real shared plan/render geometry, and durable history/migration. It may expose selected root beats first without weakening core scope. It is not detection/tracking, per-play escalation, full framed Ungroup, fixed-time attachment envelopes, or completed DP-08/export qualification.
