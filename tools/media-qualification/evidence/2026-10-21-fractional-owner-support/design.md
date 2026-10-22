# Fractional retained owner support

Scratch proposal only. No Cargo or runtime claim.

Production: crates/deadpan-plan/src/audio_owners.rs, one overlap predicate:
replace floor(reference_at) + 1 == allocation.start with
floor(reference_at) < allocation.start, retaining:
- positive allocated interval;
- explicit Retained provenance;
- exact transferred half-open interval end strictly beyond allocation.start.

The sampled coordinate and rate are untouched. Only the structural probe moves
to the first valid reference point, exactly as the existing overlap contract
already specifies. The private tests simulate the receiving side of a checked
retained transfer; the public fixture obtains real clocks through
capture_unbound_audio_bindings and SetHoldDuration, then checks the actual NTSC
seam and exhausted processor history.

At the real seam, root cut36/7 frames maps to sample41184/5 = 8236.8.
Retained root sample6635 is the transport of consumer8236, and its structural
probe6635.5 lies after the historical cut33176/5 = 6635.2. Therefore the following
Hold owns it. Its lookup point maps to input13727/4576, yielding Hold local
-1/4576. The reference point interval [64057/20, 64057/20+7/4) crosses the
first PointCeil Hold point3204. There is no preceding Source ownership at8236,
no clamped gain coordinate, no unity fallback, and no descendant clock invented
from a different owner. Empty/exhausted/wholly-before states remain inactive or
strict errors; current-origin inconsistencies remain errors.

The public pure-plan fixture uses a picture-only Source plus captured silent
Holds, so it proves clock ownership rather than PCM audibility. The separate
real-media PCM witness must prove nonzero processed output and different
Source-before/following-Hold treatments at the seam.

Proposed installation:
- new crates/deadpan-plan/src/audio_owners/fractional_support_tests.rs
- registration in audio_owners/tests.rs from registration.patch
- production.patch only after root authorizes after pinned PCM failure.

No change to AudioOwnerSupport or its consumers. No renderer source-admission
or raw-domain relaxation. Existing strict nonzero-Inactive checks stay intact.
