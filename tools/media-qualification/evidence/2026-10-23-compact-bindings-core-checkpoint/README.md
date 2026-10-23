# Compact audio-binding Core checkpoint, 2026-10-23

Working Core58/SQLite87 candidate, not yet production-qualified or committed.
Final Core run: 1,513 tests /69 binaries, zero skipped. Source, six inputs and
executed test binaries match their recorded hashes before and after the run.

Retain the initial build refusal (two borrow errors in new tests), then
1,512/1,513 before a negative nested fixture correction. The corrected fixture
shares a validated state with a resume term and reanchor, confirms a positive
roundtrip, then tests null/oversized paths in all three locations. The original
rejection fixture had no enclosing resume/reanchor and failed its expected
three-placement control. Production logic stayed unchanged through that
fixture correction; compare source manifests.

Independent general/codec/capture reviews and test-fixture follow-up are in
reviews.json. Broader Store/UI, strict checks, actual media/native, package
reopening and current-format performance remain owed. See docs/REQUIREMENTS.md.

Source overlays are relative to the immutable a18ff654 snapshot named by the
qualification script; the declared new-change base is917f9a54. Cargo targets,
nextest archives and executables are excluded; executable identities are kept.
