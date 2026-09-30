# Destination publication host review

Source-only review of `encoded_render/publication/mod.rs`, the current `VerifiedCandidate` and `EncodedCandidate` ownership APIs, and specification sections 22.1 and 22.7. The unfinished filesystem and provenance implementations were not reviewed. Their public types, method signatures, and comments were inspected only to understand the host boundary. No source edits, builds, tests, formatting, or native execution were performed.

## P2: Preserve cancellation and deadline diagnostic codes during provenance capture

At `publication/mod.rs:196`, every `provenance::capture` error becomes `PublicationDiagnostic { code: "provenance_failed", ... }`. The declared provenance error type has distinct `Cancelled` and `Deadline` variants. A user cancellation or expired deadline observed inside capture therefore loses the stable `cancelled` / `deadline_exceeded` code that the same publication operation returns from its outer control checks.

This is observable deterministically when the `CapturingProvenance` progress callback sets the cancellation flag: the outer check has already passed, capture receives the cancelled token, and the host reports a provenance failure. The candidate is preserved, but callers using structured codes cannot distinguish a normal cancellation from invalid/missing historical evidence.

Map the typed provenance cancellation/deadline variants to the corresponding publication codes. Other provenance errors can retain `provenance_failed`. Also preserve interruption categories when a controlled destination reader/writer returns an I/O error: the current host adapters flatten these into `report_write_failed`, `destination_readback_failed`, or `candidate_copy_failed`. The filesystem body remains outside this review, so its precise I/O representation needs to guide that mapping.

Disposition: reported to the parent. A focused test can cancel at `CapturingProvenance`, then assert the cancellation code, the unchanged project, retained candidate identity, and absence of a committed movie.

## Ownership and commit observations

- `publish` consumes one verified candidate but calls the internal operation by mutable borrow. Every pre-movie-rename error returns the original `VerifiedCandidate` plus recovery paths. Failure during report publication may retain a published report while leaving the movie uncommitted; its report scope explicitly describes preparation, so that sidecar does not falsely claim a completed movie publication.
- The final movie rename is the only movie commit point. A filesystem error marked published becomes `PublishedDurabilityUnconfirmed`, rather than an uncommitted failure or a cleanup request. Ordinary failures preserve the candidate. Correctness of the published flag and exclusive rename itself is delegated to the filesystem implementation, which was intentionally excluded.
- `VerifiedCandidate` and its contained `EncodedCandidate` have private fields and no public constructor or mutable report/manifest access. `copy_to` exposes only bounded reads of the private snapshot into a caller-owned sink. The host derives its expected movie identity from the verified report and performs a full destination readback against that identity before committing either final name.
- The provenance API takes the same immutable verified candidate. Its declared report carries the encoded manifest and verification report together with the project/revision/document identity, catalog hashes and generated intervals. Historical lookup and complete-document binding must be established by that implementation; they are not proven merely by its declared type and remain with its owner.
- The local report and receipt distinguish movie identity from report identity. The report includes a unique publication ID and both sibling names; the receipt records the exact report digest/length. The local report describes precommit preparation, so its existence alone does not authorize adopting either name or infer successful publication.
- Copy and hash work uses 64 KiB buffers and captured byte lengths. Destination readback rejects truncation and extra trailing bytes. Report serialization has a 16 MiB output ceiling. This host does not retain a whole-movie cache or touch the project writer.

## Planned changes excluded from findings

The parent has already planned to call `report_partial.confirm_published` immediately before the movie commit and again after a successful movie commit. The second failure must stay a published outcome. Those calls were not yet in the reviewed source; their absence is not a new finding, and their final integration still needs inspection. This report does not claim that unfinished filesystem/provenance behavior or the overall publication path has passed verification.
