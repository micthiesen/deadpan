# Edited slice review

## Store admission review

The initial admission allowed any exact qualified asset or accepted artifact in
the named historical revision. Review identified that a forged payload could
therefore include an exact catalog asset outside the declared selection.

Resolution: `CapturedEditSlice` retains its scratch timing identity and exposes
`validate_capture`. Before admitting historical media, the store reads the named
revision, deterministically recaptures the declared parent/range and requires
complete equality. Tests reject a valid unselected historical catalog asset,
forged receipt, wrong history and forged artifact provenance without writes.
Generic media admission remains unchanged for other commands.

## Final independent core review

Agent: `/root/edited_slice_core_review`, read-only review, no Cargo or UI.

Result: no findings in the assigned correctness scope. Review covered capture,
partial windows, clocks, repeat/lineage/alias renaming, marks, wire bounds,
command integration, sound transforms and the narrow store provenance fix.

The reviewer confirmed deterministic recapture at `deadpan-store/src/lib.rs:644`
and that nested frozen audio equality includes gap overrides and lineage.

## Parent integration review

Checked the store and CLI changes, actual decoded-PCM fixtures and scalar
references, picture-plan fixtures, command dispatch and documentation. Corrected
picture oracles before execution to round path progress to Q32 before endpoint
interpolation. Independent source coordinates and PTS were unchanged.
