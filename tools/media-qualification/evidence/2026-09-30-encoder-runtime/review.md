# Runtime binding source review

Reviewed the dirty tree over main `026ff88` on 2026-09-30. Read-only review; no repository changes, builds, tests, formatters, or native programs were run by this reviewer.

## Findings

No actionable correctness finding in the reviewed source.

## Checked boundaries

- `native/deadpan-encode/src/runtime.c:43`, `:139`, `:205`, `:217`: own-process readable, non-writable mapped regions bind header and function anchor to a regular vnode. Loaded Mach-O parsing is bounded to 4,096 commands and 1 MiB, requires one exact nonzero UUID, and checks the descriptor's slice CPU/type/UUID against the loaded observation. Metadata and mappings are checked again after reads. Matching bytes or UUID on a replacement inode do not confer loaded-image identity.
- `native/deadpan-encode/src/runtime.rs:171`: the live observation has private native state and no deserializer. Serialized mapped identity is a strict observation DTO. Opening uses nonblocking, close-on-exec and no-follow flags; fresh descriptor validation precedes use.
- `crates/deadpan-cli/src/encoded_render/runtime.rs:99`, `:136`, `:337`: capture pins all five matched descriptors, hashes under a 512 MiB per-image bound with cancellation/deadline checks, and retains them through work. Revalidation repeats mapped-object checks and hashes, then compares platform observations. A deserialized binding must match a fresh capture before use.
- `crates/deadpan-cli/src/encoded_render/admission/worker.rs:81`: the probe holds RuntimeCapture across native encode, emitted-file verification and content inspection. Revalidation applies to success and failure; a later runtime fault replaces a capability-eligible error with Contract failure. Failed controls carry no checked runtime.
- `crates/deadpan-cli/src/encoded_render/admission/host.rs:93`, `:328`, `:344`, `:456`: consuming the private fresh admission checks attempt/token, direct-helper configuration and runtime, then starts bound project encoding. Probe selection requires exact typed capability failure, checked runtime evidence and equality across attempts. Later protocol, process, cancellation or cleanup faults invalidate fallback eligibility. Common explicit process finalization remains in place.
- `crates/deadpan-cli/src/encoded_render/worker.rs:179`, `:381`: bound project encoding validates the fresh loaded runtime before opening project media, holds it through native work, and revalidates before successful completion.
- `crates/deadpan-cli/src/encoded_render/protocol.rs:26`, `:94`, `:346`, `:519`: protocol 3 requires an explicit binding value, including null for engineering execution. Completion must preserve the exact requested binding. Persisted EncodedManifest grammar is unchanged. Probe protocol 2 independently requires expected helper observations.
- `native/deadpan-encode/src/policy.rs:68`, `:91`: the old constructor remains an alias of frozen new_v1. CLI manifest reconstruction explicitly uses new_v1. No bitrate, rounding, GOP, B-frame or clock derivation change was found.
- `native/deadpan-source/src/decoder.c`: geometry edits add diagnostic stage and measured limits without changing admission thresholds.

## Test source inspected

- `native/deadpan-encode/tests/runtime.rs`: strict wire/scalar/platform bounds; actual mapped helper and linked FFmpeg images; descriptor cursor preservation; same-header replacement-vnode rejection; explicit unsupported-platform behavior.
- `native/deadpan-encode/tests/policy.rs:19`, `:121`, `:142`: literal version-one contract and rounding/rate derivation coverage.
- `crates/deadpan-cli/src/encoded_render/protocol/tests.rs:303`, `:348`: required explicit binding, old manifest grammar and exact runtime-bound completion matching.
- `crates/deadpan-cli/src/encoded_render/worker/tests.rs`: invalid binding fails before missing-project media preparation.
- `crates/deadpan-cli/tests/encoder_admission.rs:109`, `:153`: missing runtime, helper mismatch and a library change between probes stop selection; real supervisor fault/cancellation coverage remains.

## Verification limits

The parent reported 13 focused native tests passing. This reviewer did not execute them. At review time the parent was still adding the real qualified probe-to-project consumer and had not reported a complete CLI gate for this wave. That execution and any newly added consumer remain outside this report until a follow-up review.

The evidence identifies backing objects under trusted installed code. It does not hash relocated resident memory or claim complete OS framework, kernel, driver or hardware attestation. Native filesystem/kernel calls are cooperatively bounded, not preemptible. The source states these limits and does not use serialized observations to mint a fresh QualifiedEncoder. Durable automatic policy storage and public UI/DB integration are intentionally outside this wave.

## Follow-up: bound consumer and public contract capture

Reviewed `crates/deadpan-cli/examples/qualify_bound_encoder.rs` and the public visibility change at `crates/deadpan-cli/src/export_picture/contract.rs:89`. No actionable findings.

The example captures the explicit committed revision through a read-only picture session and drops that reader before worker execution. Admission and encoding share the original attempt identity, cancellation token, output raster/rate and outer deadline. It consumes the private QualifiedEncoder, compares the resulting decision and exact binding against the selected runtime, then uses a fresh verification attempt and confirms the verified candidate retains that binding. It never reconstructs live authority from the serialized report.

Probe, candidate and verified bytes are retained in newly created mode-0600 files under canonical `/tmp`, outside the project package. Each retained copy is checked for exact length and SHA-256 under the same deadline. The report distinguishes these stages, includes the selected/rejected runtime evidence, and propagates cleanup uncertainty from admission, encoding and verification errors. A successful report is written only after independent emitted-file verification and retention of the verified bytes. This remains an engineering example with no durable job or publication claim.

Making ExportPictureContract::capture public exposes the existing checked geometry/clock capture from an immutable ProjectPictureSession. Fields remain private and serialization remains one-way. The visibility change grants neither decoded-media admission nor output-file/publication authority and changes no derivation logic.

The parent now reports 13 native tests, 102 CLI encoded_render library tests and 12 supervision integration tests passing. This follow-up inspected source only and ran no checks. Real execution of this example remains the parent's verification task.

## Final follow-up: direct reference retention

Reviewed the added direct-reference call and preflight in `qualify_bound_encoder.rs`, plus the two mode-0600 output additions in `qualify_render_workflow/reference.rs`. No actionable findings.

The new preflight uses checked arithmetic for exact tight-I420 and stereo-f32 extents, rejects more than 540 frames and rejects either reference above 512 MiB before admission. The shared helper receives the same explicit revision, half-open range and original deadline. It checks picture identity during capture and exact audio interval agreement; OfflineAudioSession retains and checks the passed absolute deadline on reads. The caller checks the returned complete contract and both exact file lengths before verification. Reference names include a fresh UUID, and both outputs use create_new with mode 0600 in the already admitted private output directory. The only shared-helper behavior change is file creation mode.

Reference creation does not claim that encoded content matches those references. It retains direct inputs for the separate encoded-reader comparison, while the existing independent emitted-file verifier still gates this example's success. Deadline handling remains cooperative around native GPU/filesystem calls, as in the existing shared helper.

The parent reports an initial real project run passed with 128 frames and 205,005 samples, exact runtime continuity and unchanged database tables. Those results were not executed or independently replayed by this reviewer. The parent also reports tiny probes exposing coded geometry of 192 by 96 against the existing 256 dimension and 4,096 pixel budget; no admission bounds were loosened in this follow-up.

## Final gate correction: encoded verification fixture

The parent reports the full workspace run exited 101 because all 21 tests in the encoded_verification target failed during shared fixture setup. Source inspection confirms the fixture still expected encode protocol 2 and emitted protocol-2 encode progress/completion after production encoding moved to protocol 3.

Reviewed the correction in `crates/deadpan-cli/tests/encoded_verification/fixture.py:48`, `:68`, and `:80`: owned-wait encode progress now uses protocol 3; ordinary encode setup requires protocol 3 plus an explicit null binding; encode completion returns protocol 3 and binding null. Verifier progress and completion retain protocol 1, and cancellation echoes the actual request protocol. The fixture remains explicitly synthetic and gains no media-verification authority.

No remaining finding in this small diff. This is a test-fixture compatibility correction with no production/helper change. The parent had started the focused 21-test rerun at review time; this reviewer executed no checks and does not record that rerun as passed yet.
