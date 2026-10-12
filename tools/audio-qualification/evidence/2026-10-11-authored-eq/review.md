# Solo review

The primary agent implemented and reviewed this change alone under the owner's
instruction. Review covered the complete tracked and new-file diff, persisted
recipes, semantic transactions, owner/context capture, recursive treatment order,
resource admission, dependencies, silence and failure cleanup.

Three findings changed the implementation or its qualification:

1. A whole Repeat's changing play was incorrectly part of EQ cache reuse. A
   70-play regression failed with `Limit("EQ owner count")`. Reuse now compares
   checked owner provenance and affine clock within the same parent context.
   Per-play escalation is still read from each owner query; distinct child
   occurrences keep their own identities.
2. The first delay-tail fixture ended before the 300 ms delay could ring. It
   now has a nonzero expected tail and compares against processed source input.
   An additional shared parent EQ proves that the parent applies once.
3. Frozen audio contexts needed a version boundary for the new treatment.
   Schema 10 authenticates EQ, schema 9 rejects it, and unmodified older contexts
   retain their existing identity. The stage-capacity and current-version tests
   were updated along with unknown-version and duplicate-field rejection.

Strict Clippy also identified a 1,696-byte context enum. Immutable domain and
retained handles now use shared ownership, keeping graph entries and recursive
copies small without changing their checked provenance.

The graph chooses the outermost EQ, recursively evaluates its input prefix,
then applies the suffix. Parent-context identity prevents reuse based solely
on a matching nominal clock. Hidden domain support and retained consumer maps
remain distinct. All halos use the parent controller and carry dependencies;
each reservation releases on success or failure. Explicit silence/exhaustion
gates filtered output. Authored mute remains at its specified stage.

The release fixture checks independent tone amplitudes before rendering, then
compares the emitted file with the committed revision through the shared bus.
The retained source and binary identities describe the final-source run; earlier
diagnostic failures are retained separately and are not counted as successful
qualification.

Independent sound events currently reject with a specific error before source
admission. Full shared sound/group processing and its per-voice policies remain
required implementation, along with native authoring, broader nested binding
coverage, persistent filtered tiles and latency qualification. This increment
makes no native GUI, device, listening or interactive-performance claim.
