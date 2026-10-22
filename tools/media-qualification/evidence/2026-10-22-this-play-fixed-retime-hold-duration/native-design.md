# Native This-play fixed Edit Retime replay

Owned paths:
- crates/deadpan-app/src/preview/harness/scoped_plays/this_play_fixed_duration.rs (new)
- crates/deadpan-app/src/preview/harness/scoped_plays.rs (module registration)
- crates/deadpan-app/src/preview/harness/scenarios.rs (scenario dispatch)
- crates/deadpan-app/src/ui_harness.rs (allowlist)

Replay name: this-play-fixed-duration.

Fixture starts from the qualified shared-duration fixture, then replaces its root with Lead(3) / Repeat(3 total) / Tail(3). The Repeat has a single direct fixed Edit Retime child with output 6 and source mapping [0,8). Its ordinary Sequence body is Before(2), real Freeze Hold(2), Suffix(6); every Hold uses the qualified RoomTone recipe. Total output stays 24 frames. The selected Hold is completely inside the Retime window.

The replay navigates to Edit frame 10, selects This play 2/3, enters the Retime and body, then selects the Hold. It covers actual 2f prefill, 2f->4f and 4f->3f updates, no-op, isolation count/node stability, Undo/Redo, explicit reselection before dot, semantic duration recording and Run, delayed durable receipt after navigation, reopen verification, and restoring the original project/cursor. It captures prefill and history at 960x640.

No Cargo, rustfmt, or Git command was run; execution belongs to root.
