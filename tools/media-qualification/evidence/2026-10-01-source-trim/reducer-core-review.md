# Ripple Trim reducer: core review

Read-only review, 2026-10-01. Reviewed `source_trim/apply.rs`, the prefix changes in
`marks.rs`, `tests/source_trim_command.rs`, and their command/sound/capture seams.
No Cargo or native execution. Root owns the running verification.

## Conclusion

No concrete correctness defect found in the reviewed reducer. It preserves the
agreed stored-mark semantics, checks wrapper/timing metadata before installing a
candidate, and returns through the existing complete reversible-patch path.
This conclusion does not replace the pending PCM and indexed-picture gates.

## Mark behavior

- `transform_marks_with_source_prefix` translates only decomposed physical content
  owned by the rebased Source. It adds the exact integer prefix before ordinary
  reconstruction, so ancestor-local and occurrence coordinates use the final crop
  and parent offsets. Other Sources, Source PTS and absolute Sequence coordinates
  are unchanged by the prefix helper.
- Existing unresolved bindings bypass transformation. A newly lost binding retains
  its original coordinate under KeepUnresolved, rather than a partially translated
  coordinate. Drop policy and multi-binding iteration still use the existing path.
- LeadingEdge/TrailingEdge remain host sentinels. The first content point at local
  zero with Right bias translates; the same zero with Left bias remains at the
  host edge. Physical Source-local points hidden behind a crop remain stored and
  bound; their visible occurrence may fail. Source PTS remains bound to the full
  measured asset. Ancestor-local and explicit occurrence paths can become lost.
- The new command tests cover these distinctions and extension without automatic
  revival. Useful additional boundary coverage, not a found defect: both biases
  exactly at a newly cropped edge, a wrapper-local interior mark through a physical
  prefix, and Drop/multi-binding behavior. These reuse established generic mark
  rules rather than requiring new Trim-specific semantics.

## Atomicity and resources

- The reducer requires the timing allocation to name the new revision, rejects a
  resolved zero, requires a wrapper exactly when requested by resolution, checks
  wrapper identity collision and node-count budget, then operates on a clone.
  An existing Partition retains its ID and changes only allocation/duration.
- One old-tree capture supplies target and suffix placements. The shared phase-only
  layout remains installed while both groups append; target rebasing happens after
  those steps exist. Limits are charged for existing entries and appended steps;
  capture checks retained timing identity and aggregate layout budgets. Final
  structural/document validation catches depth and full binding constraints.
- The admitted Source has positive duration. Framing retains its old normalization
  duration for either prefix or tail growth; gain/mute translate only for a prefix.
  The Source ID, full asset spans and independent audio offset remain intact.
- Generic command `apply` detaches/restores root sounds once, bypasses its generic
  second mark transform for Trim, then validates final duration and builds the full
  document diff. There is no intermediate authored revision. Its inverse includes
  the wrapper, original Source, effects, marks, root sound routes and binding state.
- The command tests serialize both request and transaction and assert exact document
  identity after inverse. They cover all four edge directions, crop/re-extension,
  retained wrapper identity, effect clocks, root sound routing once, stale/missing/
  extra/colliding metadata and a saturated reanchor budget without input mutation.

## Verification notes

The first compiler failure from the shared resolver extraction was an error-type
boundary issue. It is retained in the root log; the agent restored DocumentError
boundaries before this review. No passing runtime results are asserted here.
