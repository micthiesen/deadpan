# Sequence Follow creep Plan test handoff

Own only:
- crates/deadpan-plan/tests/picture_plan.rs: one focused module registration.
- crates/deadpan-plan/tests/picture_plan/sequence_follow_creep.rs: two tests.

No Plan production changes, Git commands/mutations, docs, or other lanes touched.
Scoped rustfmt only these two files. Source hashes are in plan-freeze.json and
plan-focused-1.json, with exact Core/Plan production .rs basis hashes in the
focused receipt. All owned sources and production basis stayed unchanged during
the focused run. Main supplied base fea6b081775165f9830ba14d938576d346fec6c8.
No final combined 14-file source freeze is claimed.

## Verified behavior

Sequence g occupies edit [2,17), duration15, between two 2-frame silent neighbors.
Children have independent nonidentity static poses and real SourceAudio mappings:
a5frames maps Original ticks0..10010; b7 maps20020..34034; c3 maps0..6006
from another registered asset. Target is saved typed point or rectangle with
tracked interpolation, Lost samples, manual corrections and display aspect16:9.

Every output picture across all19frames is checked against the fixed Follow
baseline for provider identity, retained context, and all child/ancestor framing.
All15group frames have an independent integer round-even Q32 scale oracle,
whole-group local/effective duration, hand-fraction target centers and Original
ticks. First/seam/last centers are also explicit. Live scale1.5->2.5 and separate
fallback.75->1.25 use factor5/3. Target span end33033 is exactly b's last sampled
point and excluded; c's otherwise covered coordinates isolate asset mismatch.
Lost positions continue until correction; source time jumps across the cut while
OwnerOutput progresses continuously. Linear/Smoothstep/Step all check every
group frame. Audio duration and complete nonempty Source audio-plan spans match
before/after. Child records, group kind/other fields and total duration match.

No nested captured Hold case was added in this bounded Sequence fixture.
Captured Hold projection retains its existing independently verified Plan lane.
This is generic serialized Follow testing; Core and App authoring admission
are owned by their other lanes. No production defect found.

## Focused command and result

CARGO_TARGET_DIR=/tmp/deadpan-follow-creep-2026-10-24/target
DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
cargo test --locked -p deadpan-plan --test picture_plan sequence_follow_creep

plan-focused-1: terminal0, 2passed,0failed,0ignored,127filtered,3.985s.
Exact argv/environment/stdout/stderr/source pins retained. No failed test run.
Cargo target released to Main immediately after terminal0; no active process.
Final broad tests/native/media/source freeze belong to Main.
