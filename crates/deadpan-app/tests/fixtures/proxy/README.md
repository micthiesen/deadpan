# Proxy fixture

`cfr-bframes.proxy.mp4` is the preview proxy of
`native/deadpan-source/tests/fixtures/cfr-bframes.mp4` (320×180, 120
pictures), produced by the real isolated worker:

```sh
DEADPAN_PROXY_FIXTURE_DIR=$PWD/crates/deadpan-app/tests/fixtures/proxy \
  cargo test -p deadpan-media-worker --test proxy_real_media every_proxy
```

then renamed from `cfr-bframes.mp4.proxy.mp4`. It is VideoToolbox H.264
intra (proxy recipe 1, quality 60, H.264 VUI sample aspect written by `h264_metadata`) with the Original's exact timestamps. App
tests verify it against the Original with `verify_proxy` and publish it
through the proxy cache, so they do not need the media worker binary.
SHA-256 `671b26671bea4413d0f1d32140d4269a03d07417840a89fbc359340cf7d932c8`.
