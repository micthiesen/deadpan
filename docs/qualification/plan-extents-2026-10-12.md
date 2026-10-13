# Effective plan extents, 2026-10-12

Compiled nodes now retain an exact output extent separately from the integral
duration in their authored recipe graph. Picture bounds, continuity, normalized
framing, Hold context limits, audio allocation, definition signals, owner windows,
retained-binding support and independent voice admission use the exact extent.
Source mappings, Hold audio recipes and intrinsic Preserve counts keep their
separate processing meaning. All three ordinary/context compilers embed the
current integral document durations without rounding.

`node_exact_duration` and `AudioDefinition::exact_duration` expose the allocated
extent. The existing `node_duration` returns no count when that extent is
fractional or exceeds its integral representation. `NodeInspection::duration`
and `AudioDefinition::duration` still describe the integral recipe graph, as
their API comments now state.

`PictureFraming::effective_duration` carries the extent used for committed path
evaluation. Camera's temporary path preview reads that same sampled extent,
instead of independently supplying an integer count. Its ordinary authored
count remains available separately. No key, control or saved document changes.

Ordinary nonunity Preserve preparation explicitly refuses an allocation whose
extent differs from its intrinsic count. An owned processing route must supply
that relationship before derived Preserve insertion is admitted. Enlarging or
rounding its input recipe would change the signal and is not a substitute.

## Verification

Base `6cb83e04`; locked dependencies, Rust 1.97.1. The primary implemented and
self-reviewed alone. No live GUI or physical audio output was used.

The compiler-boundary fixture keeps a two-frame root and two complete two-frame
Source children. It installs coherent exact FollowSpeed mappings and Sequence
prefixes over unchanged integral recipes:

| Retime | Recipe count | Output extent | Child frames per output frame | Root sample allocation at 30000/1001 fps |
| --- | --- | --- | --- | --- |
| First | 1 | 1/2 | 4 | 0..801 |
| Last | 1 | 3/2 | 4/3 | 801..3203 |

Independent expected coordinates verify that each allocation's midpoint selects
its source midpoint and the midpoint of its normalized camera path. The last
definition owns 2,403 PointCeil samples; its placed root allocation owns 2,402
RoundEven samples. Cold queries use 1-, 31- and 1,601-sample chunks. A separate
case checks an independent 1,800-sample sound that fits the exact owner but
exceeds its integral recipe count, plus overflow and FitBeat refusal. Another
case verifies that changing this allocation to Preserve requires its route.
The Camera test evaluates the temporary path against a sampled 3/2-frame owner.

The fixture changes a private compiled plan coherently; it is not evidence of
persisted effective clocks, an authored fractional project, decoded PCM or an
end-to-end Retime insertion. Existing media/PCM suites provide regression
coverage for currently admitted integral documents.

Initial setup found a misplaced test module, missing definition-index fixture
arguments and an incorrectly typed qualification identity. These fixture errors
are retained in the verification record; they are not media failures.
The initial native test filter selected the empty library and ran no tests;
the corrected binary target runs all ten Camera tests. Those and the 22
presentation tests pass, along with all six focused plan tests.

All 1,153 plan, audio and playback tests pass with none failed or ignored,
including the existing real-media, canonical PCM, retained Preserve, owned
sound, storage-reopen and cold-query regressions. Production sources are
unchanged across the final focused, native and broad runs.
Strict all-target Clippy for plan, audio, playback, CLI and app passes, as does
workspace formatting. The
[evidence](../../tools/media-qualification/evidence/2026-10-12-plan-extents/metadata.json)
retains complete logs, source hashes, final test-executable hashes, toolchain and
machine identity, setup failures and the self-review scope.

## Remaining integration

Document validation and persistence, retained timing-layout extents, effective
Retime maps, mark transforms, owned Preserve input/output routes and their
atomic lifecycle remain open.
Integer timeline/Zoom editing and generation admission also need their declared
clock contracts integrated before fractional documents can be exposed. The
scoped Hold conditioning query still needs qualification of adjacent samples
when fractional edges lie within half a definition frame of an endpoint. The
existing Retime insertion guard remains. No DP requirement or gate is newly
complete, and this increment does not establish a performance target or a new
real-media path.
