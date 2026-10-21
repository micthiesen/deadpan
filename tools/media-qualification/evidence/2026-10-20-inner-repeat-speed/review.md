# Independent source review

Base: 92907cb0. Reviewers did not run Cargo or edit production.

`speed_review` reviewed the one-level family, indexed scale projection, historical predicate cuts, actual lattice units, current owned/default branch admission, dormant gaps and shared bounded work. It found no concrete production issue. It confirmed the pre-edit snapshot is captured once and reused, historical timing layouts stay authoritative, current branches are checked by indexed ancestry, and commit captures all missing bindings while `affected` controls reanchors.

The initial ancestry proposal walked the current subtree recursively for every owner. Root identified the repeated scan; the applied indexed patch instead uses scoped_repeats, scale_in_scope and node_extent. The reviewer checked the final patch and its 1024-unrelated-owner regression, without further findings.

The initial scale-scope proposal skipped the scope root's Retime factor. Root compared project_scoped semantics and corrected the proposal before applying production. The final helper includes that factor and rejects opaque processing. A proposal fixture also selected play0, which had an owned gap; it was corrected to play2 for the default-gap assertion before application.

The first runtime new-carrier assertion expected no reanchors. Existing composite code deliberately retains AllocationEntry steps under Repeats. Review confirmed this is intended. The corrected test compares actual factory lattices, timing table, one pre-edit entry step, literal per-occurrence origins and sample boundaries, current moved geometry, inverse and JSON roundtrip. No production fix was needed.

Audio/export review is recorded separately once returned.

`speed_audio_review` found no findings in the repeated rate formula, default-gap transfer admission, per-carrier PCM oracle, seam/partition/second-update coverage, independent 22-picture literal oracle and Source-only PCM peak window. Review preceded execution of the CLI test and made no runtime claim.

`speed_review` also independently reviewed the proxy disk-full fixture repair. Pre-staging isolates the worker write from host metadata allocation; strict Worker, disk-full/environmental classification and cleanup checks remain. The one-use factory supplies fresh staging on a later factory call, and the production retry loop drops failed staging first. Actual disk-full worker errors are non-retryable, so this fixture does not itself exercise a retry. No finding.
