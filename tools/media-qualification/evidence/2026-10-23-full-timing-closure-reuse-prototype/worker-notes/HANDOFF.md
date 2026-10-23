# Full timing closure reuse: private source-only prototype

Ownership and scope:
- Candidate is /tmp/deadpan-full-timing-closure-reuse-2026-10-23/source, based on594dd38f.
- Only crates/deadpan-core/src/audio_reference.rs changed. tests/command_work.rs was unnecessary: its exact10k command fixture and private Arc/alias assertions fit the existing scoped_capture_property unit module.
- One production branch was added after the entire required alias/ancestor loop and after the unchanged derived-clock fallback. Local mode, complete layout and exact full closure require wire_bytes()? before returning self.clone().
- No other production line changed. A source comparison removing the exact seven-line branch reproduces the original entire pre-test source byte-for-byte.
- No public API, production counter, serializer, validation/budget/authority, InsertTime or compact_new_timings change.

Source-only review:
All2050 original Rust identities matched Root's baseline before edits. A broader before/after audit confirms all33823 nonowned files among33824 baseline files remain byte-identical. No file added/deleted. Complete diff reviewed. Original/proposed .rs.txt snapshots, instrumentation.patch, source-hashes.json and source-review.json retained here. Existing six input identity receipt remains outside source and was not changed.

Five new tests, all prefixed full_layout_reuse_:
1. leaf_closure_preserves_nested_wire_and_fields: leaf-only aliases omit parents and preserve exact fields/JSON through nested Sequence, Repeat, play/gap overrides, lineage and a zero-duration empty Sequence. Local Arc shares original, reference rebuilds.
2. partial_and_provisional_tables_still_rebuild: partial complete-table slice plus provisional requests for original required aliases and every stored node (including spacers). Compare exact values/bytes and require nonprovisional serializable output with a different Arc.
3. late_unknown_alias_precedes_identity_and_clock_fallbacks: aliases cover every valid node before an unknown alias, checking the exact InvalidTree error in both modes; valid derived-clock fixture also preserves existing shared fallback in both modes.
4. retains_canonical_wire_limit_refusal: a valid complete99,999-Hold graph with real128-byte identities/lineage, admitted through private admit_sized(false), must independently reject wire_bytes and full-closure sliced in both modes with exact LimitExceeded / frozen audio JSON exceeds byte limit. Failed counting must not populate the cache. No invalid index, test-only ceiling or production bound change.
5. ten_thousand_hold_interior_pause_matches_reference: fresh10,000 unbound four-frame Holds, insertion at frame17933; compare exact results/transactions, document and transaction compact bytes, forward/inverse application and restores checks. Gather all resulting binding-placement aliases, prove their ancestor closure equals the complete retained table, and directly assert local Arc sharing versus reference rebuild. Arc identity across compaction alone is not used as branch evidence.

Unexecuted fixture uncertainties for Root:
- All Rust source and fixtures remain uncompiled/unformatted/untested as assigned.
- The large canonical fixture's valid admit_sized(false) success and exact real-ceiling refusal are asserted but not yet observed; expected canonical wire is roughly75MB. It directly exercises unchecked-canonical private admission, and does not claim that its oversized canonical representation itself is accepted by from_json.
- The10k test asserts rather than assumes that exactly one complete table remains and the gathered aliases cover it. Root's focused execution must establish those facts.
- No performance claim.17.760625ms was an inclusive group upper bound, not a saving prediction.

Root retains Cargo, formatting, tests, builds, runtime, measurement and Git ownership. This worker executed none of those. Ready for Root's focused release tests and review.
