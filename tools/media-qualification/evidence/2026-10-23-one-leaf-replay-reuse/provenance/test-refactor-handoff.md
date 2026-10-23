# Qualified-media test inclusion fix: source-only handoff

Writes stopped. The old test lived in a recipe module included by multiple integration targets, so preview_export-only crate-root calls failed their broad compile. The shared work is now a non-test public test-fixture helper; the actual test lives in the existing preview_export-only export module.

Only two assigned existing repository files changed:
- crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs
- crates/deadpan-cli/tests/preview_export/group_endpoints_export.rs

Shared API: one_leaf_replay_qualified_media_fixture(root: &Path) -> Result<(Fixture, Fixture, serde_json::Value)>. It performs all real setup, capability, Store, full PCM, history, literal-picture fixture work, retains canonical files, and returns cached/uncached fixtures with canonical/history summary. It has zero crate:: preview_export-root calls and no #[test]. Caller owns the temporary directory lifetime.

New full test path: group_endpoints_export::one_leaf_replay_qualified_media_commit_export_matches_uncached. Existing release/debug-ignore convention is unchanged. It owns keep_or, both fixture checks, strict physicalMac guard, render/verify/provenance/completeaudio assertions, failed verifier report retention, exports[] completion and internal/external final receipt.

Schema deadpan-one-leaf-media-parity-1 is unchanged. The actual root/evidence_dir and both leg/package/movie/revision/verification entries remain in DEADPAN_PREVIEW_EXPORT_RESULTS, written only after both outputs pass. DEADPAN_REQUIRE_PHYSICAL_MAC=1 must remain set for qualification; DEADPAN_PREVIEW_EXPORT_KEEP=1 preserves all movie/evidence paths.

Every original62 assertion macro and its multiplicity is preserved (51helper+11wrapper). See assertion-preservation.md/json. Original pre-one-leaf recipe prefix and all prior export file bytes are preserved. No assertion, authority, verification stage, receipt or failure handling was weakened. No customcfg, new source file/module, production API or skip workaround was added.

Snapshots: *.before.rs.txt and *.proposed.rs.txt. Exact hashes: source-hashes.json. Complete two-file patch: refactor.patch. Unformatted, uncompiled, unexecuted by this worker. No Cargo, rustfmt/compiler, media/GUI/runtime, Git or other repository writer ran.

Root owns scoped formatting, independent review and focused release execution, plus the broad integration-target compile that exposed the inclusion error. Recommended focused command suffix:
cargo test --release --locked -p deadpan-cli --test preview_export group_endpoints_export::one_leaf_replay_qualified_media_commit_export_matches_uncached -- --nocapture --test-threads=1
with Root's pinned toolchain/media environment, DEADPAN_REQUIRE_PHYSICAL_MAC=1, DEADPAN_PREVIEW_EXPORT_KEEP=1 and DEADPAN_PREVIEW_EXPORT_RESULTS set to the requested new external receipt.

Prior qualified runtime evidence belongs to its original source. This refactor has source-only preservation evidence until Root reruns it.
