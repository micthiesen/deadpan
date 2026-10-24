# Final fresh-run media audit

Parsed retained final-run records, media reports, PCM evidence, source files, and emitted movie only. Did not rerun product code.

- Test: follow_creep_export::follow_creep_preserves_live_motion_lost_correction_fallback_and_verified_av; test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 66 filtered out; finished in 5.25s. Exit 0; terminal records binaries and runtime sources unchanged.
- Final retained binary hashes: four required helper/CLI binaries and the test executable match invocation digests. CLI SHA-256 e9ad52a23d3f91ac198007f18e69811d0ec09797e24c53230255f4e083150701.
- Runtime source freeze: 11 paths; before, after, and freeze maps equal; invocation freeze SHA-256 matches file.
- Video: 120 sequential pictures; 0 picture failures; 0 nonzero offsets. Movie SHA-256 fb5a58fbce268699889f67208433616e3aeb50d3f5fdbb7d81f84992fb5b6238 (211945 bytes), matching render receipt.
- Audio: stereo [0,192192), full coverage, verified_zero offset=0. Authored, limited, and limiter-gain arrays match bitwise to baseline. Nonzero click window [47808,48192); sample 48000 decodes as (0.749076783657074, -0.649470865726471). This does not claim listening or independent DSP correctness.
- Pose: 120 exact per-frame rows; branches {'fallback': 20, 'tracked': 60, 'lost_holds_region': 20, 'manual_correction': 20}; actual equals expected for all frames. Counter footage and manually authored regions do not test tracker accuracy.
- Portable copy: {'replayed': 0, 'revisions': 7, 'verified_by_receipt': 7}; Original cloned. Undo and copy restore compare to baseline/final state when revision_id is excluded.

## Checks

- PASS test_1_passed_66_filtered_not_skipped
- PASS terminal_exit_zero_binaries_and_sources_unchanged
- PASS final_cli_hash_is_e9ad
- PASS all_four_retained_binaries_hash_match
- PASS retained_test_hash_match
- PASS current_sources_exactly_match_freeze
- PASS result_equals_raw_summary
- PASS 120_sequential_pictures
- PASS picture_failures_zero
- PASS picture_offsets_zero
- PASS full_stereo_audio_192192
- PASS audio_offset_verified_zero
- PASS authored_pcm_bits_equal
- PASS limited_pcm_bits_equal
- PASS limiter_gain_bits_equal
- PASS click_window_nonzero
- PASS pose_120_rows_and_exact_oracle
- PASS portable_copy_seven_receipt_verified_revisions
- PASS portable_original_cloned
- PASS undo_matches_before_ignoring_revision_id
- PASS portable_restore_matches_after_ignoring_revision_id
- PASS movie_hash_matches_render_receipt

## Limitations
- Existing burned-in counter video; target samples and correction are deterministic and manually authored. This verifies resolver/pose behavior, not tracker accuracy.
- Exact PCM float-bit equality is a before/after consistency check, not an independent DSP oracle. Nonzero PCM confirms sample presence only; no listening claim.
- Undo/copy state equality excludes revision_id, which necessarily changes across undo and portable copy. Hashes establish audited file identity, not provenance beyond the supplied invocation and terminal records.

## SHA-256 inventory

- efcb68a289f5165646a6676ffb1e23a7a3f81fe763ff0f893ba3aa9d3864e547 (1720 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final-invocation.json
- 7b573b76a214dfad0c430e1904e334f48f3457f19db802fa6492af9126feb13b (128 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final-terminal.json
- d3ff6ce64cb106a82b760534ce9bdac4c16437a974090130b084cf719a568a2a (2339 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final-result.json
- e64ad2a95c9e2df1e9c3ad249cb58647e467a69c4837fa454bc5350b906cddaf (222 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final.stdout
- f53148e71405c4b3480ed7b54e8a12e4fc043c7c726633c0867a64bb07387f2d (2230 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final.stderr
- cd45a91c008fa103c06aacb133f4d2eb63ff149803764cc7771de38f32ac9400 (1278 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final-source-before.json
- cd45a91c008fa103c06aacb133f4d2eb63ff149803764cc7771de38f32ac9400 (1278 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-final-source-after.json
- cd45a91c008fa103c06aacb133f4d2eb63ff149803764cc7771de38f32ac9400 (1278 bytes): /tmp/deadpan-follow-creep-2026-10-24/runtime-source-freeze.json
- d3ff6ce64cb106a82b760534ce9bdac4c16437a974090130b084cf719a568a2a (2339 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/follow-creep-summary.json
- ef9265a1875f8d95900b9c70fd709ac697ce7ca07cd97bde20bfbcee5c030d03 (162647 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/follow-creep-verification.json
- 36e72094a438fec6e9e82a9dd530cc18aa1a46e42ba46595a75dff9c868a7c53 (162648 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/follow-creep-verification.stdout.json
- e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 (0 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/follow-creep-verification.stderr.txt
- 2419dc9f5229f8a13d6a085409721a95b29f1e2f51941291ebd316c1bf8a5411 (15193 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/follow-creep-render.stdout.jsonl
- e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 (0 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/follow-creep-render.stderr.txt
- 1e2a655ddc8545501b7f1d7ce43fbb8643a3488e3e620fffe29a2d9a93557a13 (32968 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/exports/deadpan-render-e10aaedd-4d66-4b6f-828b-4a37a5c0aa69.json
- 494d12651157269752adf80d6186a899b2c603a53210cfae92000b203f25f19d (129978 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/follow-creep-authored-evidence.json
- 2ae6bc9a5a406c7839f9250045c0f04a4c2767eba681f7d1bb73002d27c9acd8 (6246730 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/follow-creep-before-pcm.json
- 10cb6b31edd1cac052eb4956440321b723175be9efe4bda16107e4d138d7dadd (6246627 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/follow-creep-after-pcm.json
- 9e40c1548dab3591ba9e4cf8658de124b9e9ff5088d70b9bdb1fd81afab2450f (894 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/follow-creep-portable-copy.json
- 122a7d4650a381695cbe9855fb4d695e8112f3b5c2d4143fe51704d36759e317 (2282 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/follow-creep-apply.stdout.json
- e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 (0 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/follow-creep-apply.stderr.txt
- 68d18a6e6b504d73f8cb2754d1a9be34591bbe952d06156b203b556bf781ba20 (1103 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/request-follow-creep-r01.json
- c6e10dc6dd99cab61e1cb84de299b8f1cf121627394534a7371e117b0cd1e5a7 (479 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/request-follow-creep-r02.json
- 12e58e50ddb38eaabc195808c05f1dc36e3c023844c04443191bf35a72c79c4f (447 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/macro-request.json
- 5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918 (39157 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/fixture/original.mp4
- fb5a58fbce268699889f67208433616e3aeb50d3f5fdbb7d81f84992fb5b6238 (211945 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG/exports/follow-creep.mp4
- 6293b75721dd84941ce2646a9e17d3d46ee8e221737c76d1d82e7ae5d9d999b5 (35901584 bytes): /tmp/deadpan-follow-creep-2026-10-24/final-media-retained/preview_export
- e9ad52a23d3f91ac198007f18e69811d0ec09797e24c53230255f4e083150701 (50018096 bytes): /tmp/deadpan-follow-creep-2026-10-24/final-media-retained/deadpan-cli
- 9bb776f74a23a3c875c52251770c161bfc2b0b49146891aa40de9578683424f7 (1169872 bytes): /tmp/deadpan-follow-creep-2026-10-24/final-media-retained/deadpan-media-worker
- 351468f98590062f06b6913d94ccbf2b07b019d1185c2939f09c8650c91f71d5 (2603984 bytes): /tmp/deadpan-follow-creep-2026-10-24/final-media-retained/deadpan-transcribe
- 11aeb6de474b1e6815146f2db4c826ae8721a8d3409607e0dee87b3a212c23c8 (1656592 bytes): /tmp/deadpan-follow-creep-2026-10-24/final-media-retained/deadpan-track
