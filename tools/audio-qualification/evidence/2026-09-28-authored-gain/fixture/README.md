# Genuine database-38 gain migration fixture

`generate.py` is the exact script used with the preserved verified CLI at
`5fd8b23365df0d203e3a9ae8f5c0c14cc69b6040`, before new schema compilation.
`old-cli-provenance.json` records that executable's SHA-256 and its verified
source manifest. The 132 MB executable remains in task scratch, outside Git.

The script opens the existing database-37 fixture, migrates it through the old
CLI, and authors direct Hold room tone, a three-play Repeat, one isolated silent
play, abandoned Tail intent and pending redo. Existing routes, allowances,
abandoned sound edits and qualified originals remain in historical revisions.
The resulting SQL contains 30 revisions and 21 history entries, no absolute
local paths, and no relabeled modern snapshots or authored SQL modifications.

The first attempt truthfully rejected Repeat wrapping while current sound
events existed. The second rejected a mistyped occurrence field. The final
script removes current sounds through commands, preserving their history, and
uses the closed occurrence grammar. All three command logs are retained;
only `fixture-generation-3.json.gz` produced the fixture.

To reproduce, obtain a core-32/database-38 CLI from the recorded revision,
configure the pinned FFmpeg prefix, and set the script's `root`, `scratch` and
`cli` paths to an isolated destination. Its `doctor` assertions reject a modern
binary. Undo revisions are freshly allocated, so reproduced SQL has different
UUIDs; the migration assertions compare complete retained chronology and intent.
