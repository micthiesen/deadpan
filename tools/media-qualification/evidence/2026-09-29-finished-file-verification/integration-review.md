# Encoded verification integration review

Source review of `crates/deadpan-cli/tests/encoded_verification.rs`, its Python transport fixture, retained fixture metadata, and README. The production host/protocol/worker paths were read to assess what the assertions prove. After the initial review, the parent authorized focused assertion fixes in the Rust test and one completion-emission witness in the Python fixture. No tests, builds, native execution, or formatting were performed by this reviewer. The parent is running the tests independently.

## P2 resolved in source: Negative tests admit generic crashes as the intended rejection

`encoded_verification.rs:502` accepts every `EncodedRenderError::Worker` for a corrupted movie. `encoded_verification.rs:546` accepts Worker, Protocol, or Supervisor for every hostile transport mode; only `failed_exit` then checks an exact diagnostic.

The production host maps supervisor Fault and unsuccessful exit into Worker, so these checks cannot distinguish the intended media/protocol rejection from an unrelated worker crash. For example, a branch-local Python exception before emitting the `wrong_movie` completion satisfies that case through the unsuccessful-exit error. A native decoder crash while reading `slice_payload` likewise satisfies the corruption assertion. A later successful retry proves candidate retention, but it does not establish which earlier rejection branch ran.

Require mode-specific diagnostics or stable diagnostic fragments for protocol cases and mutation-specific rejection witnesses for native corruptions. In particular, reject generic unsuccessful exit, missing terminal response, runtime/setup failure, and unrelated pipe errors for modes that are intended to exercise validation. The intentionally failing-exit case should separately prove that its valid completion was emitted before the unsuccessful exit, such as through a fixture witness retained outside the protocol stream. The existing exact `failed_exit` diagnostic is a useful pattern.

Disposition: each hostile mode now requires its specific protocol/worker diagnostic, with no Supervisor acceptance. Each movie mutation requires its expected structural rejection or explicit native decoder failure operation; generic process exits cannot satisfy those checks. The intentional completed-then-failed-exit fixture writes an exclusive JSON witness only after flushing its completion, and the test validates that completion plus the exact identity, captured contract, document/movie hashes and byte count before accepting the exact unsuccessful-exit diagnostic. Parent execution of these strengthened assertions remains pending.

## Positive source observations

- The positive path invokes `CARGO_BIN_EXE_deadpan-cli` through production `verify()`. It exercises candidate staging, the private native verification child, protocol binding, supervised teardown, and retained candidate bytes. Assertions include exact frame/sample counts, fresh-GOP frame count, movie/document hashes, priming origins, manual versus ordinary physical counts, and actual B-picture/GOP observations on the software fixture.
- The Python encoder path only replays retained bytes. It asserts equality of the complete requested output contract and expected native moov budget before rebinding the test document hash and actual movie identity. `Fixture::new` validates the captured manifest and independently checks file SHA-256 and length before use. The deliberate document rebind is explicitly documented in code and README, and the test asserts it differs from the original provenance hash.
- The eight mutations change distinct real MP4 fields or packet payloads: visible sample width, color primaries, audio edit offset, movie duration, sync table, NAL length, IDR NAL type, and IDR slice payload. The mutations preserve the file length and candidate admission recomputes the movie hash. NAL mutations use an actual inspected video packet extent; IDR payload/type changes assert that an IDR was found. These are meaningful mutations once their failure assertions establish the intended rejection.
- Preflight failures use a missing worker executable and assert cancellation, deadline, and protocol errors before runtime launch; only the explicit missing-runtime case accepts Supervisor. Each result returns the same bytes, followed by a real native retry.
- The README accurately limits the fixtures to decoder/host integration. It does not claim that replayed media came from the synthetic background document, qualify encoded content against that document, authorize destination publication, or replace broader encoder/content/sanitizer evidence.

## Cancellation claim and limit

The cancellation test requires a real native progress callback, sets the shared cancellation flag, asserts the exact Cancelled error variant, checks returned bytes, clears cancellation, and successfully retries the same candidate. That is a sound check that cancellation observed after reported native work prevents admission and preserves retry ownership.

It does not establish active decoder interruption or receipt of a worker cancellation acknowledgement. The host coalesces progress, and a callback can run in the same poll that receives clean completion; the final host cancellation check still produces Cancelled. The current test name and README make the narrower cancellation/retry claim, so this is a coverage limit rather than an additional defect. Any evidence writeup should preserve that distinction.

## Retained fixture provenance

The three original manifests and `fixtures/provenance.json` identify the source qualification report, its SHA-256, original paths, original document hashes, movie sizes, and movie SHA-256 values. The README lists matching movie identities and links the producer qualification. Tests bind the checked-in movies to their captured manifests. They do not automatically authenticate the separate provenance JSON or recreate the original producer run; those remain retained provenance claims alongside the independent native decode checks.
