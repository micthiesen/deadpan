# Authored gain evidence

See the [qualification record](../../../../docs/qualification/authored-gain-2026-09-28.md)
for the implemented boundary, review findings, results and remaining native work.

Each command JSON records the literal invocation, start time, base commit,
working-diff hash, source-manifest hash, terminal exit and elapsed time. Its
matching gzip log preserves the complete output, including failed checks.
`summary.json` derives test counts from the terminal result lines. Counts from
focused checks overlap the workspace suite and must not be added to it.

Source manifests retain SHA-256 identities for tracked and untracked build,
source and fixture files selected by the runner. Their gzip filenames contain
the hash of the uncompressed JSON. Final formatting, strict Clippy, artifact
inventory, resumed tests and doctests use the final source identity. Documentation and evidence
updates after the gate are not claimed as compiled source.

`core-01` is diagnostic: two stale context fixtures failed, and one additional
test-source edit occurred after its starting manifest. Corrected focused and
final checks establish the verified boundary. The first full Clippy run also
records two missing empty-treatment defaults inside playback `json!` fixtures;
the corrected full Clippy run supersedes that failure.

The initial workspace invocation stopped at an outdated expected duplicate-key
error string in the document test. The bounded deserializer still rejected the
duplicate. Only that test assertion changed to verify the current diagnostic
and `InvalidJson` category. The initial invocation's successful targets remain
valid; its failed target's partial passes are excluded from the final total.

`tests-artifacts` records Cargo's exact current executable inventory after
rebuilding the changed test. `tests-workspace-resume` runs the failed target and
all remaining native targets in their package working directories, then
`tests-doc` uses Cargo for all doctests. `resume-inventory.json.gz` retains the
selected and completed targets, binary hashes, freshness and individual exits.
The retained runner sources explain this continuation. `qualified_workspace`
in the summary combines only successful, nonoverlapping target results.

The [fixture directory](fixture/README.md) separately retains the genuine
database-38 history generator, old executable provenance, failed preparation
attempts, final SQL identity and reproduction details. The old executable itself
is not committed. No current snapshot was relabeled to manufacture old history.

`manifest.json` hashes every retained file below this directory except itself.
Durations include compilation and are not product performance measurements.
No GUI, physical-device listening, export or complete DP-09 qualification is
implied by these backend checks.
