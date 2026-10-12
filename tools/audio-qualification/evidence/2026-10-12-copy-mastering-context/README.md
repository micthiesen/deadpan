# Copy mastering context

This resolves the mixed-bus uncertainty retained in the preceding
`2026-10-12-partial-sound-copies` evidence. Base source is `a459fc28`.
No production code changes are part of this follow-up.

The new CLI regression copies an Original Source with independent attached
PCM and shared EQ. It compares the authored input separately from the limited
output. Authored f32 bits match; both limited blocks equal the independently
read authored bus multiplied by reported master gain. A strong Original
transient precedes the new placement, so current master gain differs.

`debug-result.json` and `release-result.json` retain identical numerical
results. `project.zip` contains the release fixture and its request files.
The existing partial-copy export also runs again in release; its result and
movie are retained. No live GUI, physical output or listening check ran.

Initial Original-only and mixed-stage diagnostic runs are retained. The only
failed follow-up command requested the app-only ui-harness feature from the
CLI lint target; the corrected CLI/all-target lint command passes with warnings
denied. Source identity, executed-binary hashes and the complete test patch are
retained. Run `python3 check-evidence.py`, optionally with `--source`, to verify
the retained evidence. `checks.py` records the serial verification commands.
