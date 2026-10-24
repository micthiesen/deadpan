# Repeat Follow creep Plan lane

Only owned repository edits:
- crates/deadpan-plan/tests/picture_plan.rs: module registration.
- crates/deadpan-plan/tests/picture_plan/repeat_follow_creep.rs: three tests.
No Plan/Core production changes or Git commands. Root owns the discovered universal
live-scope clipping fix and its older tests/independent renderer comparison.
Final source freeze: plan-freeze.json. Focused run plan-focused-1 passed
3tests,0failed,0ignored,130filtered, terminal0 in1.6625s (.08s test execution).
Full Root21manifest, owned2source paths and all Core/Plan production .rs hashes
were unchanged before/after. Exact argv/env/stdout/stderr/bytes/executable hash
are in plan-focused-1.json and its companion files. Cargo target released to Root.
No failed runtime fixture and no production mismatch found. Earlier preclip
drafts were NEVER EXECUTED; plan-draft-history.md explicitly records their status.
Root's oldhelper/render-oracle failure is independent evidence owned by Root.

## Fixture and oracle

Repeat local [0,17):
- play0 [0,4), Source points1001,3003,5005,7007.
- default gap0 [4,6), Freeze6006 (Lost x=1/2) or Background fallback.
- play1 [6,10), same Source-point resets.
- explicitly owned gap1 [10,13), captured Freeze1001 with own live pose.
- play2 [13,17), same Source-point resets.
- explicitly owned final gap9 is retained but dormant, excluded from duration.
Two background frames on either side make total project21frames. Every output
frame checks provider/context/instance/gap identity and independent child layers.
Root/children/gap records, iteration order, gap overrides and Repeat other fields
remain byte-structurally equal between fixed and animated documents. Complete
nonempty Source audio-plan spans and audio duration match.

Each target is typed point or rectangle. Tracked samples at0 and4004 interpolate
x=1/8,3/8 for Source1001,3003. Lost5005 retains x=1/2; correction7007 gives7/8.
Child x-shift -1/8 yields input to escalation [0,1/4,3/8,3/4].
Captured owned gap maps source1/8 through retained1/2 and live1/2 zooms:
1/8 ->5/16 ->13/32. Target source time resets per play; outer scale is always
Repeat OwnerOutput local(f+1/2)/17.

Independent positive integer round-even division derives Q32 progress, each
Linear/Smoothstep/Step weight, live3/2->5/2 and fallback3/4->5/4 via factor5/3.
Expected values call no production framing/quantization/target projection helpers.
All17owner frames are checked for both bases where visible/fallback applies.

Escalation modes: None, centered additive+1, targeted additive+1. Last two modes
retain distinct inner PictureFraming scale2/3 on later plays and the owning
play's gap. Target clamp is independently compared to exact [1/(2s),1-1/(2s)]
then integer-grid rounded. Outer center uses the stored escalation center:
(p-center)*scale+1/2, followed by the exact inclusive scope clip.

Centered exterior witnesses: play1 base0 maps-1/2; play2 bases0,1/4,3/4 map
-1,-1/4,5/4 and must select fallback. Targeted scale3 left margin1/6 rounds UP
to715827883/Q, so source-left0 maps exactly-1/Q. That tiny exterior point is
also clipped and selects fallback. No epsilon or clamp-policy change is claimed.
Other targeted samples and captured gap stay visible. Root explicitly confirmed
this boundary with actual renderer closed-point semantics.

Third test replaces child pose with centeredzoom4 and Repeat escalation with
multiplicative1/2. Child source1/8,7/8 maps-1,2 and clips. On play2 the later
zoom1/4 would map them back to1/8,7/8 but cannot restore them; fallback persists.
All17owner frames are checked for both target shapes.

## Cases / proof limits

First test12combinations (2shapes*2defaultgaps*3curves) without escalation.
Second24combinations adds centered/targeted escalation. Third2combinations for
irreversible child clip. Three test names begin repeat_follow_creep_.
No real media, PCM/gain bits, native routing, history, tracking accuracy, arbitrary
nested repeats or perf are claimed. Audio Source spans are unchanged; escalation
gain_step1dB is preserved in the non-None modes, with actual gain qualification
left to Root's media lane. Generic raw Follow schema only; authoring admission
and semantic replay are covered by Core/CLI/native owners.

Executed focused command:
CARGO_TARGET_DIR=/tmp/deadpan-follow-creep-2026-10-24/target
DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
cargo test --locked -p deadpan-plan --test picture_plan repeat_follow_creep

plan-run.py retains argv/env, owned source bytes for every run (including failed
fixtures), source/basis before/after hashes, stdout/stderr, exit and binary hash.

## Final owned source pins

- crates/deadpan-plan/tests/picture_plan.rs: 90937bytes SHA256 27fd5159ade0098c02e66616e5ff6ed5ad870bfd94cab11cf51f3e98e12ba584
- crates/deadpan-plan/tests/picture_plan/repeat_follow_creep.rs: 18811bytes SHA256 6f266c4f9fc23b5da753e5882626d5c119ac4b4953a014deb8d4e35048902c32

Final broad checks, native replay, media qualification, docs/progress/artifact and Git remain Root-owned. No repository changes after this passing run.
