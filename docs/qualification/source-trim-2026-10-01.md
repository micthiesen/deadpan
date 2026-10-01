# Ripple Source Trim and editorial audio edges, 2026-10-01

Implementation base: `40f3320dd117e024699ed8d83d498650dbdfde9a`.
[The command contract](../SOURCE_TRIM.md) adds atomic linked In/Out ripple Trim
for qualified Sources and unity Partitions under ordinary Sequences. The shared
path resolves exact limits, previews without saving, and commits one reversible
transaction. Native Trim entry and its boundary-picture pair remain open.

Core schema 40 and database 49 identify the command and separate editorial audio
edge intent. Unused development databases 39 through 48 are refused under the
session's authorized format break. Frozen audio context schema 6 carries the
same intent; the closed older grammars reject the new field. No requirement or
gate is complete.

## Behavior and evidence

Core checks cover both signs of both edges, exact fractional selected/output
limits, clamping, dormant versus absent audio, coherent non-natural A/V maps,
independent offsets, physical growth/cropping/re-extension, retained effects,
root sound transforms, marks, resource-limit rejection and exact inverse patches.
Mark checks distinguish stored physical/PTS bindings from visible occurrence
queries, both boundary biases, wrapper/ancestor clocks and per-binding loss.

Indexed-picture checks use a synthetic signed-origin VFR index with literal
expected PTS/ordinals and retained Source/live group camera evaluation. They
qualify indexed mapping, not decoded or displayed pixels.

PCM checks decode the retained 48 kHz stereo and 44.1 kHz mono WAV fixtures
through explicit test-provider speaker matrices. Video clocks in these pure
command fixtures are declared and do not establish measured video admission.
Literal sample/support oracles verify physical prefix/tail material, retained
phase, prior symbolic resumes, chronological reanchors, dormant activation,
offset application once, Repeat gaps, opaque Preserve output and outer Sequence
siblings. Bounded 256-sample reads are compared exactly across cold reverse
chunk orders. Wrong-phase controls distinguish the oracles, and inverse patches
restore exact PCM.

The moved Trim edge and its incident neighbor receive short creative fades.
Slip now marks both changed joins and their incident neighbors, including on
transparent Partitions. This intent is independent of raw filtering support.
Exactly coincident Hard policies win; ramp gains combine by minimum. Unchanged
retained edges keep their original width and progress. New edges use the actual
delivered incident sample interval, including a one-sample fractional-retime
case. Binding tests cover delayed entry, early exit, opaque inner owners and
later Partitions whose hidden Source context overlaps an earlier marked edge.
Split retains marked owner context. Ungroup refuses to discard a marked
Sequence until explicit transfer or clearing is implemented.

Store checks use real measured and registered MP4 sources. They cover valid
zero preview without history, invalid timing/wrapper metadata, exact receipt-bound
crop, atomic commit, reopen, durable Undo/Redo with fresh revisions, historical
validation and missing-receipt refusal. A second history scenario retains the
new edge through an explicit Hard setting, two Undo steps, reopen and two Redo
steps. CLI checks cover zero/raw-zero refusal, handle clamping, cold/live-writer
dry-run equality, preview/commit equality, stale rejection and durable Undo/Redo.

## Review and corrections

Independent core, plan/audio and store/CLI reviews are retained with their
follow-up findings and resolutions. The final bounded reviews found no remaining
actionable issue. Runtime verification is recorded separately below.

Review found that the first Trim implementation preserved raw phase but omitted
the required creative fade. The original raw-equals-faded test was incorrect.
The correction introduced separate editorial intent and also fixed the existing
Slip-on-Partition join. Further review caught binding-ignorant leaf selection,
marker leakage into later hidden-context children, use of old virtual width for
a new one-sample edge, and duplicate recreation of a rejected bound-owner marker.
Each has a focused regression. These findings and the initial passing checks are
retained; they are not evidence that the earlier implementation was correct.

The retained first failures also include typed error conversion, a wrong framing
layer assertion, an invalid absent-audio fixture, stale schema assertions, old
fade API/namespace references, an ambiguous float initializer and a zero-duration
Hold in a new regression fixture. The full workspace first attempt failed on a
stale SQLite schema assertion. The first Clippy run passed while sources were
changing and is diagnostic only. [Failure notes](../../tools/media-qualification/evidence/2026-10-01-source-trim/failure-notes.md)
record the corrections alongside original logs.

## Results

| Check | Result |
| --- | --- |
| Workspace with `deadpan-app/ui-harness` | 3,250 unit/integration tests passed, one outdated Slip assertion failed; both documentation tests passed. No ignored tests. |
| Corrected native Slip assertion, same workspace feature graph | Passed. Only this test file changed; production sources match the full run. |
| Default app | 428 app tests and 3 headless integration tests passed. |
| Focused core/context/legacy/edge checks | 226 passed; the later Slip/Trim join checks passed 35. These overlap the workspace coverage. |
| Focused plan/picture, audio library and composite audio | 36, 77 and 63 passed respectively. |
| Focused store and CLI Trim | 2 store tests and 1 CLI test passed. |
| Strict workspace/all-target Clippy with `ui-harness` | Passed with warnings denied. |
| Final formatting | Passed. |
| Python qualification-tool suites | 83 passed: audio 20, model 54, FFV1 5 and media-host 4. |
| PCM fixture reproducibility | Both retained WAV fixtures matched. |

The workspace invocation remains a failed invocation in the evidence. Its only
failure compared a slipped wrapper against its old value without the required
editorial markers. The correction retains exact equality for every other field,
checks both incident neighbors, and keeps the picture and Undo assertions. The
same test then passed; no other source changed. Together these runs cover all
3,251 workspace unit/integration tests and both documentation tests. Final lint
and formatting passed on the corrected-test source manifest.

Full-run source manifest:
`03c82a9a7178f601ffb605af91da542efb450abac0548bac9f224f947852a107`.
Corrected-test/default-app/lint/formatting manifest:
`cf35fdfdd4008fa19a77b09649297fbe2140ce9bfa97c5b678b483be86978d11`.
The retained source comparison proves that only
`crates/deadpan-app/src/project/tests/slip.rs` differs. The debug linker retains
the existing `__eh_frame` warning. No release build or painted replay was run
for this backend increment.

[Evidence](../../tools/media-qualification/evidence/2026-10-01-source-trim/README.md)
retains commands, exit codes, timings, before/after source manifests, reviews
and fixture hashes. Focused, default and workspace counts overlap and must not
be added together.

## Remaining scope and cleanup

No native GUI was opened for this backend increment. The final process scan
found no Deadpan instance running. Native Trim entry, paired boundary pictures, waveform, audition,
overwrite and Roll remain required. Audio-only picture lead/tail, treated/nested
targets and Repeat occurrence isolation remain unsupported. These checks do not
qualify physical display, device playback, export or release packaging.
