# Schema 13 migration fixture

`generate.py` archives commit `ed5c8eccebb8bb77451284f95b99b4ed33c49c93`,
builds its locked CLI and store example in `/tmp`, and verifies its doctor reports
core schema 8 and database schema 13. It seeds two independent packages from the
committed schema 12 fixture. The old CLI migrates each, and that commit's store
API creates the new history using `schema13_fixture.rs`.

The added edits include signed video and audio placements through direct and
occurrence commands, a local mark, an abandoned rename branch, and undo/redo
ending with one pending redo. Existing operational requests, attempts, bundle
receipts and original records remain. Each result is validated with the old CLI,
then dumped through SQLite's backup API. Both dumps must equal the checked-in
`crates/deadpan-store/tests/fixtures/v13-history.sql` byte for byte.

Run from the repository with the qualified FFmpeg development prefix:

```sh
DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-media-compatible-xyhilms4/prefix \
  python3 tools/media-qualification/evidence/2026-09-21-source-registration/fixture/generate.py
```

`manifest.json` retains source, generator, harness, executable, archive, seed and
output identities plus both reproduction hashes. `generation-output.txt.gz` records
the build and old-binary validation commands. The fixture contains metadata only;
it does not supply external media or establish source qualification. Migration
must retain `source_qualification: None` for every historical asset and create an
empty source qualification inventory.
