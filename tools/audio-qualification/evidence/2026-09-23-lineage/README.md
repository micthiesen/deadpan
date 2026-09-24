# Audio lineage evidence

The gate script records the six required repository checks and verifies unchanged
SHA-256 hashes for all tracked and untracked Rust source, native source, fixtures
and Cargo configuration before and after the run. Numbered compressed logs match
the command order in `report.json`. `summary.json` records aggregate Rust test
results and the design/specification checks.

`fixture-provenance.json` records the preserved CLI revisions and executable
hashes, the two committed SQL fixture hashes and the capture script hashes.
The binaries and live developer project packages are not distributed here.
The scripts are retained capture records using task-specific developer paths.
The database-20 script starts after an empty project was created with its old CLI:

```sh
deadpan-cli-core14 project create /tmp/deadpan-lineage-20260923/legacy-fixture.deadpan --fps 30000/1001 --size 16x16
```

The database-4 script creates its own empty project. Each script validates the
populated package with its old executable and takes a SQLite backup before the
SQL dump. The database-20 dump's recorded CLI hash was added as a comment after
capture. Repeated execution requires new scratch packages and produces new
project/navigation identities; the committed fixture bytes are the tested input.

See [qualification](../../../../docs/qualification/audio-lineage-2026-09-23.md)
for behavior, review results and limits. These checks do not qualify playback,
listening quality, export or the complete editor.
