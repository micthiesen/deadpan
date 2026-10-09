# Offline and cross-volume Original relinking

The opt-in `relink_cross_volume` integration harness passes on macOS 26.5.2
(25F84), arm64, against source base `5e0993ab` plus this qualification change.
It uses the pinned FFmpeg 8.0.3 prefix and the committed `cfr-bframes.mp4`
fixture (SHA-256 `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`).

Two private 128 MiB APFS disk images establish different filesystem devices.
The fixture creates a linked-only Original on A and a protected single-Original
baseline with durable redo history. After dropping store and decoder handles,
it detaches A and proves that the Original is unavailable. Ordinary CLI
verification reports `OriginalOffline`; bookmark recovery does not remount A.
A changed-byte candidate on B is refused as `OriginalContentMismatch` without
changing saved rows. Explicit relinking to an identical renamed copy on B
succeeds and preserves authored, history, redo, profile and qualification rows.
Reopened frames 0, 59 and 119 retain identical PTS, identities and decoded pixels.
A remains detached throughout, and both owned images are confirmed detached
before success is reported.

Run with:

```sh
DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix cargo test --locked -p deadpan-cli --features qualification-relink-volumes --test relink_cross_volume -- --nocapture
```

Both tests passed in 6.86 seconds, including inventory-path refusal checks.
Focused Clippy with `-D warnings` passed. Independent review found no actionable
issue. Evidence is retained at
`/private/tmp/deadpan-relink-volumes-20261008/run-QfE2is`, including command
receipts, before/after rows, decoded-picture hashes and cleanup confirmation.
CLI SHA-256: `5140364be9f91e3b328d7d7460defda8a615164ef7f207dfa8e116ed4e457fba`.
Test SHA-256: `64f818de3211bf53bb99f851f39141310364c3ad1cf24d816b790ff4b4065d14`.

The release configuration also passed both tests in 4.06 seconds, using the
same command with `--release`. Evidence:
`/private/tmp/deadpan-relink-volumes-20261008/run-kmRXfU`.
Release CLI SHA-256: `695963d390e4b2e3e4133a9cade62e49c13dffaaebf9823bbfe745c6ae888b7c`.
Release test SHA-256: `6d15f5bb67d841b3861a1bf2f0c05884c46f873a57680a950e2c1d1fcf76ea24`.

The initial harness failed before image creation because `hdiutil create -type`
requires `UDIF`, not the format name `UDRW`. Removing `-quiet` preserves useful
failure diagnostics. A diagnostic probe also confirmed that `-format UDRW`
requires a source folder/device and is not the blank-image API.

This proves the store/media/ordinary CLI workflow on private volumes on this Mac.
It does not establish native UI behavior, physical removable-device behavior,
VoiceOver, or preservation of separately populated analysis corrections and
Original provenance. Those tables are outside this fixture's row comparison.
DP-01 remains partial for named history branches/takes; this evidence does not
complete DP-15's broader import and format matrix.
