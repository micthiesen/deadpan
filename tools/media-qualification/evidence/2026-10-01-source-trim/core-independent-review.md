# Core Ripple Trim and Slip admission review

Read-only review of the current core Trim implementation and the shared Slip admission refactor. Compared against `docs/SOURCE_TRIM.md`, the prior design reviews, and the current resolver, reducer, command, marks, root-sound, and test paths. No checkout edits, Cargo, or native execution.

## Findings

No actionable correctness findings.

## Scope checked

- `crates/deadpan-core/src/source_edit.rs`: common Source/neutral Partition admission, complete qualified spans, linked affine audio/video clock, exact selected-window agreement, independent audio offset, and Sequence ancestry limits.
- `crates/deadpan-core/src/source_trim.rs` and `source_trim/apply.rs`: signed edge limits, strict positive-selection and one-output-frame rules, fractional window/padding behavior, prefix growth, wrapper identity, old-tree target/suffix capture, one aggregate reanchor budget, framing/gain/binding translations, and final mark transform.
- `source_slip.rs`, `command.rs`, and `sound_routing.rs`: shared admission use, atomic Trim dispatch, zero/no-op handling, root sound operation derived once from the same resolution, and outer detach/restore ordering.
- `marks.rs` plus `source_trim_resolution.rs` and `source_trim_command.rs`: stored Source/physical-local versus visible occurrence behavior, prefix translation, loss policies, exact inverse, metadata rejection, and the tested arithmetic/clock cases.

The exact clamp equations, all four edge/sign root-ripple operations, target and suffix windows in the old project clock, and prefix coordinate translations agree with the documented contract. The reducer takes one old-tree audio capture before mutation, applies target and suffix reanchors from that capture, then translates only the physical target binding/effects for a new prefix. Existing Partition identity and physical Source identity are retained as specified. Root's focused test results are tracked separately; this review ran no checks.
