# Repeat Follow creep native replay handoff

The complete five-path native source set is frozen. New scenario: repeat-follow-creep. Base supplied by Root: cb89b8b79a01a4ad59fc29dc572ed65015a3c3af. Source copies and SHA-256 pins are in native-source/ and native-harness-freeze.json.

Construction uses production keys and project writes only. Require 120 frames at 30000/1001 and a 16:9 canvas/source. Save point target-1 at Original0 with center (.58,.5), manually correct it at indexed Original12 PTS to (.68,.5), retain Original[0,24) by deleting [24,120), then run repeat 3 gap=4f zoom-step=0.125. Select the actual whole Repeat, choose f1,t in Camera and separately commit fixed Follow. Record x while applying implicit Keep creep from=1.1 to=2.2 curve=linear. No authored fixture mutation or backend seam.

Final media contract: 80 output frames and 128128 canonical 48k audio samples. Plays [0,24),[28,52),[56,80); default silent Freeze gaps [24,28),[52,56), holding measured Original23 PTS with captured geometry. No final gap. Local Original ordinal is j%28 for a play, 23 for a gap. Gap ownership is the preceding stable iteration; ordinary play pictures carry that Repeat iteration in their provider instance. Explicit owned gap branches are not authored or qualified by this native fixture.

Independent Q32 oracles: Q=2^32 and R(n/d) is integer nearest-half-even. B=R(11Q/10), E=R(11Q/5), Pj=R((2j+1)Q/160), Sj=(B+R((E-B)Pj/Q))/Q. Centered escalation is 1,9/8,5/4, kept as a distinct inner layer. For source percent p=58 before Original12 or68 afterward and zero-based play k, C=R((400+(p-50)(8+k))Q/800)/Q. Exact unquantized centers by play: (.58,.68),(.59,.7025),(.60,.725); gap centers .68 and .7025. This is a visible-target case; Core/Plan independently cover clipped fallback and non-resurrection.

All80 plan frames assert owner frame-center clock, independent scale and projected center, source provider/Original point/ordinal, stable iteration/gap ownership, captured gap context, and unchanged inner layers including escalation. Layout asserts stable starts0,28,56, duration24 each, gaps4,4,0, no owned branches and no picture at80. All other nodes, complete Repeat fields except framing, target rows, sparse branches, canonical audio spans, duration and both cursors compare before/after.

Stopped presentation checks at Edit0,11,12,23,24,27,28,39,40,51,52,55,56,67,68,79 assert complete framing and exact source identity/point. Captures are requested at24,28,56 and final56. Root runs and inspects actual images.

History and Camera: one Undo restores fixed Follow; Redo restores full animation; no-change Enter and adjusted Escape preserve without history; explicit t release explains animation removal and saves exactly the installed static pose; Undo restores animation. Named x and fresh Apply/Undo/dot replay semantic Keep. Visual ranges4 and80 refuse without writes. A temporary enclosing Repeat supplies the same fixed-Follow inner Repeat under scope all and scope play2; exact RepeatEditBranch authority is asserted before each refusal. Leave and Undo remove that wrapper before the positive animation. Final node/target/sparse-branch maps and audio spans equal the live animated fixture.

Prior Sequence scenario: removed obsolete ordinary Repeat wrapper refusal; retained its Retime-only wrapper refusal and restoration. Search found no other native follow-creep ordinary-Repeat refusals needing edits.

Verification performed here: scoped rustfmt parsing/formatting exit0, static review and exact registration/source-copy checks. No Cargo, runtime or Git. Parent owns compile, full native/media/PCM and portable-history verification. Runtime assumptions remain assertions: saved target f1; Group-independent Repeat owner selection; default gap capture; nested scope selection and branch; history selection restoration. There are no claimed native passes yet.

Boundary oracle is retained in native-harness-freeze.json. Pins:
- crates/deadpan-app/src/preview/harness/repeat_follow_creep.rs: 5fb0e16387351c0a5d951e56a8187811b3e51d5c30f3eafaf75f6648ac2339ce
- crates/deadpan-app/src/preview/harness/sequence_follow_creep.rs: fd34ccdbb0d26a2ff69319a45c428dbbcf1acea1ead504368bd84ccd966b1874
- crates/deadpan-app/src/preview/harness.rs: 810383d8493a36a61db423ca87850758438a5cb66c6afeca8a54d6310a451cb0
- crates/deadpan-app/src/preview/harness/scenarios.rs: 683a6173fd3e2b2c80b358632c5bb07f51aca87401b5159f2e9f778cce6d8614
- crates/deadpan-app/src/ui_harness.rs: a7d27c25cc52b6af7c6133934de45c2ac7dc331e4a4a531813d08cf82b6a52aa
