# Source Effects integration review

No blocking issue found in the new store test or core/database version integration.

The independent test arithmetic is consistent: after a three-frame prefix, the ten-frame camera path evaluates at local 8 as the midpoint (scale 1.5), then holds its final pose at 13 and 15; changing the final scale to 3 makes the midpoint 2. The gain helper shifts the envelope `[1,5)` to `[4,8)`, its segment endpoint 5 to 8, and mute `[2,3)` to `[5,6)`. The test checks values on both sides of the half-open boundaries. `source_effects.rs` then verifies current snapshots, serialized requests/transactions, exact forward/inverse patches, durable Undo/Redo revisions, and reopened historical snapshots. Its fixture and module comment accurately limit the claim to persistence of already-retained effects, not Trim or media qualification.

The format closure is coherent: core schema 38 and database 47 are current; unused development databases 39–46 reject before writable open or backup; supported databases 1–38 retain their frozen migration path. The new `LegacyFraming` wrapper is used by all historical schemas that admitted full Framing (18–32). It upgrades to `OwnerOutput`, rejects projection of retained clocks, and closes the new `clock` field on old documents, requests, subtrees, and patch sides, including explicit default/null and escaped spellings. `CapturedFraming` stores static `FramingPose` layers rather than a live `Framing`, so it does not need another wrapper.

No check was run here; the root owns execution and reports focused core, plan, and PCM checks passing, with store still running.
