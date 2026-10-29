# Owner capture baseline V1

Frozen before capture admission at query checkpoint cb6a81d6ef2d233f07f72922103673548ddf31c8.

snapshot.json is the actual isolated legacy producer output. Main and isolated
producers emitted identical logical fixture, supplemental and event arrays.
The metadata records the actual variant and compiled-source authority; do not
compare those two fields as semantic output.

Five original families retain all 14 unchanged query/capture cases. Two validated
supplements cover an unbound Shift stage and unbound 44.1 kHz Source leaf.
The 128 ordered events include 96 actual direct/candidate captures, 16 probe
walks and 16 probe results. Six ample/exact/one-short direct/candidate captures
per case run through the real legacy walker, including its discarded span.
Thirty-two error events retain an already-populated capture slot.

Observers are test-only and thread-scoped. They borrow the actual old walk
Result before its discard, without another walk or reconstructed Domain.
Serializer allocations make these logical/work/error observations, not memory
peak or native/media qualification. Expected values must never be regenerated
from an admitted or changed capture kernel. The ignored producer requires an
explicit new scratch output path and cannot overwrite this file.

PointCeil raw-domain output includes its public reference map, but not its
private AudioSignal provider. This is sufficient for the capture-only boundary;
extend that oracle before changing retained-probe/PointCeil construction.
ProjectedStage serialization is present but not exercised by these fixtures.
See docs/qualification/owner-capture-baseline-2026-10-29.md for qualification.
