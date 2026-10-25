# Exact-CFR non-face fixture follow-up

All Native repository edits are frozen. Root owns subsequent optimized nextest
and release/native replay runs. No Cargo or product command ran in this follow-up.
Producer generation and byte reproduction ran with the already recorded external
FFmpeg9.0.2 build. No product admission policy changed.

The first native replay rejected the old millisecond MKV during default Original
cadence admission, before detector work. native-regions-first/report.json remains
the actual failure. Existing MKV/index/detector evidence remains explicit-profile
qualification and is not relabeled as native default-profile success.

## New fixture

native/deadpan-track/tests/fixtures/salient-regions.mp4
585089 bytes; SHA256
aaffac5cc021297348101e3ce500a3febac48fc45021a511468cb3fd76784e93.

Measured producer metadata:120 pictures, video-only H264 all-intra lossless YUV,
640x360 yuv420p limitedBT709 (primaries/matrix/transfer), exact30000/1001 CFR.
MP4 tbn1/30000; each PTS1001*n/duration1001; last119119; terminal120120=4.004s.
Same original drawings/sections: blank[0,40),mug[40,80),two[80,120);
section PTS0,40040,80080. SAR unspecified means square per existing source policy.
Producer profile is High4:4:4 Intra even though actual pixel format is yuv420p.
Single encoder thread, qp0, explicit video timescale, faststart and output bitexact.

All existing MKV/WAV/geometryMP4/PNG bytes and hashes remain unchanged.
The separate WAV remains192192 stereo PCM16 samples at48000Hz.
Manifest schema is unchanged; fixtures[salient-regions.mp4] is added.
Manifest SHA256:
bcb45f5c135229159236a64376bdb0f5a35cf04b0007f1becd6db50abf4a51cf.

fixture-cfr-generation-1 and fixture-cfr-reproduction-1 exited0; stdout, stderr,
raw inputs, encoded outputs and exact argv retained in their separate directories.
The generator now skips rewriting already-identical retained files.

## Source/tests

5 paths: generator, newMP4, fixture manifest, fixture README, relevant Native test.
Before snapshots/hashes: native-cfr-before + native-cfr-before.json.
Frozen source copies: native-cfr-final-source.
native-cfr-freeze.json SHA256:
2511ec4051d45d4034a8453c2c81a80e4d04554493cb6185138075284f98c5f0.

New test exact_cfr_fixture_admits_default_original_and_real_proposals uses actual
CLI create-original without an explicit profile, asserts selected30000/1001 rate,
exact clock/three indexed section points and terminal span, real Both results,
face-zero counts, canonical runtime identities/objectness2, and no authored writes.
Existing qualified_decoder_preserves_generated_picture_and_pcm_clocks additionally
checks all120 new MP4 indexed metadata and RGBA pictures against measured manifest.
Scoped rustfmt parsed/formatted the owned test. No native pass is claimed yet.
Expected Native total37 (previous36+new1), proposal integration5. Root records
actual test count/results and any new failure independently.

## Scratch package driver adaptation

Prior exact script is retained at native-cfr-before/package-checks.py.txt.
Current package-checks.py no longer hardcodes MKV, PTS or source timebase.
Config supplies pinned fixture + measured fixture_manifest; build/runtime validate
the actual per-filename manifest entry and conserve both input hashes.
Six actual pinned baseline reports supply their own selection/PTS/picture
identities. Their ordinals must cover both modes at each measured section start;
PTS/timebase must agree with the selected fixture's actual index. Semantic output
equality remains exact except4 elapsed fields; Regions-only and Both compare
per measured picture. The skipped official verifier receipt names actual configured
fixture/copy paths. No packaging/product/Cargo command ran.

Updated driver SHA256:
4c4fa4a2901a8d3f5c574c47806a057756f6f3b30c605dfc057c4847e79d6dad.
package-cfr-preparation.json retains current pins and static actual-manifest checks.
The example config now names newMP4/manifest hashes and explicit rotation0.
Build still needs final source manifest; runtime still needs actual retained native
project/report pointer, exact identities and actual six baseline JSON/reference
CLI+worker pins. Prior package-plan.md/package-driver-handoff.md are historical
pre-MP4 plans; this note supersedes their hardcoded input/clock sections only.
No official full bundle/AI/runtime/release qualification is claimed.
