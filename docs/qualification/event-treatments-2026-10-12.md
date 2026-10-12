# Independent sound treatments, 2026-10-12

Root and attached sound events now own a complete `AudioTreatments` recipe.
Core 53/store 81 replace the previous separate gain and send fields; obsolete
development packages fail their version gate without writes. Frozen owner
context stays at 12. Source identity, placement, selection, routes, edges and
Hold allowances remain independently owned.

## Processing and clocks

The per-event chain follows time/pitch mapping and input edges. Its default
order is gain, EQ, saturation and balance; the serialized recipe retains any
supported explicit reorder. Continuous and allocated-Tail sends tap its complete
processed dry output. The event's own continuous return cannot feed its own
Tail tap. Parent treatments receive the combined dry and returned contribution.

The automation clock starts at the complete natural-rate sound onset in exact
local project-frame units. It is independent of a later selection, cut or root
move. Occurrences carry enclosing time maps; routed inputs retain old integral
sample labels and their old clock. Inserted route gaps remain zero and do not
advance envelope or filter history.

EQ recursively evaluates only its preceding stages with complete finite history
and look-ahead on fixed 1,024-sample tiles. Wet effects read the complete ordered
chain on their 4,096-sample tile grid. Current Hold/cut gates apply afterward.
The processed dry path uses a separate output envelope so its original input
edge is not applied twice. Continuous returns retain the selected output fade.
No effect grants audible duration beyond the sound's existing allocation.
Neutral EQ and centered balance retain the ordinary constant-gain delivery path.

Combined owner/event temporal counts, depth, plan work, prepared frames,
dependencies, cancellation and deadlines remain bounded. EQ workspaces, wet
workspaces, retained tiles and transient PCM count against residency. A graph
lives within one immutable preparation request; this is not persistent filtered
caching or a realtime performance qualification.

## Native authoring

The placed-sound inspector teaches EQ, saturation, balance, mute and envelope
commands alongside its existing gain, edges and sends. The five new verbs are
`:sound-eq`, `:sound-saturate`, `:sound-balance`, `:sound-mute` and
`:sound-envelope`. A matching exact envelope range is replaced. Clearing curves
preserves trim, mute ranges and other effects. Each command captures the selected
sound, session and revision and saves through one typed reversible transaction.
Attached voices use `SetBeatSound`; this does not add an attached-sound selector.

## Verification

Evidence: [retained commands, media, reports and source identities](../../tools/audio-qualification/evidence/2026-10-12-event-treatments/README.md).

All 3,969 tests across core, plan, store, audio, playback and app pass, with nine
existing opt-in ignores. The initial run completed app/audio, then stopped on
the frozen core fixture's obsolete sound fields. Both test-only loaders now
adapt its trims without changing the frozen input. The four remaining crate
suites were rerun to completion. A new aggregate-budget test initially stayed
below its intended limit; valid cubic controls now make it exercise the actual
aggregate refusal. An attempted rerun selected `ui-harness` without the app
package and was corrected before compilation.

The first focused run passed eleven real-PCM tests, including the new direct
convolution oracle, reordered stages, cold reads, input-edge ordering and
processed history through inserted gaps. Strict workspace/all-target Clippy
passes. Initial checks caught a redundant result wrapper and a test using a
field as a method; both were fixed. Command-reference regeneration deliberately
fails after writing its reviewed output. Final strict workspace/all-target Clippy,
formatting, whitespace checks and release builds pass.

The real release H.264/AAC export passes: 30 pictures, four audio blocks and one
signal window at verified zero offset. Minimum block SNR is 43.3605 dB and
maximum level difference is 0.023135 dB. The fixture has an onset-local gain
ramp, +3 dB middle EQ, 6 dB saturation, 50% right balance and a -6 dB delay send.
The unchanged Original click, event burst and echoes all verify. Headless
placed-sound editing parity also passes. The MP4, project package, render receipt,
verification report and executed binary identities are retained.

The first release export attempt stopped in the fixture's signal check before
encoding. Its expected peak described the unattenuated right channel, but the
shared signal helper checks the left channel. The recipe's 50% right balance
halves the left signal. Both expected echo bands were divided by two with the
same relative tolerance; the measured first peak was 0.19900201261043549. This
was a fixture expectation correction, with no production DSP change.

Five final release offscreen replays pass 663 scenario checks: wet returns 117,
sound placement 259, Tail sends 81, audio treatments 124 and EQ 82. Each also
passes its Kestrel shortcut audit. The new commands preserve absent/stale captures,
complete recipes and each Undo step. The initial placement replay stopped after
three wheel steps before reaching Remove sound in the taller inspector. Its
helper now permits eight bounded attempts, retaining real wheel input and the
same full paint-visibility check. The failure report and image remain retained.

The 960×640 and 1280×820 treatment captures were inspected against the workspace
and sound-placement boards. Values/key hints wrap inside the inspector, the
picture remains visible, selection/focus stays distinct and the footer remains
anchored. The inspector scrolls at minimum size. Final treatment PNGs are
byte-identical to the inspected initial release captures.

Separate existing observation: the sound-placement scenario's `Sound removal is
reachable at 960x640` checkpoint has a black viewer while its semantic state
reports displayed frame 3. The previous event-return increment's same checkpoint
also has that appearance. Both images are retained as `placement-viewer-current.png`
and `placement-viewer-base.png`; this is not picture-presentation evidence.
Diagnose the harness/presentation discrepancy in a follow-up using the retained
reports and that exact checkpoint. This does not occur in the treatment captures.

The primary agent is implementing and reviewing this work alone. No independent
review, live GUI, physical keyboard, device output or listening check is claimed.
Partial attached-sound scope routes, broader loop/incoming-tail qualification,
persistent filtered caching, mastering and performance remain open. DP-09 and
DP-16 and the full product goal remain partial.
