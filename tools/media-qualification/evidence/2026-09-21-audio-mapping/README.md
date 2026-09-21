# Independent source audio mapping evidence

`gate/` records the exact required workspace commands and deterministic gzip logs.
`source-manifest.json` identifies the tested base and changed source/fixture hashes.
`review.json` records independent review dispositions.

`fixture/` retains the script and Rust harness that reproduced the schema-10 SQL
fixture using revision `82f76aaf86e24d4fb3aec822ada0a638d19ddf25`, plus exact old
source/CLI hashes and old-binary validation output. The SQL is committed at
`crates/deadpan-store/tests/fixtures/v10-history.sql`. Its SHA-256 is
`32ebf6e759518f227bf9019379338e0f7697668dab92902ce67e27eed49d8be0`.

The recorded reproduction script uses the local checkout, archived CLI and
qualified FFmpeg prefix. It is a development evidence script, not an application
runtime requirement. To reproduce on another checkout, copy it and its Rust
harness to scratch, set `REPO` and `CLI` to the corresponding checkout and a
binary built from that exact revision, and retain `DEADPAN_FFMPEG_PREFIX`.
The historical SQLite backup remains at
`/tmp/deadpan-schema10-evidence/schema10-before-migration.sqlite` on the measured
machine. SQL and hashes capture its metadata without duplicating the database.

No renderer, device output, listening, GUI, startup or accessibility claim is
made by these headless document/history checks.
