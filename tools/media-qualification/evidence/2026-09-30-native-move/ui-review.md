# Native Move UI production review

Reviewed current shared changes against `21e714f` and the agreed native Move
design. Read-only repository inspection with ripwire, exact Git diffs including
the new comparison file, and direct reads of adjacent transport/presentation
contracts. No edits, Cargo, decoder workers or GUI interaction were performed.

## Findings

Two actionable P2 findings were sent to root and corrected during this review.
Both corrections were inspected on disk. Their regression execution remains
root-owned; this report does not claim those tests passed.

### 1. A receipt delivered after its workspace could lose destination scope

Original changed `preview.rs` restored `commit.scope` only when the workspace
revision/session changed. If a new workspace arrived first and the receipt
arrived later, matching completion advanced `last_committed` using rows from the
old navigation scope. A cross-parent Move could then select only an enclosing
group, and `select_committed_range` rejected the range's different parent. The
receipt was consumed, so later updates could not repair the selection.

**Correction inspected:** matching unconsumed non-sound receipts restore scope
independently of the workspace transition; `restored_scope` also triggers row
rebuild before node/range selection. Root requested a cross-parent
workspace-before-receipt regression in addition to existing delivery orders.

### 2. Comparison changed a legal stopped terminal sample by one

Original changed `Draft::compare` clamped every mapped `draft.position` to
`target_window.end - 1`. Natural audition completion retains the valid boundary
sample `window.end`. Even an ownership-only Move, whose comparison must use
identical global samples, mapped `B(N)` to `B(N)-1`. The frame cursor remained N,
leaving frame/sample state inconsistent and making a later audition replay the
last sample instead of following existing end-of-window handling.

**Correction inspected:** the clamp now includes `target_window.end`, preserving
legal stopped boundary coordinates while bounding genuinely outside values.
Root requested ended/paused terminal witnesses for ownership-only and temporal
Move comparison. Picture requests independently clamp N to picture N-1.

## Other inspected behavior

- Comparison pairs and directional context caps match the design's before/after
  table. All sample boundaries derive from absolute frame coordinates, and the
  offset mapper uses checked arithmetic. Empty seams map to counterpart starts.
- Boundary reparenting uses identical affected intervals and sample positions,
  with explicit unchanged-timing text, rather than inventing a removal gap.
- Removal/insertion site changes reset local cursor/transport state without
  advancing proposal identity. Source/destination/operation edits advance the
  proposal and preserve the retained insertion target across Replace toggles.
- Service replies require the matching pending proposal; copied source endpoint
  identity is checked separately. Prepared forest validation replaces the old
  single-root-duration assumption. Existing sealed media checks remain present.
- Successful draft closure leaves range installation to generic receipt handling.
  Matching Move completion checks revision/project/session; duplicate completion
  does not reselect after navigation. Saved-refresh failures suppress completion.
- Main picture labels/accessibility still come from accepted presentation, and
  source endpoint identity remains independent of destination/site changes.
- New m/s routing preserves modifier, repeat, IME, native button and background
  focus rules. Pointer actions use the existing discarded-layout retry path.
- Footer variable text is measured before viewer allocation and reuses those
  galleys. Two site strips expose saved/proposed clocks without one giant loop.

## Scope and limits

Inspected `navigation/splice.rs`, `preview/splice.rs`, new
`preview/splice/comparison.rs`, `preview/splice/controls.rs`,
`preview/edit_range.rs`, generic completion in `preview.rs`, the new receipt
match helper in `preview/selection.rs`, and relevant existing playback and
picture admission code. No additional actionable production defect found after
the two corrections.

Actual 960x640/default layout, paint clipping, Metal readiness, native input and
the complete keyboard/media path still require the root-owned harness and native
checks. Static review and unit test source do not establish those results.

## Final read-only review

The same reviewer inspected the final production delta after the comparison and
receipt corrections. One P3 remained: the Unicode arrow in the Move timeline
labels rendered as a missing-glyph square in captures132/133. Root replaced it
with `to`, updated the exact accessibility assertions, reran visual-final and
inspected both final sizes. No other actionable finding was returned.
Execution results belong to the separate final gate and replay records.
