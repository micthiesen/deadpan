# Forest replay failure

The failed combined assertion included `splice_picture_work(None).is_none()`. In `preview/splice.rs`, an empty structural source at a nonempty destination deliberately resolves to `Work::Project` with the saved base workspace and `ProjectView::Sequence { frame: ProjectFrame(0) }`. This is destination picture work, not copied-source or proposed media work. The report's settled frame 3632, before `:splice`, displayed source frame 0 with label `Showing sequence frame 1` and `loading=false`; the failure frame 3643 displayed the same frame and label. A wait before capture is unsupported by this evidence.

`diagnostics.patch` replaces the incorrect no-work predicate with an exact `Work::Project` identity and frame check. It retains every other assertion, reports failed predicates by name and records the actual roots, slot, destination and before/after picture. It also avoids indexing the second imported root when missing.

Source SHA-256: `65bdbb43ac0ed87758101d9695c83a612cfaf13e2bfe03f8184d464d3cee17d3`
Scratch SHA-256: `a2b546fd35c520b3eb073792f3771e17416af78f5ca888de03b38e2610a9014e`

`rustfmt` and `git apply --check` passed. Cargo, the app and replay were not run, per task bounds. The checkout was not changed.
