# PCM reader contract fix: source-only handoff

Writes stopped. Only assigned repository file crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs changed. No Cargo, formatter/compiler, runtime/media/GUI, Git or other Main writer ran. Root owns formatting, typecheck/Clippy, broad shared-target compile, release archive rebuild and runtime checks.

The old common reader body is now read_exact_with_stage(path, partitions, expected_stage). Legacy read_exact passes exactly authored_bus_pcm_before_mastering, preserving the old aligned/neutral fixtures' strict expectation. New read_one_leaf_exact passes exactly edge_faded_pcm_before_voice_effects. There is no allowlist, stage normalization, fixture FX addition, production change or different audio read API.

After the existing ProjectAudioSession::open and before reading any block, the explicitly selected edge expectation asserts that this same session's plan has:
- sounds().is_empty()
- beat_sounds().is_empty()
- !has_audio_treatments()

The session whose predicates are checked is the mutable session subsequently passed to read_authored_bus and read_limited. No extra unused proof session is opened. Every returned block still has an exact stage equality assertion, with the wrapper's fixed expected value.

Exactly six call sites inside one_leaf_replay_qualified_media_fixture now call read_one_leaf_exact:
1. Cached baseline read [256].
2. Portable baseline read [37,173,251].
3. Cached published result read [256].
4. Published cached/uncached partition loop [37,173,251], two invocations.
5. Undo cached/uncached loop [251,37,173], two invocations.
6. Reopened redo cached/uncached loop [173,251,37], two invocations.
Total actual reader invocations on the source-defined successful route: nine. All other full-reader call sites still call legacy read_exact.

source-preservation-proof.json records a checked textual reconstruction:
- Remove the three new same-session plan assertions and substitute the legacy fixed stage literal for expected_stage in the extracted reader body, and the body is byte-identical to the original.
- Restore the six call names, and all other file bytes reconstruct the complete original.
- cold_exact is byte-identical, including authored/limited/limiter-gain cold bit comparisons.
- The Some capability assertion remains present.
- No preview_export crate-root dependency was added.

All previous62 logical test assertions remain: the one-leaf's erroneous fixed processed-stage assumption is replaced by its justified exact edge expectation, while the legacy processed-stage expectation is unchanged. Three independent shape assertions are additions. Sample/gain/finite/length checks, read ordering, canonical authored/limited/limiter bits, partition/cold comparisons, silence/nonzero controls, Store/Host/forward/inverse/history checks, literal pictures and both export/receipt checks are unchanged. The export-wrapper file was not edited.

Snapshots: aligned_group_recipe.before.rs.txt and aligned_group_recipe.proposed.rs.txt. Exact before/proposed SHA256: source-hashes.json. Patch: reader-contract-fix.patch. This source remains unformatted, uncompiled and unexecuted by this worker. No positive test result or performance claim is made.

Required Root execution retains existing qualified pinned media/toolchain environment. Positive release target:
cargo test --release --locked -p deadpan-cli --test preview_export group_endpoints_export::one_leaf_replay_qualified_media_commit_export_matches_uncached -- --nocapture --test-threads=1
with DEADPAN_REQUIRE_PHYSICAL_MAC=1, DEADPAN_PREVIEW_EXPORT_KEEP=1 and DEADPAN_PREVIEW_EXPORT_RESULTS pointing to the new external receipt. Broad typecheck must still cover targets importing recipes. Root also plans the old aligned/neutral controls to confirm their unchanged strict tag via the extracted common loop.
