# Combined Trim prerequisite corrections

Independent endpoint review found one existing one-line `AudioReanchorStep`
literal inside the `closed_gap_origin!` macro that the staged patch missed.
Root added `anchor: Default::default()` before the first compile. The frozen
staged patch and review remain unchanged; this is a static integration finding.

`endpoint-core-first` failed to compile its lib tests: the new legacy tests
called an ambiguous `validate` imported by two glob imports. Root qualified both
calls as `super::validate`. Production code is unchanged. No runtime tests ran;
the original command, diagnostics and stable source manifest remain recorded.
The child completed and all Cargo jobs drained before this correction.

`endpoint-core-second` passed 205 tests and failed 11 new endpoint/legacy tests.
All eleven failed during construction of the same invalid fixture shape: a
Source had neither picture nor audio. Root added a declared picture-only asset
and stream to the two new fixture builders, preserving intentionally absent
audio and every timing expectation. The binding production code is unchanged.
The old run is retained; planner and decoded-PCM checks had not started.

`endpoint-core-third` passed 215 and failed one new copied-slice expectation:
the final collapsed resume boundary is local 0 rather than the expected old
endpoint 4. Independent review confirmed slice capture appends an AllocationEntry after
the historical endpoint; boundary 0 is correct. The test now changes the current
lead from 1 to 2 frames, asserts the retained End/AllocationEntry order and exact
−5/8008-frame (one-sample) phase, and checks that removing the endpoint instead
yields zero. Production code is unchanged. The same absent-media fixture problem
was found statically in the pending audio absent-Source case and corrected to
a declared picture-only stream before that audio test's first execution.

Before the root-map PCM test first ran, root corrected its invalid Blank/no-audio
Source fixture to a declared picture-only Stream. The timing and absence of
audio remain unchanged. The frozen staged patch and review are retained.

`combined-core-first` failed to compile its new integration submodule: the staged
core and plan test roots used bare `mod trim` without their explicit relative
paths. Root added the two path attributes. No runtime tests ran; production
code is unchanged, the failed command is retained and all Cargo jobs drained.

`combined-core-second` passed 255 tests and failed the copied-slice negative
control admission. Both pasted bindings passed the strengthened exact phase
assertions, but the control kept all timing records while keeping only one
binding, leaving the other paste's records unreferenced. Root now retains the
complete binding map and removes the endpoint only from the controlled owner.
No production code changed. All 11 new root-map core checks passed in this run.

`combined-plan-first` passed all 52 checks. `combined-pcm-first` passed 21
Source-audio checks and failed the earlier-endpoint oracle. One diagnostic
reproduction is retained with its temporary print block. The test oracle used
source support 0..8197, but local owner start 0 maps to source 1000−offset7=993,
so actual filtering is limited to 993..8197. The retained phase 4968/5 is
unchanged. The correction uses that literal support, rejects both wrong phase
and wrong full-file support, and checks equality with whole-file reconstruction
200.6 samples beyond the edge. Both chunkings and inverse equality remain.
The temporary diagnostics were removed. Also removed a redundant Copy clone
from the new root-sound picture fixture before lint. Production is unchanged.
