# Review: recovery, concurrency, and revocable byte ownership

Scope reviewed: current dirty and untracked changes under `crates/deadpan-store/src/render_media.rs`, `crates/deadpan-store/src/render_jobs.rs`, the render namespace changes in `object_storage.rs`, and the encoded render host/jobs/verifier paths. `git diff origin/main...HEAD` was empty; the implementation under review is in the shared dirty/untracked tree.

No surviving findings.

The active-child revocation gap found during review was fixed in the current tree. `encode_guarded` polls the owning render handle while supervising the encoder and requests cancellation when the handle closes. The verifier loop now polls `candidate.check_live` and cancels its child on revocation. I found no remaining concrete defect in this lens.

No builds, tests, formatters, or native programs were run in this review.
