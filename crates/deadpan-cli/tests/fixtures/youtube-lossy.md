# Lossy WebP thumbnail fixture

Owned procedural 120 × 80 RGB quadrants: top-left `(200, 180, 80)`,
top-right `(20, 180, 80)`, bottom-left `(200, 30, 80)`, bottom-right `(20, 30, 80)`.
Encoded with Pillow WebP quality 80, method 6, lossless disabled.
This exercises the lossy VP8 branch separately from image-rs’s lossless encoder.

SHA-256: `2f9a9054c87a54db93abebcff572e27bb28e449ce46b063e341c7d9c6bead0f7`.
