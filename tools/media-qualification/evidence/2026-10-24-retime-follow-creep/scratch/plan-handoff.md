# Whole Retime Follow creep Plan handoff

Owned only:
- crates/deadpan-plan/tests/picture_plan.rs: module registration.
- crates/deadpan-plan/tests/picture_plan/retime_follow_creep.rs: three focused tests.
No production edits, Git commands, worktrees, docs or whole-tree writers.
Only these two files were rustfmt formatted with skip_children=true.
No repository changes after the passing focused run.

## Actual verification

Command:
CARGO_TARGET_DIR=/tmp/deadpan-follow-creep-2026-10-24/target
DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
cargo test --locked -p deadpan-plan --test picture_plan retime_follow_creep

plan-focused-1: terminal0,3passed,0failed,0ignored,133filtered,1.6739s total
(.02s tests). No failed fixture/runtime run and no production finding.
Cargo released to Root immediately after terminal0; no active processes.

plan-focused-1.json contains exact argv/environment, runner hash, actual executed
test executable/hash, stdout/stderr hashes, owned2 before/after source pins,
all287 Core/Plan src .rs basis pins, and actual global17 presence/hash before/after.
Every scope was unchanged during this focused run. Exact pre-run source bytes,
including all17 actual paths plus the Core/Plan basis, are .rs.txt snapshots in
plan-focused-1-source. Earlier written snapshots are plan-written-*.rs.txt.
This is an observed focused-run source snapshot, not Root's final17freeze or
claim that broader App/native/media checks passed. Base supplied by Root:
aa80624cbafda91a1c9d6f8d13b749586133be41.

## Test names

- retime_follow_creep_fast_output_clock_keeps_fractional_target_points_and_child_clips
- retime_follow_creep_slow_retained_clock_crosses_exact_freeze_and_source_boundaries
- retime_follow_creep_retained_endpoints_hold_without_freezing_provider_target_time

Ten bounded cases: fast5outputframes *2shapes*3curves; slow12outputframes
*2shapes with retained-offset clock; same slow geometry *2shapes with retained
endpoint holding. All78 owner picture samples and40 outside-neighbor samples
(118 total) compare actual provider/context/instance/gap/local position and all
nonedited picture-framing layers with the fixed Follow baseline. Full nonempty
Source audio-plan spans, audio duration, total picture duration, child records,
Retime kind/purpose/input map/pitch, targets and override maps remain equal.
Only the owner scale transition differs between the two raw valid documents.

## Exact fixture / oracle

Child Sequence has total input10frames:
A Source[0,4) maps Original ticks0..4004.
Captured Freeze[4,6) holds Original6006, with retained zoom1/2 and own livezoom1/2.
B Source[6,10) maps Original20020..24024.
Group live zoom1/2 applies after every child. A live crop centered3/4 atscale2
maps target x to2x-1 and clips before that group zoom-out.

Retime samples input[1,9), a nonzero mapping origin.
Fast8/5 input rate ->5outputframes. Complete raw owner scale clock is5frames.
Expected input positions:9/5,17/5,5,33/5,41/5.
Expected Original points:9009/5,17017/5,6006,103103/5,111111/5.
Expected decoded ordinals:1,3,6,20,22. Original points are checked separately
from IndexedSourceFrame selection and PTS; at least2 fractional-vs-decoded
witnesses are mandatory per case. Fast frame1 uses Original3.4, producing
target.675 -> child.35 -> group.425. Using decoded Original3 would produce.375,
and the actual pose explicitly rejects that ordinal-based alternative.

Slow2/3 input rate ->12outputframes. Expected input positions:
4/3,2,8/3,10/3,4,14/3,16/3,6,20/3,22/3,8,26/3.
Expected Original points:
4004/3,2002,8008/3,10010/3,6006,6006,6006,20020,62062/3,64064/3,22022,68068/3.
Expected decoded ordinals:1,2,2,3,6,6,6,20,20,21,22,22.
Exact input4 selects Freeze at frame4; exact input6 selects B at frame7.
Both half-open seams are asserted on the actual provider. Freeze Original6006
stays fixed across three output samples while owner scale continues moving.

Targets are saved typed point and region versions of the same path:
Tracked0:x1/4 ->Tracked4004:x3/4; Lost5005 retains3/4.
Manual correction20020:x7/8 then tracked21021:x5/8.
Target span ends at22022, making that exact B point and subsequent points
fallback. Slow A interpolation first rounds to millionths
416667,500000,583333,666667. Independent static expected centers honor those
roundings before framing: fallback1/8,1/4,333333/1000000,416667/1000000.
Freeze3/4 ->captured5/8 ->own9/16 ->group17/32.
Corrected B7/8 ->group11/16; tracked B5/8 ->group9/16.

Fast first target.475 maps to-.05 at A's crop, then would reenter at.225
under group zoom-out. Slow first target416667millionths likewise maps negative,
then would reenter at.166667. Both must remain fallback1/8. Actual poses are
explicitly checked against those tempting but invalid restored positions.
This exercises the just-committed universal live clipping semantics without
calling production visibility/projection helpers to construct expectations.

Scale oracle uses only independent small integer division with round-to-even
on Q32, live3/2->5/2 and fallback3/4->5/4 (factor5/3). Expected target centers
are hand-listed fractions and independent integer Q32 rounding. No Framing
evaluate/quantized or target.region_at helper constructs expected values.
Fast uses all Linear/Smoothstep/Step curves. Slow retained clock offset2,duration18
normalizes (ownerlocal+2)/18, distinct from physical12outputframes and provider
input. Third test retains offset-2,duration4 with Smoothstep: authored scale
holds start/end while current provider target lookup still advances. Actual
early fallback3/4, early live3/2, late live5/2 and late fallback5/4 are explicitly
asserted as endpoint values.

## Proof limits

Pure generic FollowScaleTransition validity and Plan evaluation only.
Core/CLI/native owners separately verify new authoring admission and scope.
No new representation or production helper change. No actual media/GPU/PCM,
gain-bit comparison, arbitrary nested Repeat/Retime depth, history, UI or
performance claim. Existing Repeat escalation/gap coverage was not duplicated;
this lane focuses on nonunit Retime, fractional provider time, retained camera
clocks, exact child seams, Freeze context, target state and irreversible clips.

## Final owned source pins

- crates/deadpan-plan/tests/picture_plan.rs: 91010bytes SHA256 c79623e8b6e0452ab118240031fc0a6e4c8ae9915629729bd819e42ebf554b01
- crates/deadpan-plan/tests/picture_plan/retime_follow_creep.rs: 18427bytes SHA256 3fba182c5c07571e5dbfbda4f532ae2c259f2191c07e85508f982ad1f2c5880c
