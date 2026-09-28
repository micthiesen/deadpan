# Sound allowance qualification, 2026-09-27

One root sound can now be explicitly permitted in one concrete silent Hold or
Repeat gap. Other sounds, Original audio and other pause occurrences retain
their existing suppression. Core 31/database 37 persist this relation separately
from sound recipes and chronological routes. No full-product requirement or
gate becomes complete through this increment.

## Authoring and playback

`SetSoundAllowance` validates the sound and full stable occurrence address and
commits an inverse patch through ordinary history. Split copies the appropriate
addresses, occurrence isolation remaps only the selected full path, and deletion
prunes removed owners. New pauses gain no permission. Replacing a recipe under
the same sound identity retains its explicit allowances. Entries, depth and
aggregate serialization are bounded.

Sound preparation reads the unsuppressed input. Current Hold rules, the sound's
explicit allowances and endpoint edges combine afterward on the contribution's
output clock. Original suppression remains independent. A permission never
creates selected media in a retained route gap or beyond selection exhaustion.
Source dependencies remain admitted even when their output is entirely gated.

The inspector and `:sound-allow` / `:sound-silence` share the revision-bound
service. They retain the sound, Edit frame, concrete issuer, session and revision,
including an absent or failed target. The service re-resolves the scope and
storage rechecks the sound's source receipt. Revocation remains available after
sound selection moves away. Very short effects are eligible through overlap
with the pause's samples inside the current Edit frame. Multiple pause issuers
in that frame are rejected explicitly.

## Review and migration

Independent reviews covered core identity transforms, storage and legacy replay,
plan/audio gates, and native command capture. Core review caught a shadowed Split
context variable during compilation. A test collection also needed an explicit
element type. UI review identified the subframe-effect eligibility problem:
checking only a frame's first sample could make a valid short effect impossible
to allow. The correction queries the frame's bounded pause interval and uses
indexed half-open selection overlap. Follow-up review caught an empty sample
island on an identity route being treated as overlap; the lookup now excludes
empty support and tests a sampleless selection inside the frame. A retained
audio-context constructor also needed explicit empty allowance state.
The full compile gate found a native command handler returning a submission
boolean in only one match arm; both branches now complete with unit.
The first complete test invocation stopped at a diagnostic test still expecting
core 30/database 36. Its explicit expectations now match 31/37 and migration
through database 36; earlier passing groups are retained in that failed run.

The first decoded-audio witness used a WAV whose unspecified speaker layout is
correctly refused by canonical preparation. The witness now uses the existing
qualified MP4 audio stream, with two pauses spanning its selected contribution;
source admission was not weakened. The next witness called source-only edge
inspection and correctly observed no overlay. It now reads the canonical
limited authored bus, the same path used for audition, while checking Original
audio separately. Failed development attempts remain evidence.

The frozen core-30 adapter retains its original command grammar and contextual
admission. Database-36 snapshots gain empty allowance state. A genuine old-CLI
fixture contains 15 revisions, 10 history entries, a pending redo, an abandoned
branch, a source receipt and chronological sound routes. Migration compares all
snapshots and transactions before promotion; forged allowance fields, commands
and patches are refused without upgrading the package.

## Visual target and acceptance limits

The existing [ImageGen sound board](../design/boards/sound-placement-board-v2.png)
already includes the selected-sound pause permission. Its retained prompts remain
the target for the inspector, separate event selection, visible keys and picture
priority. Current controls identify the exact pause and expose only implemented
commands.

The first complete replay found composition Enter surrendering the command
field's focus. The app router had suppressed submission, but the single-line
text widget's own Enter handling still blurred the field and closed command
mode. Command submission must remain owned by the router while the widget
retains native composition. Screenshot review also found database IDs crowding
the pause inspector. It now uses the pause's name and readable Repeat play
positions alongside the Edit frame; the full stable issuer remains in the
captured command. The explanatory text is shorter to preserve useful space.

The final inspected captures show the permission and revoke controls, their
visible commands and readable context at 960×640 and 1280×820. The minimum-size
revoke button wraps inside its clip. Lavender selection, the yellow Edit cursor,
separate Original/edit clocks and picture area remain intact. These inspector
captures are scrolled to the permission section; upper fields and catalog
details may be outside their scroll view. Thumbnails, richer sound rows and
further spacing polish remain work toward the full board.

This is a root-event subset. Nested ownership, send/tail policy, the full voice
graph, remaining structural sound transforms, listening, encoded export and
preview/export equivalence remain open. Production replay uses real media and
storage with Metal; simulated keyboard/IME events do not prove native input
delivery, VoiceOver or physical display appearance.

## Verification

The complete locked workspace run passed **1,910 tests** across 152 groups,
with zero failures or ignored tests. Formatting and strict workspace Clippy
passed at source manifest
`9fa3bfb6b45bd8ba489a8c24253d0f542fc20a5b51684e9b04af27ce73abc4b5`.

The subsequent IME and inspector fixes changed only five app files. At final
source manifest
`379db4507769ccb2a5e815874f546b0817b70e0cce58c766c258f20457c4bd5f`,
formatting, strict base-app and optional-harness Clippy passed. The normal app
passed **233 unit tests and 2 headless integration tests**; the optional harness
passed **262 unit tests and 2 headless integration tests**, with no failures or
ignored tests. Unchanged backend suites were not restarted after these app fixes.
All runs use base commit `357ae533b399ccf51e6dc3196ba2e0bba720ef9f`.

The final complete Metal replay passed **819 checks**, including **190** in
sound placement, across fifteen UI scenarios and the separate shortcut audit.
The bounded intermediate-screenshot quota is the only warning; named checkpoint
images and semantic frames remain available. The first replay's IME failure is
retained, not counted as successful evidence.

The separate release replay passed **1,789 checks** at the final source identity,
with no findings. Warm navigation CPU and input-to-picture p95 were **0.140 ms**
and **1.505 ms**, respectively, over 120 inputs. The 10,000-beat structural
fixture's navigation CPU p95 was **0.332 ms** over 160 inputs. Cached Repeat and
inserted silent-freeze input-to-picture p95 were **1.660 ms** and **2.360 ms**,
respectively, over 40 edits each. All existing budgets passed. Picture completion
means offscreen GPU completion; the large fixture contains Background/Silence
Holds and does not qualify large media or audio workloads.

Runs used Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned LGPL FFmpeg 8.0.3
prefix. Every Cargo command completed before the next started. Thermal state,
power conditions and OS file caches are uncontrolled. These small fixtures do
not establish full-size workload performance or physical display latency.

The [retained evidence](../../tools/ui-feedback/evidence/2026-09-27-sound-allowances/README.md)
contains exact commands, source identities, failed attempts, full compressed
replay reports and the five inspected captures.
