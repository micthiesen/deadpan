# Generated media storage qualification, 2026-09-21

This is a developer qualification of content-addressed generated-media storage. Two already-qualified FFV1 masters were published into a project package, relocated, and read back by typed BLAKE3 reference without consulting the worker inputs. The storage API does not validate media, register an asset, or accept a candidate. A separate decoder verifies the readback pixels; this is not a release qualification.

The complete command record, typed references, hashes, source hashes, environment, and independent FFV1 verification output are in the [storage evidence report](../../tools/media-qualification/ffv1/results/generated-storage-2026-09-21.json). The storage example is [qualify_generated_storage.rs](../../crates/deadpan-store/examples/qualify_generated_storage.rs).

## Host and inputs

- Apple M5 Max, arm64 macOS 26.5.2, build 25F84, 128 GiB unified memory.
- Storage limit: 64 MiB per object; copy buffer: 64 KiB; reference JSON read bound: 16 KiB.
- FFmpeg source and decoder evidence came from the isolated LGPL FFmpeg 8.0.3 prefix recorded in the [FFV1 qualification report](../../tools/media-qualification/ffv1/results/qualification-2026-09-21.json). The storage example did not invoke FFmpeg or a model worker.
- Upstream candidate MP4, before FFV1 conversion: SHA-256 `d7fcf04e2bbe213d0352153443eb77c4a1534855ab8c19c0ff5dde4f2ddf85d9`.
- Upstream native 25-frame MP4, before FFV1 conversion: SHA-256 `c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33`.

The actual storage inputs were the canonical FFV1 masters, whose SHA-256 values
appear in the result table below. Private copies were promoted and removed before
readback. The original qualification inputs remained untouched; this run did
not repeat conversion or inference.

## Result

A fresh package received both objects through `ProjectStore::promote_generated_object`. The package was moved from `original.deadpan` to `relocated.deadpan`. Separate read-only invocations then consumed only the relocated package and each reference JSON file.

| Object | Bytes | BLAKE3 digest | Canonical master and readback SHA-256 | Independent FFV1 probe |
|---|---:|---|---|---|
| Candidate snapshot | 5,450,240 | `d14a394d0deb6900bb54522c53d38301e7f79063c56f9b4d26763b539b11e00d` | `a6dc975063db480471d111b1e821c9f3931e57d3a330808751667709aa8b1dda` | 30 frames, exact RGB pixels and tags |
| Native sequence | 4,544,157 | `b5b5955bf988496908e057be1bfd06bb4f3bda15ee725804faa2299fdba9b31a` | `a04d7778169328def88878e90cbf8b06629c612c5efdeddae7061efc302fde11` | 25 frames, exact RGB pixels and tags |

Each readback length and SHA-256 matched its canonical master. The example also recomputed BLAKE3 while streaming the verified snapshot and compared it with the typed reference. The FFV1 probe independently confirmed frame count, every RGB pixel, FFV1 v3 with slice CRC, full-range GBR/sRGB/BT.709 metadata, and the recorded source versus Matroska timebases.

The reference JSON uses the validated storage wire shape, including the explicit `blake3` algorithm and digest. Publication and readback are bounded and use create-new output/reference files with `sync_all` before reporting success.

## Reproduction

Use the pinned project toolchain and the existing retained masters. The example creates a package when the package path does not exist and opens it read-write on subsequent promotions:

```sh
cargo check -p deadpan-store --example qualify_generated_storage --locked
cargo run -p deadpan-store --example qualify_generated_storage --locked -- \
  promote /tmp/generated.deadpan /path/to/candidate-snapshot.mkv /tmp/candidate.json
cargo run -p deadpan-store --example qualify_generated_storage --locked -- \
  promote /tmp/generated.deadpan /path/to/native-25-sequence.mkv /tmp/native.json
mv /tmp/generated.deadpan /tmp/generated-relocated.deadpan
cargo run -p deadpan-store --example qualify_generated_storage --locked -- \
  read /tmp/generated-relocated.deadpan /tmp/candidate.json /tmp/candidate-readback.mkv
cargo run -p deadpan-store --example qualify_generated_storage --locked -- \
  read /tmp/generated-relocated.deadpan /tmp/native.json /tmp/native-readback.mkv
```

The final run used `/tmp/deadpan-generated-storage-final-ak64vd61`. The evidence
contains ten execution receipts with actual argv, environment overrides,
resolved executable, exit code, stdout/stderr, and log hashes. It binds the final
storage source, store wrappers, error mapping, Cargo manifests/lock, Rust
toolchain, and tested executable. Storage invocations used a retained copy of
the executable so subsequent Cargo checks cannot replace it. The independent decoder checks used the
retained probe binary and raw fixtures from the FFV1 report.

## Boundary

This run qualifies byte identity and package relocation for the tested objects. It leaves media readiness, semantic validation, project asset registration, acceptance/promotion policy, model provenance, eviction, hostile same-user process isolation, and release packaging open. The storage object is not a media-validity claim. Matroska's physical output timebase remains `1/1000` in the FFV1 evidence; the exact source rational clock and ordinal mapping remain recorded separately.

Linux, physical power-loss recovery, filesystem disk-full injection, and
process-kill recovery were not exercised. Unit tests inject publication and
durability errors; those tests do not simulate hardware power loss. No GUI,
audio device, model inference, or end-user distribution was exercised.
