# Scoped Follow creep Plan handoff after Clippy correction

Owned repository paths only:
- crates/deadpan-plan/tests/picture_plan.rs registration.
- crates/deadpan-plan/tests/picture_plan/scoped_follow_creep.rs new tests.
No production, Git, worktree or whole-tree writes. Scoped rustfmt only these two.
Base supplied by Root:fb38159bae82f2c08e7dfd96f112c24d6571b8e2.
Final source pins:plan-freeze-2.json; exact tested bytes:plan-focused-2-source/**/*.rs.txt.
plan-written-* and pretest-freeze-* are explicitly unexecuted preparation snapshots.
After Core released the first Cargo lane, plan-focused-2 reran the actual2tests after the Clippy correction:
terminal0,2passed,0failed,0ignored,136filtered,2.1148s total (.03s tests).
Own2source paths, all Core/Plan src .rs basis and the observed named manifest
were unchanged during the run. Named manifest:22present,0missing. No failed
focused test or production mismatch. Root Clippy did fail at one collapsible_if,
which this one conditional let-chain fixes without a behavior change. No repository edits after this pass.
Cargo released to Root immediately; no active processes.
The pre-Lost-continuation draft was NEVER executed and remains separately saved.
plan-focused-2.json retains exact argv/env, runner/source/stream/binary hashes.
All scoped source bytes, including failed-run protection, are .rs.txt in
plan-focused-2-source. This observed run does not replace Root's final freeze.

## Actual raw preparation and identity proof

Both tests invoke typed scoped_edit_requirements and prepare_scoped_edit with
serialized CommandRequest::EditScoped/ScopedNodeEdit::SetFraming. The fixed Follow
is retained, adding factor5/3 scale transition (live3/2->5/2,fallback3/4->5/4).
Prepare's transaction equals ordinary apply; exact forward, inverse, document
round-trip and serialized isolation-record replay/validate are asserted.
No semantic authoring eligibility or Camera route is inferred.

Expected clone identities are independently derived from the resulting authored
override tree, not obtained from the isolation map being tested. The selected
outer Play1 overrides body; its Sequence child order identifies lead, inner
Repeat, tail. The inner default child identifies the first-cloned Retime,
provider Sequence and Source. For nested Play1, a second override identifies
the final Retime/provider/Source clones. Exact expected node-pair maps match
isolation records, all prefixes, every allocated fresh ID, and clone count.
All original template nodes remain exact; cloned node values equal explicitly
rewired originals, with only the one selected Framing payload changed.
No physical clone ID is treated as forbidden, and no identity fields are stripped.

Independently mapped InstancePaths are compared with map_instance for every
project picture and with map_target/target.matches_instance for captured
owner presentation. Stable IterationIds remain unchanged; physical Repeat IDs
change only in the selected outer clone's descendant scope. All picture
framing layers, including ancestors and escalation, are compared after that
explicit mapping; only the selected owner's pose is independently recomputed.
Audio spans remap their provider instance, each boundary-origin instance and
Retime stage node under that proven address map, then compare the COMPLETE
AudioSpan including support, grid, allocation, envelope, transform and content.

## One compact tree, two distinct edits

Project duration50:
prefix2 +outer Repeat(two22-frame bodies with2-frame intervening gap)+suffix2.
Each body is lead1 +inner Repeat(three6-frame Retime plays with1-frame gaps)+tail1.
Each Retime maps input[0,4) to6outputframes, source ticks0..4004.
Its provider Sequence applies centered zoom1/2 after the Source crop2x-1.

Case1: select outer Play1 and inner Play1 explicitly; edit whole Retime
global[34,40), complete owner duration6 with OwnerOutput linear transition.
Expected isolation:7body nodes then3selected-Retime-subtree nodes =10fresh IDs.
Every other play and every default gap retain fixed Follow/render semantics.
No inherited Repeat is implicitly selected.

Case2: select whole inner Repeat inside outer Play1, global[27,47), duration20
including both inner gaps. Expected isolation:7body nodes only. Its fixed Follow
uses retained output offset1,duration24 and Smoothstep. Inner centered escalation
step1/8 stays a distinct inner layer; source target clock resets per Retime play.
The two default background gaps keep fallback while creep's owner clock advances.
No final gap is inserted.

Each test runs typed point and region targets:4cases*50frames=200picture samples,
with52selected-owner samples independently recomputed. All remaining layers/views,
providers, captured contexts, gap identities, Original points, audio support and
duration are compared with the exact identity mapping described above.

## Independent timing/target/clipping oracle

Retime source points at six output centers:
1001/3,1001,5005/3,7007/3,3003,11011/3.
Decoded source ordinals are separately checked as0,1,1,2,3,3. The Follow resolves
the exact fractional SourcePoint, not its decoded ordinal.
Targets interpolate Tracked0:x1/4 toTracked2002:x3/4; Lost2202 retains3/4;
a distinct future Tracked2802:x3/8 must not interpolate the Lost frame;
manual correction3003 then gives7/8; target span ends3500 and excludes last source point.
Moving millionth centers at the first three points are333333,500000,666667.

Source crop centered3/4 atscale2 maps target x to2x-1 and clips. Provider Sequence
then zooms1/2, so surviving centers are target-1/4:
1/4,416667/1000000,1/2,5/8. First target maps negative, then WOULD reenter at.083333;
the earlier clip is irreversible and must select fallback(1/8,7/8). Last source
point lies outside target span and uses the same separately scaled fallback.

For Repeat-owned Follow, surviving centers additionally pass through centered
per-play escalation1,9/8,5/4, then round to Q32. Expected center fractions and scale
use independent small integer round-even division. Scale normalization is
(f+1/2)/6 for the selected Retime, or(f+1/2+1)/24 for the selected Repeat's retained
clock. No production framing evaluation, target-region lookup, quantization or
projection function constructs expected values.

## Proof limits

Pure generic scoped preparation and Plan evaluation. Core/CLI/native owners
separately prove semantic authority, live Camera, macros and history storage.
No native/media/GPU/PCM/performance claims. This bounded fixture does not cover
every arbitrary nested Repeat/Retime topology, accepted generated providers,
marks or audio effects. Empty marks are explicitly required by identity counts.
The raw transaction, inverse and exact address changes are fully exercised here
by this actual focused run.

Executed focused command:
CARGO_TARGET_DIR=/tmp/deadpan-follow-creep-2026-10-24/target
DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
cargo test --locked -p deadpan-plan --test picture_plan scoped_follow_creep

plan-run.py retains exact command/env/streams/binary observations and every
owned/CorePlan/named-manifest source snapshot as .rs.txt, including failed runs.
Other lanes' manifest presence/changes are recorded explicitly, without claiming
Root's final source freeze before those lanes finish.

## Final owned source pins

- crates/deadpan-plan/tests/picture_plan.rs: 91083bytes SHA256 872a4f0494a5cb5a9b0a19cd854c6e9314ef22a2b8a44ba1190f12ab49cfa04c
- crates/deadpan-plan/tests/picture_plan/scoped_follow_creep.rs: 21889bytes SHA256 2df29ab6958ba388e3db8d7982f7cddeb7b7e427364739bc172eeb51e36151e9

## Retained style failure and proof boundary

Root's clippy-first.stderr reported only clippy::collapsible_if at this module's
line218. The old exact module and registration are plan-pre-clippy-fix-*.rs.txt;
original plan-freeze.json / plan-handoff.md remain unchanged, and additional
verbatim copies are plan-pre-clippy-fix-plan-freeze.json and
plan-pre-clippy-fix-plan-handoff.md. The original Clippy failure is not rewritten.
The new source collapses the nested inner-play/Some map condition to a let-chain.
No production code, assertions, fixture values or behavior changed.

The focused2tests reran after scoped rustfmt and passed. The new receipt captures
actual named22source paths plus all Core/Plan src .rs basis before/after unchanged,
and the executed binary hash. Root owns strict Clippy rerun and final broad,
native/media/source freeze/progress/artifact verification. Cargo released and no
repository writes after the passing rerun.
