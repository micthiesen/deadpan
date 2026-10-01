# Independent root-sound Trim review

Read-only review of `root-sound-trim.patch` (SHA256 `488fb4cfbae60028ede6b11370f0e4c9444e5afcb6a1977ef5ac447d67b50d45`), `design.md`, `README.md`, the root-sound audio review, and staged core/plan fixtures. No source edits, Cargo, native execution, or commits.

## Result

No actionable correctness finding in this bounded patch. The shared projection implements the accepted three-Keep/two-Gap old-to-final map, normalizes coordinate-contiguous Keeps before sample anchors/cut flags, and uses i128 until clipping and checked narrowing. I independently checked the six literal maps in the design, scalar Insert/Delete special cases, signed extreme cancellation, output containment, and the mapping from `start_cut`/`end_cut` to existing `after`/`before` policies.

The core retention paths apply the same projection to actual rounded sample support and to exact logical support only when the original selection allocated no samples. They bound interval growth to `1 + 2 * MAX_ROOT_SOUND_EDITS`; the sampled-support path intersects and clips in i128 before narrowing. Capture appends one operation on the retained recipe grid, preserves prior route entries, checks the history limit before append, removes only events with exhausted support, and bypasses route creation for a globally identity map. Frozen schemas 30–32 now explicitly admit only Insert/Delete; the legacy tests cover Trim, identity Trim, escaped type keys, and patch projection refusal.

Plan envelope compilation consumes the same Keep ranges and cut flags; its existing chronological island and exact-boundary logic remains in place. Literal NTSC phase/gate assertions, independent sample-rate phase values, an equal-In/Out terminal-sample witness, prior-route chunk queries, mixed-gap policies, and scalar comparisons provide meaningful coverage. The staged plan fixture uses a still-image Source with a matching still-image asset; the core fixture uses a Background Hold and an audio-qualified sound asset. I found no forbidden Blank/no-audio Source fixture.

The staged source advertises 17 new tests, matching the six capture tests, five core integration tests, and six plan tests. Compilation and test execution were not performed by this reviewer; the staged manifest records Cargo/native as not run. Root-owned checks remain necessary before integration.
