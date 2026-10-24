# Sequence Follow creep native replay handoff

Owned four paths are frozen. Source base supplied by Root: fea6b081. Only the new native harness file and the exact module/scenario registration lines were edited.

Scenario: sequence-follow-creep. Requires the counter fixture of 120 frames at 30000/1001; the replay asserts both values. No backend seam and no authored fixture mutation. Every target, framing and structural edit uses production Camera keys, command routing and project writes.

Construction: create point target-1 at Original0 with center (.58,.5); manually correct it at indexed Original80 PTS to (.68,.5), using f1,c,10l,Enter and discarding temporary framing. Delete half-open Original [40,80). Group both remaining 40-frame fragments into an ordinary selected Sequence at root. Retime-fragment children are accepted. Select target-1 in group Camera, press t, and save a fixed Follow separately. Record x while applying creep from=1.1 to=2.2 curve=linear with implicit Keep.

Final media contract: 80 output frames, 128128 canonical audio samples at 48kHz, Original mapping Edit j -> Original j for j<40, j+40 otherwise. Edit39/40 jumps from Original39 to80; target center changes from Q32(.58),.5 to Q32(.68),.5. No Hold is involved. OwnerOutput is the complete Sequence80 clock, both live/fallback bases 1.1, linear factor2.

Independent scale oracle: Q=2^32; half-even integer round; B=round(1.1Q), E=round(2.2Q), Pj=round((2j+1)Q/160); Sj=(B+round((E-B)Pj/Q))/Q. Boundary exact fractions: {"0": "4753991927/4294967296", "39": "3528584069/2147483648", "40": "7116223939/4294967296", "79": "4709700075/2147483648"}. All80 planned frames are checked independently. Each verifies provider and exact follow point against the original plan, measured Original ordinal, unchanged child scopes/clocks/poses and captured context, and continuous group owner position (j+.5). Stopped displayed scope lists and source-frame identities are checked at Edit0,39,40,79; seam captures retained through Driver capture.

Preservation: compare all other nodes, expected complete group node except its framing, all target rows, full canonical audio spans, duration, and both cursors. Audio spans compare before/after at identical zero-origin range. Final history restoration compares node and target maps and audio spans again.

History: one Undo restores the fixed Follow, Redo restores full animation. No-change Camera Enter and adjusted Escape preserve it without history. Explicit t release explains animation removal and commits the exact installed pose; one Undo restores animation. Named x invocation and fresh direct/Undo/dot re-resolve SemanticZoomCenter::Keep. Temporary wrap-repeat2 and wrap-retime2 pitch=tape each receive fixed Follow and refuse Keep creep without a write, then two Undos restore live group. Visual subrange refusal is checked before animation. Final state selects the ordinary live group outside its scope at Edit40.

Verification: scoped rustfmt exit0, source review and exact registration count checks only. No Cargo, product runtime or Git ran. Root owns compilation, real native replay, exported media/PCM and evidence. Potential runtime assumptions deliberately remain assertions: Group selects its new owner; f1 selects the sole saved target; wrapper Undo restores the group selection.

Pins:
- crates/deadpan-app/src/preview/harness/sequence_follow_creep.rs: cd7114bfe6aa323f01fcb2bb8a34442403dd0d7edee3e8882230e4e2bdca467a
- crates/deadpan-app/src/preview/harness.rs: 37b385c23f2eb666eeccb802bda266f7dc88b9ddfbb288a26e9a2fd15ac440cf
- crates/deadpan-app/src/preview/harness/scenarios.rs: 52c73107248b6d4c92047afb663fc35a069c80326da90299e93962ef571f3e3a
- crates/deadpan-app/src/ui_harness.rs: c82e041885aa5792e6946c2c1c9ea39d0eccf169694f375b1b0db990ce0d587a
