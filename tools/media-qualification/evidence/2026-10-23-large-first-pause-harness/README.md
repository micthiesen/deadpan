# Large first-pause harness evidence

This directory holds the verified final release run and bounded supporting
evidence for the diagnostic `large-first-pause` UI scenario. See [the
qualification note](../../../../docs/qualification/large-first-pause-harness-2026-10-23.md)
for measurement results and limits.

Retained items include final and diagnostic summaries, raw JSON/HTML reports,
input and guard receipts, build/test/lint receipts, source and binary
identities, the final three `release-final-source__*.rs.txt` overlays,
historical draft overlays explicitly labeled `historical-draft`, wrapper
scripts, and compressed final command output. The complete final UI archive
run receipt and output log are retained; the archive binary itself is omitted.

The initial wrapper's failed full-path unchanged assertion is preserved. It
failed because SQLite created coordination sidecars absent from the input path
map; the missing persisted before-map is an evidence limitation, not the cause.
The main database hash matched. The final complete maps differed only for the
32 KiB `-shm` file and empty `-wal` file. No executable binaries, Cargo target
directories, or generated fixture packages are included.

The final release binary SHA-256 is
`3bf2d47c6fbc7eade3a259fc8e9ffc275043af2bd62e698a0b98c0937dcfd9bd`.
`inventory.json` lists every file except itself with its stored byte count and
SHA-256.
