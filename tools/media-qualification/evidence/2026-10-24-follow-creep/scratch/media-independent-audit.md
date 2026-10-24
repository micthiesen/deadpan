# Independent retained-media audit

Parsed only supplied invocation/terminal records and retained raw reports, PCM arrays, copy report, source file, and emitted movie. No product command, build, or test was rerun.

- Test: follow_creep_export::follow_creep_preserves_live_motion_lost_correction_fallback_and_verified_av; test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 66 filtered out; finished in 5.29s; terminal exit 0, binaries unchanged.
- Video: 120 sequential frames checked; 0 failures; 0 nonzero picture offsets. Movie SHA-256 171a987281d70bd0b7af1bae7b6e6d8534e08e9afccee959cda74ee0d0e3f0a1 (211945 bytes), matching render receipt.
- Audio: stereo [0, 192192), 192192 samples, no uncovered samples, verified_zero offset at 0; PCM bit arrays match before for authored, limited, and limiter gain. Nonzero authored pairs 2816; limited 2816. The retained source-click window [47808, 48192) is nonzero; pair 48000 decodes to (0.749076783657074, -0.649470865726471). This confirms PCM presence only, not listening. No independent DSP claim.
- Pose evidence: 120 rows; branch counts {'fallback': 20, 'tracked': 60, 'lost_holds_region': 20, 'manual_correction': 20}; every actual pose equals expected. Target samples include motion, Lost, later tracked sample and manual correction; source pixels are the counter fixture, not a real moving subject.
- Portable copy: {'replayed': 0, 'revisions': 7, 'verified_by_receipt': 7}; Original cloned; undo state equals before and restored copy state equals after after excluding revision_id. Authored history separately reports {'replayed': 6, 'revisions': 7, 'verified_by_receipt': 0}.

## Checks

- PASS test_1_passed_66_filtered_not_skipped
- PASS terminal_exit_zero_and_binaries_unchanged
- PASS raw_result_equals_summary
- PASS 120_sequential_pictures
- PASS picture_failures_zero
- PASS picture_offsets_zero
- PASS audio_full_192192_stereo
- PASS audio_offset_verified_zero
- PASS pcm_authored_bits_equal
- PASS pcm_limited_bits_equal
- PASS pcm_limiter_gain_bits_equal
- PASS pcm_signal_nonzero
- PASS pose_120_rows_and_oracle_matches
- PASS portable_history_7_verified
- PASS portable_original_cloned
- PASS undo_state_matches_before_except_new_revision
- PASS copy_state_matches_after_except_new_revision
- PASS source_binaries_match_initial_and_terminal
- PASS movie_hash_matches_render_receipt

## Limitations
- Existing counter video, not a naturally moving subject. Target samples and correction are deterministic and manually authored.
- Evidence checks tracking-state consumption and independent Q32 pose arithmetic, not tracking accuracy.
- PCM arrays compare retained IEEE float bit patterns (authored/limited/gain) against before baseline. This is not an independent DSP oracle and makes no listening or audibility claim.
- Undo and copy state compare equal after removing the intentionally different revision_id. Hashes identify audited files; they do not independently attest how invocation records were produced.

## SHA-256 inventory

- 0fb0502518ac864580edcb9b0bf2a7d72fe4d486cc292ec1fbf27c74e7dec1f5 (1458 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-first-invocation.json
- fd8d7e11d9662e388249304623123317b21ae0c63de15f2d44d5854b13be7307 (91 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-first-terminal.json
- aada9fb48ae5a05a0c66095d58a859294f4a747d09c20da8a1795879110240ef (1071 bytes): /tmp/deadpan-follow-creep-2026-10-24/first-release-binaries.json
- f1361736ebfa33574e3a07d5cbd4f3e2b74d17f78acdce5d5f005527ef939d9f (222 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-first.stdout
- 9e48ba044be2d31b10030283f7a29708ca15e7a9341b4ae899b29d7e9d1e02ac (2227 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-first.stderr
- e1a9b7a6f2a8caa3a133c34a4efa1f3f6d0a3735bcd56ba855b28be013f8ad53 (2336 bytes): /tmp/deadpan-follow-creep-2026-10-24/media-first-result.json
- e1a9b7a6f2a8caa3a133c34a4efa1f3f6d0a3735bcd56ba855b28be013f8ad53 (2336 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/follow-creep-summary.json
- f62e052a545d3e5ed2145722f15b965e2697022713d1a928929e3b28a05bacc9 (162647 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/follow-creep-verification.json
- 830f883779ebfc97be6a28e3034b368459d5945a142669d69a3628da6590c1ae (162648 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/follow-creep-verification.stdout.json
- e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 (0 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/follow-creep-verification.stderr.txt
- ad8b91f0ff2cc81e9ebc69a29d79b62c0f8e7396a77b4b4e207fddb7d3efba15 (15193 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/follow-creep-render.stdout.jsonl
- e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 (0 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/follow-creep-render.stderr.txt
- de16416e7cde8a87ca849e4bc95f4e7787c39ee2d7280f82f6ab0c47b18bb330 (32960 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/exports/deadpan-render-dcd88887-cbed-4221-8fe2-371b0b0ca6a9.json
- cfc22dfff4372334d4f8de23ba3b404d85665516385d61a1086163adf8415ce3 (129978 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/follow-creep-authored-evidence.json
- 2ae6bc9a5a406c7839f9250045c0f04a4c2767eba681f7d1bb73002d27c9acd8 (6246730 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/follow-creep-before-pcm.json
- 10cb6b31edd1cac052eb4956440321b723175be9efe4bda16107e4d138d7dadd (6246627 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/follow-creep-after-pcm.json
- cf860ca8f16235088c4ad7b7479ad822375104575a67362895061ce2c798d52e (894 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/follow-creep-portable-copy.json
- 0c30b01e8b54d7fdc00ee580ffa1ce2f250ed1ab66c3bf5b4c6afabc33f3b72a (2282 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/follow-creep-apply.stdout.json
- e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 (0 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/follow-creep-apply.stderr.txt
- 6ed8595dc7871ad679e06a81f797d860fc19ef2133f0eded360d050eade715d9 (1103 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/request-follow-creep-r01.json
- f07134681cc886e418847f00a4c3bba8e8163f7be7e46de24f9d7eea80ae0eaa (479 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/request-follow-creep-r02.json
- 5253f9ca905a4fa0cc5c174bd739d3ec4af01d06d6ce7cf77434dd6a8088fd4b (447 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/macro-request.json
- 5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918 (39157 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/fixture/original.mp4
- 171a987281d70bd0b7af1bae7b6e6d8534e08e9afccee959cda74ee0d0e3f0a1 (211945 bytes): /private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C/exports/follow-creep.mp4
