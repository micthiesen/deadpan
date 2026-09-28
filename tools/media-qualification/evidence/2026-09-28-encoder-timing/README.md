# SDR encoder timing evidence

See [qualification](../../../../docs/qualification/encoder-timing-2026-09-28.md).
The native and sanitizer matrices deliberately retain a nonconforming AAC path.
Their exit 1 is not a passing export gate. Video follow-up reuses original media;
the original failed run and original source bytes remain separate.

Reports and command logs are compressed without truncation. Log tar archives
contain complete original stdout/stderr. measured-fixtures.tar.gz contains all
private synthetic MP4/PCM and before-mux sidecars, including the failed hardware
B output. No user media, project, executable or library is published. Archive
member hashes/sizes are in archive-contents.json; manifest.json hashes every
retained file except itself. audit.py checks both levels without extracting.

Source inventories identify the complete checkout inputs; probe source archives
contain the relevant developer files only. The build receipt belongs to the
September 26 prefix, with fresh admission observations recorded separately.
Scripts retain their original task-specific paths; adapt paths to reproduce.
Test populations overlap. AVFoundation, closed GOP independence and full product
export are not qualified by this record.
