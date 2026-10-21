# Fixed Retime export fixture construction audit

2026-10-20. Original failed `export.log`, retained package/request files and root-retained binary/source hashes are unchanged. No Cargo execution or production edits by this agent.

Failure occurred in request r07, which is `Command::Insert`, before any SetHoldDuration or render. Shorten/split preparation leaves Partition Retime roots around qualified raw Source descendants. The fixture copied only root recipes for lead/pre/speech/outside, then deleted the complete old tree. Copied Partition child fields still referenced deleted IDs n5/n7/n39. Insert validation correctly rejected missing n5. It was not an identity allocator collision or a duration-command failure.

Correction only in `crates/deadpan-cli/tests/preview_export/fixed_retime_duration.rs`: clone each complete owned fragment with fresh IDs, remapping all internal child references. Clone the speech fragment separately for the outside suffix so the two views have independent qualified raw Source owners. Preserve every original Source field, Partition window, framing, Freeze recipe and retained asset/qualification; copy no sampling bindings. Before deleting preparation, explicitly assert every copied Sequence/Repeat/Retime child exists in the fresh inventory.

The fixture still requires exactly four actual raw Source owners and initially empty sampling bindings. It retains the same root duration27, fixed Edit/FollowSpeed window[0,8), output8, Hold3->5/1, every-picture literal expectations, old interior origin20/new22 or18, outside origin24, +1554 sample click windows with full nonzero256-sample comparisons, quiet-window checks and strict emitted-file verifier/provenance/zero-offset assertions. No expectation, tolerance or production behavior changed.

Rustfmt and diff checks passed. Runtime qualification remains owed to root.
