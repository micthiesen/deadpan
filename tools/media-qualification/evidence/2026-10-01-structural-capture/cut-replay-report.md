# Cut register replay handoff

## Source

- `crates/deadpan-app/src/preview/harness/delete_range/cut.rs`: new focused replay.
- `crates/deadpan-app/src/preview/harness/delete_range.rs`: module and run call,
  after input checks and before the nested fixture changes the baseline.
- `crates/deadpan-app/src/preview/harness/delete_range/capture.rs`: successful
  command cut now replaces the Original copy with the exact historical Edit
  capture. Existing stale-revision and command-entry absence checks also require
  the previously accepted Arc to remain unchanged.
- `crates/deadpan-app/src/preview/harness/delete_range/nested.rs`: unsupported
  composite cuts settle through the typed cut public error, with unchanged
  accepted Original, range, document, durable snapshot and history availability.

## Assertions

1. Real Visual `d` cuts `[20,30)` from the 120-frame Original baseline. Real `dd`
   selects the exact whole child, not a time-only range. Both accept an Edited
   register tagged with the pre-cut revision only after the saved operation.
2. Production `p` pastes the historical contents once, including into an empty
   root. Total length returns to 120; decoded first pasted Original ordinals are
   independently expected as 20 and 0. One Undo removes paste, another restores
   the complete pre-cut authored document except its fresh revision. The accepted
   historical copy remains available and the protected baseline has no Undo.
3. A real cut's ProjectUpdate is held through the existing Feedback mechanism.
   Until receipt delivery, the old document, Visual selection and accepted
   Original remain visible. A separate CaptureEditSlice query replaces the
   service update slot but must preserve the exact successful cut receipt and
   its same Arc capture. Delivering that receipt before the refreshed workspace
   accepts the register without consuming the old selection. Matching workspace
   delivery selects the saved join once. Duplicate receipt delivery after manual
   navigation does not move the cursor or replace the copy.
4. Two further genuine delayed cuts test newer production Original `y` and a
   rejected `:delete` in Original. Both supersede pending register intent. The
   exact late receipt cannot overwrite the newer accepted copy or rejection.
   Delivering the workspace still exposes the already saved 110-frame cut at
   its exact receipt revision, proving register supersession does not cancel
   authored work. Each scenario restores its baseline with Undo.
5. A new service CutEditSlice request reuses the genuine historical target with
   a fresh request serial after Undo. It must fail current-revision validation
   while retaining the accepted capture, complete current document and history
   navigation availability.
6. A genuine held cut is followed by newer production Edit `y` from the old
   visible `[40,50)` selection. Its genuine historical capture and saved-cut
   receipt are delivered with an explicitly simulated stale workspace and
   refresh warning, `committed=None`, and `cut_slice=None`. The independent
   `saved_cut` must allow the new copy while preserving the old active Visual
   selection, cursor, document and reopening warning. After delivering the real
   refreshed workspace and Undo, a new production yank `[2,4)` must finish
   Visual normally against Undo's fresh revision without reviving that warning.

## Verification and limits

Scoped `rustup run 1.97.1 rustfmt --edition 2024 --config skip_children=true`
completed for the three harness files. Scoped `git diff --check` passed.
Root's `workspace-final` compile initially rejected Case 6's `queried.clone()` because
`ProjectUpdate` intentionally has no Clone implementation. The harness now moves
the query update into the simulated stale delivery and subsequently delivers the
original held cut update, which owns the matching refreshed workspace. No
production type changed. Root's later visual replay compiled and exercised this
correction successfully.
No Cargo, decoder, GPU, replay process or UI was run by this agent. Root owns
compilation and the `delete-range` production replay. Root's `visual-cut-first`
report records 134 completed delete-range checks, all passed, including every
new named assertion in cut.rs. It also records the Kestrel audit passed with
11,904 routing cases. The overall replay failed at the later nested rejection
wait: the old harness expected `project_error`, while the atomic private capture
correctly returned its unsupported-endpoint error through `app.error` and cleared
copy pending. The reported document and selection were unchanged.

The nested replay now seeds an accepted Original `[0,5)`, requires the exact
public unsupported-endpoint error, waits for settled cut intent, and asserts
the selected `[30,60)`, accepted copy identity, complete document, durable
read-only snapshot and both durable/UI history availability remain unchanged.
That correction awaits root's rerun. Two explicit named screenshots were also
added after successful Visual cut register acceptance and historical `p` picture
acceptance, because the first run's intermediate capture budget had been spent.
They likewise await root's rerun and inspection.

Evidence: `/tmp/deadpan-structural-capture-20261001/visual-cut-first/report.json`,
debug binary SHA-256
`496623c54ce85612f15294f9a7725f224aaff78d7a23187b519c83ab26be5f8f`.

Delayed delivery uses real service-produced
capabilities; only update ordering is injected. The failed request deliberately
changes its serial to exercise actual service rejection instead of deduplication.
No disk failure is injected here; service tests own actual failed-commit and
saved-cut refresh-error cases. Case 6 explicitly simulates the presentation of a
stale workspace and warning around real service capabilities. This replay does
not establish export, physical device
output or performance qualification.
