# Immutable additive retained-fade owning-lane witness

Captured independently from Main and the isolated published copy before changing
retained fade production algorithms, following published checkpoint
ee9742711868fde9a78bcb945adfd36e67f504d6.

Complete parsed bodies agree after removing only variant, source_sha256 and
published_fade_checkpoint. Compact JSON changes whitespace only. Both exact
external full-body regressions and corrected owning-lane diagnostics pass.

Seven checked setups produce 28 actual provenance trials: 27 Some and one
RejectedSome. Their 1,882 fade calls record 1,167 successes and 715 typed errors.
Actual Some inputs cover retimes, repeats, editorial markers, definitions,
gap identities and DefinitionPointCeil/GapDefinitionPointCeil clock ownership.
These are observations of real checked constructors, not synthetic retained
handles. The separate retained-fade-query-baseline-v1 body is unchanged.

Raw bodies, source/binary/media provenance, independent review, failed initial
setup and corrected reruns remain unit evidence. This logical OLD witness makes
no claim about the new admission implementation or native allocation residency.

Do not regenerate this body from a changed algorithm to make a test pass.
