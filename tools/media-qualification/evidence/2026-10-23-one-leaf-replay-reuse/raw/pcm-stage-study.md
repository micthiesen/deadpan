# One-leaf PCM stage diagnosis: read-only

No Main edits, Cargo/compiler/media/GUI execution or Git mutation were performed. Root's actual release failure is the starting evidence: the new no-FX fixture reached read_exact before commit and returned edge_faded_pcm_before_voice_effects, while the inherited processed-fixture reader required authored_bus_pcm_before_mastering. This is a test reader's fixture assumption, not source evidence of a production audio defect. The proposed after/undo/redo behavior remains to be asserted and executed.

## Exact current contract

1. crates/deadpan-cli/src/audio.rs:255, ProjectAudioSession::read_authored_bus:
   - explicitly promises the canonical bus after authored node/sound gain, before limiting;
   - checks its bounded sample range;
   - calls stages.prepare_authored_bus, rather than read_edge_faded.
   Preserve this API call in the test.

2. crates/deadpan-audio/src/stages.rs:806, StageAudio::prepare_authored_bus:
   - calls prepare_bus, which invokes prepare_bus_inner with authored_gain=true.
   - prepare_bus_inner around lines879-899 selects the returned label from the plan-wide predicate:
     has_sounds = !plan.sounds().is_empty()
               || !plan.beat_sounds().is_empty()
               || (authored_gain && plan.has_audio_treatments()).
   - has_sounds=true returns stage authored_bus_pcm_before_mastering and processing_order [per_voice_time_pitch_edges_gain, group_mix].
   - false returns stage edge_faded_pcm_before_voice_effects and processing_order [time_pitch_mapping, edge_fades].
   This choice is plan-wide, not conditional on the requested sample window being silent or containing the click.

3. crates/deadpan-audio/src/sound_events.rs:25-64:
   - the authored reader still performs the full canonical dispatch;
   - when sounds and beat_sounds are empty and there are no audio treatments, it deliberately uses read_controlled(..., true, ...), the edge-faded mapped Original;
   - when no authored gain/mix exists, there is nothing additional to apply. That restricted shape's canonical authored bus is the edge-faded output.
   This does not make the two labels interchangeable for arbitrary projects.

4. crates/deadpan-plan/src/plan.rs:997 and audio_owners.rs:283:
   - has_audio_treatments is true for any node with nonempty audio_treatments or a Repeat escalation with non-unity gain.
   - sounds/beat_sounds and the accessor expose the exact predicate needed by this test.
   - adding EQ/gain/a sound merely to obtain the old label would change the fixture and is unnecessary.

5. EdgeFadedBlock docs in deadpan-audio/src/stages.rs:323:
   - stage/processing_order identify the selected preparation path;
   - neither path includes the final limiter.
   ProjectAudioSession::read_limited remains the canonical audition limiter path; it must remain separately read and bit-compared.

## Why the one-leaf fixture selects the edge label

The intended baseline is the qualified Original shortened to [12,42), with no authored FX, independent root sounds or BeatSounds. The operation inserts an ordinary15frame Freeze at15 with HoldAudio::Silence; it does not author gain, EQ, independent sounds or non-unity Repeat escalation. Forward/inverse/history parity already checks that undo restores the prior authored state and redo the paused state.

Thus the contract predicts the edge label before/after and after undo/redo. Do not rely on that prediction alone: every new one-leaf full-reader session should independently assert its plan has empty sounds, empty beat_sounds and !has_audio_treatments before reading blocks. A changed fixture must fail these assertions, not silently accept a different label. All sessions still call read_authored_bus, so no stage bypass is introduced.

The older aligned/neutral fixtures include actual authored treatments and/or independent sounds. Their full reader's exact authored_bus_pcm_before_mastering requirement is appropriate and must remain enforced.

## Recommended minimal test-only patch, after assignment

Ownership needed: only crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs. No production change or export-wrapper change is needed.

1. Extract the current full reader body into a private read_exact_with_stage(path, partitions, expected_stage: &'static str) helper. Keep every current bounds/length/finite/limiter-gain and sample-bit extraction assertion and exactly the same read_authored_bus/read_limited calls.
2. Leave read_exact(path, partitions) as the legacy wrapper passing only authored_bus_pcm_before_mastering. Every old aligned/neutral caller remains unchanged. Its logical stage assertion remains exact, even though the common assertion now reads assert_eq!(authored.stage, expected_stage).
3. Add a one-leaf-only wrapper (e.g. read_one_leaf_exact) passing only edge_faded_pcm_before_voice_effects. In the common reader, for this explicitly selected edge expectation, assert plan.sounds().is_empty(), plan.beat_sounds().is_empty(), !plan.has_audio_treatments() once after opening the existing session. This adds no second open/decode. Optionally assert the exact edge processing_order per block as additional proof; do not change the old wrapper's requirements.
4. Replace only the six full-reader call sites inside one_leaf_replay_qualified_media_fixture with read_one_leaf_exact: baseline cached, baseline portable copy, initial paused cached, published cached/uncached partition loop, undo loop and reopened-redo loop. These represent nine current full-reader invocations. Leave all other read_exact callers untouched.
5. Keep all canonical authored/limited/limiter-gain bits, length/finite checks, partition/cold comparisons, source nonzero and pause-interior silence controls, Store/Host/capability/history checks, literal picture fixtures, both exports and external receipt unchanged.

There is no stage allowlist. The expected stage is selected by a fixture-specific wrapper, not taken from the returned block. The edge expectation is additionally justified by independent plan-shape assertions. Do not replace read_authored_bus with read_edge_faded or relabel returned data.

## Cold-read behavior

Current cold_exact at aligned_group_recipe.rs:93 has no stage assertion. It calls the same read_authored_bus and read_limited APIs on a freshly opened session, then compares every returned authored/limited sample and limiter gain bit to the full-read oracle.

Keep cold_exact and all its existing callers unchanged for the narrow fix. Stage selection is plan-wide under the exact predicate above, and the one-leaf full reads independently prove that shape for each published/reopened document. Cold checks retain actual bit equality through the same authored API; they do not assume the processed-stage label.

If a separate cold-stage assertion is later desired, add a one-leaf-specific strict wrapper/helper without loosening the old cold checks. It is not needed to repair this failure, and an accept-either-label test is not recommended.

## Verification owed to Root

- Source review must map the old exact tag requirement through the legacy wrapper; the original62 logical assertions remain enforced, and the new plan-shape assertions are additions.
- Broad compile must include every integration target that imports recipes; the shared helper must retain zero preview_export crate-root dependencies and no #[test].
- Run the isolated release group_endpoints_export::one_leaf_replay_qualified_media_commit_export_matches_uncached test with the existing pinned media/encoder environment, DEADPAN_REQUIRE_PHYSICAL_MAC=1, DEADPAN_PREVIEW_EXPORT_KEEP=1 and the external receipt path.
- Confirm actual successful Some/None publication, complete48048/72072sample comparisons, strict stage/proof checks before/after/undo/redo, both decoded45picture/full-audio exports, zero offsets and retained receipt.
- Retain the known old aligned/neutral stage assertions and their existing verified runs; Root decides rerun scope after the extraction's source review.
- No runtime duration, positive publication result, export success or production defect is newly claimed by this read-only study.
