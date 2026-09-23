# Schema 15 migration fixture

`generate.py` archives commit `d1348a5dbf2a4979a54af823b4a4f5ca82ed44e1`
and builds its unchanged core schema 10/database schema 15 CLI and store code.
Only the standalone fixture example is added to the archive. The old CLI
migrates two independent copies of the committed schema 14 fixture; the old
store API then writes each schema 15 history.

The harness retains and qualifies `cfr-bframes.mp4`, imports it as the first
recorded primary source while preserving the existing explicit frame rate,
and adopts its measured geometry. It then authors a canvas change, exercises
undo and redo, and leaves one pending redo. The inherited source qualifications,
abandoned source alias, original ownership, and generation operational rows
remain in the complete history. Managed media bytes are not embedded in the SQL.

Both old-binary validations passed. SQLite's backup API captured both SQL dumps,
which reproduced byte for byte with SHA-256
`c888b5fd201eacbb3f45a9f01ca50e5ee4dbb3e62b52a15d1f03109a0c48757b`.
The fixture contains 61 revisions, 31 history rows, two source qualifications,
three original ownership records, and one pending redo.

Reproduce with the qualified development prefix available:

```sh
DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-media-compatible-xyhilms4/prefix \
  python3 tools/media-qualification/evidence/2026-09-23-audio-edges/fixture/generate.py
```

`manifest.json` records the archived revision, CLI, generator, harness, source
fixtures, SQL fixture, hashes, counts, and actual prefix. The compressed generation
log retains build and command output with exit codes; doctor and both validation
reports are retained separately. This qualifies old-schema fixture authenticity
and deterministic reproduction, not playback or export.
