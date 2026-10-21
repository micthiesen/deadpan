# Independent review

Scope: dirty changes against origin/main at 25f40767. Three independent
reviewers: general core/native/store; audio admission and PCM/export oracles;
native/semantic exact resolution and no-op/context/replay. Read-only review;
root owns Cargo and edits. Every owned source file was frozen for review.

Applied: the normal PCM test incorrectly required an appended outside
reanchor even when it was inert. It now preserves lattice, resume and history
prefix while the literal transfer-map, complete PCM, suppression and chunk
checks prove the actual transport. A separate store fixture with a non-entry
resume requires an actual appended step.

Dismissed after source checks:

- Semantic no-op allegedly compared old 24 to exact 27: set_clock updates the
  integral output recipe to 27; only its child window covers 13.5 to 14. The old
  retained document and the semantic no-op test establish the correct values.
- Missing mark transform allegedly leaves stale coordinates: the exact reducer
  calls supported before mutation; effective_durations also refuses every
  document combining derived clocks with marks, sounds, beat sounds, routes or
  allowances. There is no admitted marked input for that changed update and no
  valid query/commit eligibility mismatch. No redundant guard was added.
- Source audioNone allegedly needs no transfer bound: bound_at precedes the
  leaf's silence branch. StageAudio::read_bound_at constructs the transfer
  before reading that silent domain. Skipping its actual retained transfer
  would admit a renderer failure. Actual-audio rate103 evidence is separately
  executed by rate_bound_pcm; the core fixture is geometry/binding evidence.

The host reviewer found no concrete issue. The reviewers found no other
concrete production, PCM wiring, native, semantic or store issue in scope.

Independent execution found fixture-only orphan timing records after bindings
were deliberately removed/replaced. Test fixture cleanup now retains only
timing rows reachable from the remaining binding/phase/journal state. This
does not change production pruning or relax strict document validation.

Follow-up source audit and executed witnesses found empty old physical
carriers whose new output allocation is nonempty. Playback returns silence
before transfer construction. The validator now checks complete meaningful
raw support, ignoring transparent Partition visibility, for both retained
and fresh pre-edit captures. Both old admission failures were executed and
retained before the fixes. Actual PCM coverage checks the16-sample retained
and six-sample freshly captured cases, including suppressed zeros, alternate
chunks and no provider resolution. Execution remains root's responsibility.

The focused audio reviewer audited the final shared empty-carrier patch and
reported no remaining concrete audio ownership or transfer admission issue.
An extra suggested reference-local-offset adjustment was dismissed: resolved
lattice sample_boundary and raw_domain both map current local support through
the already-resolved origin and scale. Subtracting the template offset again
would test a different interval. Selected Source crop-only emptiness remains
a conservative admission limit, explicitly recorded in qualification.

Release replay follow-up: Apply returns focus to Sequence. Its next Enter
correctly tries content navigation, so the replay's inspector-prefill assertion
failed. Root added the actual Shift-Tab navigation plus a Pane::Inspector
assertion before Enter. The host reviewer independently confirms this matches
the established nested-pause replay contract; no production routing changed.
The first failed report and source/binary identities are retained separately.
