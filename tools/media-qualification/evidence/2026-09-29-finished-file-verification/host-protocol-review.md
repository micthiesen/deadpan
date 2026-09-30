# Finished-file verification host and protocol source review

Scope: `crates/deadpan-cli/src/encoded_render/verification/{mod.rs,protocol.rs,host.rs,worker.rs}`, with the existing process supervisor, control pump, encoded manifest, and candidate snapshot read for context. Inspection implementation and private dispatch were unfinished during review. No build, test, formatting, or native execution was run. This is source review, not passing verification evidence.

## Findings

### Resolved P2: Keep hashing inside the captured input extent

`worker.rs:76-85`: `hash()` rereads the current file length and hashes that entire range without the manifest length or `maximum_bytes` limit. The initial extent check at `worker.rs:33-35` does not protect against the staged file growing between that check and `hash()` taking its own metadata snapshot. The staging file remains a writable regular file. A concurrent change can therefore make either hash operation read beyond the admitted byte budget before its digest mismatch is detected. The deadline remains cooperative, but the declared byte bound is no longer enforced for that pass.

The parent now passes the captured manifest length into both calls to `hash()`, rejects a differing extent before reading, and reads exactly the admitted range. Reviewed that change: both hashing passes now retain the captured byte bound. The same-descriptor before/after metadata and digest checks remain. A regression should cover an extent mismatch at hash admission without relying on a timing-sensitive concurrent race.

### Resolved P1: Verification errors discarded the completed encode

The original `verify` signature took `EncodedCandidate` by value but returned only `EncodedRenderError` on failure. Pre-cancellation, invalid limits, staging failure, or verifier failure closed the sole anonymous snapshot, forcing re-encoding before retry.

The parent changed `verify` to return `Result<VerifiedCandidate, VerificationFailure>`, where the failure retains both `error` and `candidate`. The internal inspection function borrows `&mut EncodedCandidate` and returns a report. Reviewed that change: all normal `Result` failures now return ownership of the existing candidate. Retry behavior still needs execution evidence.

## Other source observations

The initial source findings above and the follow-up report-admission finding below are resolved by reviewed source changes. Execution remains with the parent.

- Wire framing uses the existing 256 KiB framed protocol. Tagged messages deny unknown fields. Identity, protocol version, manifest, exact contract, movie hash/length, and packet counts are checked before completion admission.
- `SupervisedProcess` withholds Completed until clean exit, owned-group cleanup, and pipe-reader completion. Verification performs its final report binding only after the supervisor finishes.
- The control pump retains the existing bounded nonblocking reader, exact cancellation identity/token check, queued-control drain, and join before terminal output. A malformed or incomplete control frame prevents completion.
- Staging uses an exclusive new file and descriptor-relative `NOFOLLOW`, `NONBLOCK`, and `CLOEXEC` opens. It checks regular-file type, one hard link, owner, and filesystem. The worker uses the same retained descriptor for hashing and inspection.
- Candidate ownership is separate from the disposable staging workspace. Success constructs `VerifiedCandidate` only after host report validation and binding; no destination publication is added.

These observations do not establish inspector correctness, malformed-file behavior, cancellation timing, runtime interoperability, emitted-file compliance, or clean teardown under injected failures. Those need the unfinished implementation and the parent's verification run.

## Follow-up review

Candidate retry ownership still returns the same candidate on every normal error. Both hash passes still receive the captured manifest extent and reject a different extent before reading exactly that range. No regression was found in either fix. The inspector is now present; it was read only to compare its declared policy against report admission.

### Resolved P2: Reject report observations that contradict the verifier's policy

`verification/mod.rs`, `VerificationReport::validate`: the report boundary accepts observations that the current inspector explicitly rejects. Changing an otherwise valid report's `video_edit_media_time` to a positive value under `BFramePolicy::None`, or to a non-frame-aligned/excessive value under `TargetTwo`, passes the nonnegative-only check. Arbitrary nonzero runtime versions also pass. For a non-AAC-block-aligned authored duration, `ordinary_physical_samples = audio_samples` passes even though the inspector requires the exact rounded physical sample count. `VerificationProtocol::classify` then accepts these reports when their unchanged identity, hash, contract and packet counts match the request.

Apply the same policy invariants to the reported fields: an integral frame-aligned reorder edit bounded by the captured B-frame policy, exactly pinned runtime versions `[4066151, 4064103, 3934311]`, and ordinary physical samples equal to `ceil(audio_samples / 1024) * 1024`. The report can also enforce the inspector's derived minimum GOP count and the requirement to observe a B picture on a sufficiently long `TargetTwo` attempt. Keep these as consistency checks on claims; they do not replace trusted decoding or prove content correctness.

Reviewed the parent's strengthened admission: runtime versions now equal the pinned tuple; ordinary physical samples equal the exact AAC-block ceiling; reorder edits are nonnegative, frame-aligned and bounded by the captured policy; sufficiently long TargetTwo attempts require an observed B picture; and GOP counts meet the minimum derived from the qualified interval. These changes resolve the grouped consistency finding.

Added ten scoped tests in `verification/protocol/tests.rs` plus its test-module declaration. They cover contradictory clocks, physical sample counts and runtime claims; long valid contracts with insufficient GOPs or missing requested reordering; exact identity and contract/hash binding; host byte/packet budgets; unknown fields and stage names; truncation/oversize; one-byte fragmented framing; and cancellation target preservation. Helpers preserve the nonzero-range 1,601-sample phase and separate 3,072 manual from 2,048 ordinary physical samples. No test execution or formatting has been performed by this reviewer.
