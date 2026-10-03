# Scoped Repeat editing qualification, 2026-10-03

Status: scoped value editing verified. No product gate is complete.

## Change

[Scoped editing](../SCOPED_EDITING.md) separates an authored definition from a
concrete picture occurrence. Enter opens Repeat or Retime contents. Explicit
All plays and This play choices apply independently at every nested Repeat.
Gain, Camera and Hold audio edits isolate only the selected shared ancestors
and commit the isolation and value together. Browsing and unchanged values do
not create overrides or history.

The native inspector retains exact affine intervals through Retime and seeks
only root frame centers that sample the chosen occurrence. Cached rows and
scope labels avoid expanding a Repeat into one item per play. An iterative,
bounded picture search avoids consuming the thread stack at maximum document
depth. A bounded representative search can still miss another sampled
descendant in a later play; the UI states that limitation and permits explicit
play selection.

Captured requests include project, session, revision, ordinary Sequence scope,
inspected root, authored target, optional concrete presentation and root cursor.
Prepared receipts map newly isolated identities. A delayed save cannot reclaim
a different play, cursor or focused view. Exact mark-only receipts preserve the
current inspector, including navigation made after mark entry. Undo and unrelated
revisions close stale nested navigation.

## Environment

Base: `c2259d955808be0b6bf0b3ba9c3aa624216b5fe3`.
Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust/Cargo 1.97.1.
Checks use locked dependencies and the FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Debug functional checks do not qualify release
response time, memory or thermal behavior.

## Verification and source identity

[Retained evidence](../../tools/media-qualification/evidence/2026-10-03-scoped-plays/metadata.json)
includes commands, complete source inventories including untracked files,
compressed logs/reports, selected screenshots, sanitized crash frames and a
SHA-256 inventory. Failed and interrupted attempts remain alongside corrections.

- The full workspace gate passed 3,780 tests, zero ignored, including doc tests.
- After the app-only mark correction, 720 normal app tests and 756 tests with
  `ui-harness` passed. Each includes four headless integration tests. These
  configurations overlap and are not added to the workspace total.
- Eight rendered workflows passed 1,083 checks: scoped plays 54, marks 155,
  gain 290, Camera 12, room tone 220, nested pause 72, custom keymap 57 and Repeat 223.
  Repeated scoped runs are not added to that total.
- Each replay also passed the production Kestrel audit: 3,319,728 routing
  cases against 62 reserved bindings, with no conflicts or source drift.
- Locked builds, formatting and strict all-target Clippy passed. The debug
  linker retains its existing oversized `__eh_frame` warning.

The final source inventory is
`99683b8b2797a6d369b547a672733b8f18ff77596c2cdf39ba4ac90c65dc9a8a`;
the final rendered binary is
`8e3fc4a241f92536e3870184c8c645e2092895e3df924acb23267e7097e5f7f4`.
The complete workspace gate ran on stable inventory `eda7fe4c…`. Only five app
files changed afterward for the reproduced mark regression: `preview.rs`, the
scoped replay, scoped integration, model and model tests. The final normal app
tests used exact executables from a fresh workspace Cargo JSON inventory, with
the package working directory and required FFmpeg environment. Final optional
app tests, both strict lint configurations, formatting and the scoped/marks
rendered replays passed on the final stable inventory. Unchanged backend suites
were not repeated after this app-only fix.

Earlier focused checks passed 2,573 core/planner/audio/store/CLI tests and 754
optional app tests. The five-package command exited 0, but its global source
guard exited 1 because two unrelated app layout/harness files changed. The later
complete workspace gate covers their final integration. Full hashes and file
comparisons remain in metadata; source-guard failures are not represented as passes.

Gain and Camera replays used binary `44cebabb…`, as did room tone and nested
pause after the core test-only fix. Keymap and Repeat used `73393228…`, before
the final compact scope-row adjustment. The final scoped replay verifies that
adjustment at both 1280×820 and 960×640. Image review confirms the breadcrumb,
scope choices, selected child and compact Gain owner row remain visible.
These rendered checks do not establish physical display presentation. Gain,
room tone and the final Escape scenario retain their explicit injected-audio
limitations in the reports.

### Release replay

The final release binary
`ec7059cb3c6dce19ce527271df78ec4b4b366b464911f5f8b36b2bcc013cc297`
passed the same 54 scoped checks without screenshot readback. No Cargo build or
other replay was running during this measurement. The tiny real-video fixture
and mixed interaction sequence produced:

| Interval | Samples | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Input frame CPU | 58 | 0.933 ms | 1.434 ms |
| UI frame CPU | 255 | 0.681 ms | 3.424 ms |
| Picture request to offscreen GPU completion | 20 | 2.893 ms | 6.093 ms |
| Input to offscreen picture completion, including cold import | 14 | 91.287 ms | 91.287 ms |

The last distribution includes initial Original preparation. It is not a warm
seek or ordinary-edit benchmark and is retained without dropping that sample.
This run does not qualify physical scanout, full-size projects, acoustic output,
memory pressure or the complete performance gate.

## Findings and corrections

Independent reviews found and corrected maximum-depth off-by-one admission,
quadratic lookup of sparse overrides, and a missed first sampled candidate
when entering a compressed Repeat with a terminal cursor. Native integration
review corrected sound-pane input precedence, Escape playback cleanup, late
receipts taking focus from Original, stale scope after mark jumps and sound
inspector precedence.

The initial scoped Gain comparison needed a fresh private preview revision.
An unchanged proposal now retains the original graph without isolation or a
durable write, while still independently validating the saved head.

The first test compile found an ambiguous test identifier and collection type.
The second found the JSON macro recursion limit in the expanded QA snapshot.
The snapshot was split into smaller expressions; no crate recursion limit was
raised. Three nested tests subsequently used expected allocation names that
the fixture's Insert operation had remapped. Their assertions now obtain the
exact stable identities from the validated fixture.

The maximum-depth navigation test aborted with a stack overflow. The retained
macOS fault frames identify `State::project_at` recursion followed by
`State::sampled`. Projection now uses bounded heap continuations, preserving
candidate order, lazy evaluation, result reduction and the 4,096-step budget.
Tests retain the full 256-edge path and add 255 nested Repeat ancestors on the
normal test stack. A separate LLDB attempt stopped at launch without a
backtrace and was terminated with its owned helper; it supplied no diagnosis.

Native inspection found that Gain and Camera drafts incorrectly retained the
ordinary "Root beat" label. The shared scope label now names every chosen
Repeat branch. The normal footer uses the same scope without repeating the
outer beat label.

The first minimum-size replay passed its label checks, but screenshot review
showed a clipped breadcrumb and child row. The list's default minimum scroll
height exceeded the fixed compact panel. Its scroll area now uses only the
remaining panel height, and Gain uses the existing compact owner-row pattern.
The stronger paint check then caught the child label's final 1.5 points outside
the clip: wrapped scope controls consumed the remaining space. Shorter controls
and a truncated breadcrumb keep the selected row visible. The next attempt
correctly failed an obsolete assertion for the removed redundant scope label;
the replay now checks both scope choices and the Previous/Next controls.
The replay now checks the breadcrumb and selected child's actual paint clips
and later opaque coverage, in addition to the scope buttons and footer.

The first five-package run also rejected a new sound-permission test fixture
because it omitted a required source qualification. The fixture now uses the
same qualified asset pattern as existing tests; runtime admission was unchanged.
The corrected full five-package run passed.

Independent final review found that a mark-only save closed the nested inspector.
The new rendered regression failed against the unfixed binary at "Saving a mark
preserves the selected Repeat play and child". The UI now rebases only when the
typed `Saved` receipt matches the state and workspace session, project and exact
old/new revisions. It updates the document/plan without rebuilding navigation
or cached rows. Tests cover SetMark, DeleteMark, repeated receipts, every identity
mismatch and unrelated revisions. The final replay also holds completion, moves
to another play and frame, then verifies delivery retains that newer navigation.
Successful mark jumps still leave the scoped inspector.

## Initial native observations

A developer bundle opened a private copy of the closed scoped replay package.
Native key delivery and accessibility observations showed:

1. Enter and `:scope play 2` select Edit boundary 120 in a 360-frame Repeat.
2. Minus changes that play from −9 to −12 dB. `:scope all` still shows −3 dB
   on the shared definition at boundary 0.
3. One Undo closes the stale inspector; re-entering play 2 shows −9 dB.
4. The unchanged Gain draft opens with a measured waveform and the retained
   −9 dB value. It is cancelled.
5. Play 3 at boundary 241 opens Camera on the corresponding picture. Plus
   changes scale to 105%; Enter commits and retains play 3 and its cursor.
6. Cmd-Q exits with code 0; a targeted process lookup confirms absence.

These observations used binary `3b78ed5c…`, before the label and iterative
projection fixes. A second developer bundle used `44cebabb…` and a private
copy of the closed native package. Camera showed `Scope: Repeat · play 3/3`
with scale 105%; Gain showed `Repeat · play 2/3` with −9 dB and a measured
waveform. Both drafts were cancelled. The copied package retained 11 revisions
and eight history entries. Cmd-Q exited 0 and both targeted app processes were
confirmed absent. The second wrapper was created after newer source edits,
but its copied binary hash identifies the earlier build; it does not verify
the final compact layout, which was checked by rendered replay.

One batched attempt to close Gain and
immediately issue another command did not select play 3; observing the closed
draft before entering the command selected it correctly. This is not evidence
for reliable arbitrary-speed native input across draft transitions.

The wrapper remains a local developer bundle with external host libraries.
These checks do not establish signing, relocatable packaging, physical display
color, VoiceOver, OS IME, every keyboard layout or acoustic playback.

## Remaining scope

Occurrence-local timing edits, copying/replacement, Repeat count and Retime
changes, implicit gap recipes, macro recording inside occurrences and complete
definition previews remain required work. All DP requirements and Gates A
through G retain their existing open or partial status.
