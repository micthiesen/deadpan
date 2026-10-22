# Transcript-search selector evidence

See [the qualification record](../../../../docs/qualification/transcript-search-selectors-2026-10-22.md)
for the implemented scope, retained failures, exact verification and limits.

`inventory.json` lists every retained file except itself with byte size and
SHA-256. Large JSON, logs and scripts use deterministic gzip. Native screenshots,
the emitted MP4 and the portable owner project archive remain directly readable.

`frozen-snapshots.json` maps each candidate to its path/SHA-256 manifest.
`frozen-source-and-input-blobs.tar.gz` stores each distinct frozen byte payload
once, named by its SHA-256. Reconstruct a candidate by reading its manifest
(decompressing `.gz` when applicable), finding each hash-named archive member,
verifying its bytes against that hash and writing it to the manifest's relative
path in a separate scratch source copy. Reject absolute paths or `..` components.
These snapshots include every Rust file plus the declared supplemental inputs;
the remaining unchanged build files come from the recorded Git baseline.
Use a separate Cargo target directory for baseline source comparisons.

The baseline controls run against committed interval-selector source before
transcript-search admission. Failed setup/build/replay candidates are preserved
alongside corrected runs. `ui-to-final-attribution.json` identifies the sole
later Core test-fixture change after the complete UI suite. The final source
audit compares every frozen Rust path and six supplemental inputs with the
published code commit's Git blobs.

The owner archive is safely extracted, all paths and bytes are rehashed, and
the extracted project passes `project validate` and `doctor --project`.
The fixture annotations are synthetic words on qualified synthetic media.
Audio comparison verifies canonical preview against the emitted stream at zero
offset; it does not establish an independent PCM oracle, listening quality,
ASR accuracy or full mastering. Offscreen native replay does not establish
physical keyboard, IME, display or VoiceOver speech behavior.
