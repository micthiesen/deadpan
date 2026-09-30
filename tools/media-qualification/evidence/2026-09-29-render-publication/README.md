# Verified destination publication evidence

See [qualification](../../../../docs/qualification/render-publication-2026-09-29.md)
and the [publication contract](../../../../docs/RENDER_PUBLICATION.md).

Seven fresh project movies passed real encoding, isolated verification, destination
publication and independent decode from their final filenames. The corpus contains
304 pictures, 390,695 authored sample frames and 21 complete GOPs. All 912 picture
planes and 18 fixed-coordinate audio marker observations passed. A separate audit
checked final movie/report hashes, historical document and Original identities,
and the exact Generated interval. All 2,248 workspace tests, strict Clippy and
formatting passed; 26 tests directly cover publication.

## Contents

- `summary.json`: measured totals, command outcomes and exact source bindings.
- `native-artifacts.tar.gz`: actual candidate/published movies, local reports,
  direct I420/PCM references, independent reader results and coherent project
  backups taken through SQLite's backup API. No bare live database was copied.
- `archive-members.json` and `archive-audit.json`: size/SHA-256 inventory and full
  readback of all 354 archive members, totaling 637,656,718 uncompressed bytes.
- `publication-audit.json`: independent final-file, report, historical-revision
  and retained Original checks for each case.
- `*-review.md` and `review-disposition.md`: original source findings and their
  final corrections. Earlier review terminology is historical.
- `*.log`, `*.json` and compressed counterparts: full check output, failed first
  compile, command journals, source inventories and copied executable hashes.
- `*.py.txt`: exact evidence helper sources, retained as text.
- `file-manifest.json`: size/SHA-256 of every other file in this directory.

Archive SHA-256:
`0b9557e4a2006b54e1ea46fc44f5a76c3aa608892db929c00982b7ea2a00cf29`.
Archive size: 10,095,978 bytes. Every archived member was read and hashed again.

All final checks share source inventory
`8332c9d07a73d66364e6ff1468399a369355cefedcb7b68de5d91db4231c790d`,
based on commit `2e381b36a0aefe7278545fc9406a585fc6e57c51` plus this publication
change. `source-changes-since-check.json` contains empty differences for each
final check. Documentation and archived evidence were added afterward.

## Scope

The report and movie are separate atomic commits. A report alone does not prove
movie publication. Failures after the movie rename retain that fact as
`PublishedUnconfirmed`. This does not implement durable job recovery, automatic
encoder policy, native Render, full mastering/effects, HDR or release qualification.
Native C was unchanged; its preceding sanitizer results are not presented as a
new sanitizer run here. Removable/network filesystems, power loss, sustained
performance and Linux publication remain unqualified. No native UI changed.
