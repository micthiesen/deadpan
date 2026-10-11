# YouTube CONNECT fixture

`youtube-connect.bin` is the exact 116-byte HTTP CONNECT request captured from
the pinned yt-dlp 2026.08.19 on 2026-10-11. The owned loopback proxy returned
403 immediately, without contacting YouTube. `deadpan:fixture` is an
intentionally public fixture credential, never a live gateway credential.

The source metadata named `https://www.youtube.com/owned-fixture`; the helper
ran with `--ignore-config --no-plugin-dirs --no-cache-dir --retries 0 --proxy
http://deadpan:fixture@127.0.0.1:<owned-port> --load-info-json metadata.json
--format 137`. The capture script and result are retained in the
[network qualification](../../../../docs/qualification/youtube-network-2026-10-11.md).

`adversarial_youtube_connect` mutates this real wire input through the
production parser and authentication policy. `cargo xtask chaos --only
cli-boundaries` includes it alongside the other CLI boundary targets.
