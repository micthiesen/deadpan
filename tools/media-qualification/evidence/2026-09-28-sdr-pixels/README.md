# Shared SDR encoder pixel evidence

See [qualification](../../../../docs/qualification/sdr-encoder-pixels-2026-09-28.md)
and the [pixel contract](../../../../docs/SDR_ENCODER_PIXELS.md).

Reports retain the actual Metal adapter, independent full-plane comparison,
readback lifecycle checks and exact Cargo artifact identity. Hardware H.264
receives the renderer's actual planes, never its reference. Both actual and
reference I420, encoded MP4s, packet logs and complete decoded planes are saved.
The declared renderer and lossy-encoder tolerances remain separate.

Complete command logs and source identities include the initial lint failure
and its narrow correction. Read each command's exit and each native report's
source/file admission and process-fault results; fixture output alone is not a
passing check. Sources are retained in Git and bound by the inventories.
No user media, executable, library or project is published here.

manifest.json hashes every retained file except itself. archive-contents.json
records every archive member. Run audit-sdr.py to verify both levels without
extracting files. The scripts keep task-specific paths for attribution.

These synthetic video-only SDR fixtures do not qualify AAC timing, native video
decoding, multi-frame closed GOPs, real footage, full resolution, HDR, physical
viewing, performance or the product's immutable project export/publication path.
