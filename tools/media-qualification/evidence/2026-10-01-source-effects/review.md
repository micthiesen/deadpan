# Read-only review: retained framing and physical-prefix gain

Reviewed `framing.patch`, `framing-legacy.patch`, and `gain.patch` against current core, plan, Camera, and audio test helpers. No actionable correctness or test-API issues found.

- Retained framing maps `current_local + offset` into the authored interval, clamps to endpoint poses, and validates current owner bounds first. Prefix composition and tail-only retention are consistent. Camera adjustment clones and transforms the existing path, keeping its clock; explicit reset creates an OwnerOutput static pose.
- `LegacyFraming` closes the pre-38 wire shape. Each legacy v18-v32 module has its own `TransposeOption` implementation; the new map/transpose preserves `None` framing and rejects an unprojectable retained clock. The document, command, occurrence-edit, subtree, and history-patch routes are covered, including null/escaped keys.
- Gain prefix translation shifts envelope ranges, each segment time, and mute ranges atomically; it leaves trim/mute values, root-owned gain/sounds, and the independent sample offset untouched. The integration oracle uses 256-frame authored reads (below the 256 maximum), source range through 7256 within the fixture's 8197-frame support, and chunked oracle reads capped at 251.
- Confirmed proposed test module helpers and public API signatures against current files. Schema 38 / DB 47 integration remains root-owned and was outside these drafts.
