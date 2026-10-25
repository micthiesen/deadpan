# Native picture proposals handoff

Native Cargo lane released. No Cargo/native/UI process active. No Git, worktree,
Cargo.lock, App, CLI or jobs edits were performed. Shared target and qualified
FFmpeg8.0.3 prefix were used for all product builds/tests. Final source copies
are in native-final-source; native-final-freeze.json binds every owned path.

## Implementation

The private faces worker mode is replaced by canonical jobs picture-proposals.
Faces remains a selected mode through the existing public CLI facade. One source
verify, exact seek/metadata selection, decoded RGBA conversion, clean aperture
and square-pixel raster preparation feeds one BGRA buffer, one image handler and
one synchronous perform of the selected requests. Both returns a complete typed
batch or a failure. No source proxy, heatmap payload, saliency score or detector
fallback is introduced.

Face constructor/default revision remains unchanged and measured revision3 here.
Objectness saliency is explicitly revision2, checked against the concrete class's
supported revisions before performing and checked again on return, including
successful empty results. At most1 image observation and64 salient rectangle
observations are admitted before iteration. Lazy salientObjects is consumed while
the observation/request/pixel buffer and autorelease pool remain live. Geometry
rejects invalid numeric values/size, clips in coded space, converts lower-left to
top-left then rotates to upright, sorts and exactly deduplicates. Cancellation
matches request+attempt+token; EOF/read failure cancels. No partial completion.

The only added Cargo feature is VNGenerateObjectnessBasedSaliencyImageRequest.
All new unsafe code remains in the qualified vision.rs boundary.

## Owned paths

- src/faces.rs removed by rename to src/picture_proposals.rs.
- src/main.rs dispatch and vision.rs adapter.
- Cargo.toml one feature plus description/comment.
- tests/faces.rs facade wording and current TargetGeometry assertion conversion.
- tests/picture_proposals.rs four real integration tests.
- tests/generate_salient_images.swift and generate_salient_fixtures.py.
- tests/fixtures/SALIENT_REGIONS.md and9 salient-* fixture/manifest files.
- Additional explicitly authorized stale-test reconciliation: tests/worker.rs
  has region.as_region().expect before identical center/size assertions,
  scale_transition:None in existing Follow, wrapped TargetRegion equality;
  tests/geometry.rs uses region.center(). Old inputs/tolerances/assertions stay.
  Exact stale sources accompany failed native-regressions-1.

## Final evidence

native-regressions-2: complete Native package35 tests pass (bin8,
extension_landmarks4, faces5, geometry2, landmarks6, picture_proposals4, worker6).
native-clippy-1: locked all-target strict -D warnings passes.
Those bind the pre-determinization fixture headers and exact Rust sources.
native-final-proposals-1:4 actual proposal/index/audio/geometry tests pass AGAIN
with final deterministic fixture bytes. Receipt confirms source+basis unchanged.
native-final-format: seven owned Rust files pass scoped rustfmt --check.
fixture-reproduction-v4 and fixture-reproduction-final both exit0, all9 generated
files byte-identical. Final generator changes after v4 only flush exact argv/
stderr records after each subprocess; final reproduction binds that source.

Product worker executable in final focused run:
  /tmp/deadpan-follow-creep-2026-10-24/target/debug/deadpan-track
  SHA256 20e4849431bbbfd06e15476aae004a9cb79c7358abf47f5d662ba0927e1310c8
  8714400 bytes
It is the debug worker. Root owns any final release build/UI/packaging checks.

Real successful detector results (saliency2, default faces3) include:
- blank PTS0: faces[], salient_regions[].
- mug PTS1335: faces[], salient x=.5400390625,y=.32861328125,
  width=.30126953125,height=.4365234375.
- two-object PTS2669: faces[], two salient regions:
  [.1575927734375,.324951171875,.2852783203125,.448974609375] and
  [.52001953125,.284423828125,.263671875,.472900390625].
- old drawn-face fixture Both retains exactly the same2 face bounds/confidences
  and revision as Faces-only. Combined call also returns separate salient bounds.
- actual SAR, clockwise90, and fractional-aperture saliency positives pass
  upright geometry checks. This is coarse bounding-box evidence, not masks.

## Final fixtures

Original Swift/CoreGraphics drawings, MIT repository license, no external media.
Producer is Homebrew FFmpeg9.0.2/GPL-enabled build; full version/build/license
and exact options retained. It is not claimed as the pinned8 product producer.
Product8 actually decoded every video picture and exact WAV PCM below.

salient-regions.mkv:635038 bytes SHA256
cc181467a19d92c3b08edf72ff34d4eaac6d4887e8e05311305ed51db7576f66.
salient-regions-sar.mkv:635050 bytes SHA256
86f4b5e08e0c4851811bc5a8412bd43eb7c94e8cf9d74b8cf775142d23e078f3.
Both120 video-only lossless FFV1 yuv420p,640x360 limitedBT709,
nominal30000/1001, measured tbn1/1000, all120 PTS/durations checked.
Sections[0,40)blank,[40,80)mug,[80,120)two; starts0,1335,2669.
Last PTS3971,duration33,terminal4004ms. SAR1/1 or4/3, display16/9 or64/27.

salient-audio.wav:768812 bytes SHA256
8f2ede07d4e8aa6743b6f441f5fb3cc6c6347a333f7b4ede15c52315f7195089.
Stream0 PCM16LE stereo48000,exactly192192 samples/channel,4.004s.
Actual qualified AudioSession output reconstructs every PCM byte; SHA256
76d5ac51af40207c6c9d1528aeeb95a48fe2333f7340b007ea4c7c6bf50e7053.
This is a separate audio catalog fixture, no embedded-audio assertion.

Geometry MP4s retain4 H264 lossless YUV pictures each, exact tbn1/30000,
PTS0,1001,2002,3003,duration1001/terminal4004. Rotation CW1/aspect9/16.
Fractional aperture left641/4,top81/4,width959/2,height639/2,
exact aspect959/639. Full hashes/source PNG hashes in frozen manifest.

## Preserved failures and limits

native-real-1: test compile corrected sha2 hex formatting and Result unwrap.
native-real-2: PCM Matroska rejected by product container admission.
native-real-3: producer Opus Matroska failed complete admitted stream inventory,
and raw native decoded frames192960 differed from192192 intended valid samples.
These candidates remain in fixture-generation-v1/v2 and run source copies;
nothing in production source/container admission was changed for them.
native-regressions-1: pre-existing stale TargetGeometry/Follow test APIs above.
fixture-reproduction-v3:42 random Matroska header bytes differed. Corrected by
output-side -fflags+bitexact, not by masking or accepting mismatched hashes.
Every failure remains recorded separately, never relabeled as a pass.

No release build, UI replay, second-Mac/older-OS capability, arbitrary-object
accuracy or performance qualification is claimed. Revision2 fits actual app
minimum15.0 and declared API support14+, but this runtime was macOS26.5.2.
Source evidence alone establishes one buffer/handler path; no claim of shared
internal neural preprocessing or measured performance improvement.
