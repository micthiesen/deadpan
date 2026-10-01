# SourceEditWindow draft review

Scope reviewed read-only: `core.patch`, `media.patch`, `inert-tests.patch`, `store-tests.patch`, and `fixtures.patch` under `/tmp/deadpan-source-window-20261001`, against the current shared checkout and downstream source/audio planning APIs. No Cargo/native execution or checkout edits were performed.

## Findings

1. **Minor, documentation clarity:** `crates/deadpan-core/src/source_edit_window.rs:211` and the `SourceNode.edit_window` field documentation describe a physical owner's local clock without stating that this is the clock **after** the independent `audio_offset`. The contract page `SOURCE_EDIT_WINDOWS.md` does state this. Audio selection mappings are stored before the offset, so the distinction matters when Trim intersects support and converts back. Add the qualifier to the Rustdoc to keep the core API self-contained. The draft's constructors all use offset zero, so this is not a current arithmetic bug.

No correctness findings found in the reviewed drafts.

## Review notes

- `SourceEditWindow::new` preserves rational endpoints, enforces positive half-open width and signed-frame bounds through `validate_placement`; `validate` adds the owning Source-duration bound. The prefix helper uses checked exact addition and cannot mutate the original on failure.
- Generic video mapping changes, audio mapping changes, and audio offset changes clear the field; value-identical assignments retain it. The command path is transactional, and the proposed tests exercise inverse restoration and occurrence isolation.
- The five historical Source wrappers upgrade with `None` and refuse to project `Some`. New all-version tests cover document, subtree/occurrence insertion, import/splice where supported, escaped/null fields, and both patch directions. The mechanical initializer patches cover every `SourceNode {` source/test file found under `crates` and `native`.
- Import, selected Original moment, and temporary AudioRange constructors compute exact selected extents before integer-frame ceiling. Independent fraction expectations match source ticks, sample counts, rates, and stream origins in the patch. The moment and AudioRange tests retain complete media context and verify the field is inert to picture/audio plans, including rounded tail silence and dormant audio.
- The prepared-source store test checks both current and prior snapshots after reopen, so it covers persisted selection metadata and retained history.
- A future Trim resolver must honor the documented post-offset window clock and subtract the audio offset when writing pre-offset audio selections. Also gate Slip/Trim when the selected interval extends beyond measured picture support unless held-picture policy is explicit; full A/V imports can have audio-only lead/tail. These are resolver requirements, not defects in this metadata-only patch.
