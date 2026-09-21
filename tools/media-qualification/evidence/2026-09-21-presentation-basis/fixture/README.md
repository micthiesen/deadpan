# Schema 14 migration fixture

`generate.py` archives commit `14c75218b695e732773a4b247a45db3b7290faac`
and builds its unchanged core schema 9/database schema 14 CLI and store code.
Only the standalone fixture example is added to the archive. The old CLI
migrates two independent copies of the committed schema 13 fixture; the old
store API then writes each schema 14 history.

The harness retains the complete `cfr-bframes.mp4` and `vfr.mp4` originals in
managed storage, snapshots their verified bytes, independently decodes video
and audio, and creates real immutable qualification receipts. Both sources
are atomically registered and inserted. Undo removes the first import before
the same asset alias is used for the second source, preserving distinct
receipts on the abandoned and current branches. Rename undo/redo leaves one
pending redo. The inherited original record and generation operational rows
remain unchanged. Managed ownership uses stable source filenames and no
temporary linked paths.

Both old-binary validations passed. The SQL dumps were captured through
SQLite's backup API and reproduced byte for byte with SHA-256
`b1f91bcd279877ddd5c6b33d656ddd0faa55ed11e1bdc9e155191f40b4b3d112`.
The fixture contains 54 revisions, 28 history rows, two source qualifications,
three original ownership records, and one pending redo. It contains metadata
only. Original/generated media can be offline during migration and historical
receipt/index validation.

Reproduce with the qualified development prefix available:

```sh
DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-media-compatible-xyhilms4/prefix \
  python3 tools/media-qualification/evidence/2026-09-21-presentation-basis/fixture/generate.py
```

`manifest.json` records the archive, CLI, generator, harness, input fixture and
output hashes. `generation-*.txt.gz` retains actual build and command output with
exit codes; the doctor and both validation reports are retained separately.
These fixture checks establish authentic old-schema history and deterministic
reproduction, not playback or export qualification.
