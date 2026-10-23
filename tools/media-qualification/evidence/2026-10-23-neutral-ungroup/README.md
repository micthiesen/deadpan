# Neutral retained-scope Ungroup evidence

See the [qualification record](../../../../docs/qualification/neutral-ungroup-2026-10-23.md)
for scope, results and limitations. `inventory.json` pins every other retained
file by SHA-256 and size. `host.json` records this machine and toolchain.

`source-base.json` identifies the immutable Git archive. Each phase retains its
complete Rust path/hash set and exact changed source overlays. Reconstruct a
phase from that Git commit plus its overlays; `.rs.txt` files are source bytes
with a storage-only suffix. Inputs are pinned separately. Executable manifests
identify the actual extracted test binaries and helpers used for each run.

The baseline and early candidate directories retain failed setup assertions
and lint checks separately from subsequent corrected runs. The broader passing
output-handle warning and its passing isolated rerun are both retained. Neither
is evidence of a diagnosed production leak or a leak repair.

Full numerical PCM documents use deterministic gzip compression. Media folders
retain encoded files, controls and portable package archives. Portable archive
members are regular files with relative paths; checksums and validation receipts
record the independently extracted release packages. No listening or independent
DSP claim follows from canonical PCM or emitted-file agreement.

`procedure/` contains the exact qualification and retention scripts and review
results. Native performance-mode reports establish routed state and submitted
picture identities; they do not establish visual appearance or physical display
timing. Existing performance and layout warnings remain in the reports.
