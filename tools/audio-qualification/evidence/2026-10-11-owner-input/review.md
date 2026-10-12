# Solo review

The primary agent implemented and reviewed this change. No subagents ran.

- Checked owner identity includes the immutable plan, definition, instance,
  kind, gap, play, origin and affine clock at the resolving sample. Arithmetic
  failure cannot compare equal. Current membership is queried again.
- Owner capture retains the absolute grid and inherited constraints while
  removing only transparent delivery clipping. Existing physical captures and
  read paths keep their prior behavior when no owner capture is requested.
- An enclosing Preserve or binding is retained as the raw processor, with
  explicit scope. Nominal clock inversion cannot replace its PCM.
- Resumed reference history is separate from the current delivery domain.
  The checked raw domain and map expose that history, including PointCeil.
  They do not claim a complete post-effect mix or transport creative edges.
- Reader queries and creative fades are admitted before source access, under
  one work/deadline scope. Foreign, cancelled and invalid reads call no provider.
- Cold decoded PCM, hidden siblings, native EQ, Preserve, silent Holds and
  transferred retained input have meaningful reference comparisons.

Initial test compilation needed fixed array dimensions and the existing
`is_cancelled` predicate. A hidden-signal assertion checked only the fixture's
silent left channel; it now checks both channels. The test also verifies that
discarding hidden history changes EQ output. No DSP assertion was relaxed.

The complete suite passed before the final PointCeil test and comment/private
field refinements. The final six owner-input plan tests pass after those edits;
workspace Clippy included the PointCeil test. No behavior changed after checks.

Ordered authored EQ, retained edge transport, complete owner contributions,
sound/group mixing, per-voice permissions, caching and editor/export integration
remain required. These contexts are prerequisites and do not close DP-09/DP-16.
