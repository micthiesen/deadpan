# Adjacent Source Roll, 2026-10-01

Implementation base: `0304852f7485d730e117d89f649b9ce0421b4945`.
[The command contract](../SOURCE_ROLL.md) adds one atomic Roll between literally
adjacent qualified Sources or unity Partitions under an ordinary Sequence.
Both exact edge intervals constrain one shared movement. Pair and project
duration stay fixed. Native Trim mode remains required.

Core schema 41 and database 50 identify this command. Unused development
databases 39 through 49 are refused without mutation under the session's format
policy. Frozen audio context remains schema 6; older command grammars reject
Roll. No requirement or gate is complete.

## Behavior and evidence

Eleven new core tests cover direct/Partition combinations, fractional selection
padding, strict shared bounds and controlling-side reporting, hidden physical
context, dormant versus absent audio, independent mappings and offsets, scope
rejection, physical overflow, identity validation, retained effects and clocks,
root sounds/routes/allowances, mark loss and bias, and closed wire grammars.
Undo restores exact authored state. Audio clocks are captured once from the old
tree; Roll adds no ripple reanchor. Only the changed seam receives new editorial
edge intent.

One indexed-picture test checks both signed origins and all direct/Partition
pairs against independent VFR PTS/ordinal expectations, including fractional
terminal padding and the unchanged suffix. This qualifies mapping, not decoded
or displayed picture pixels.

Six decoded-PCM tests use the retained 48 kHz stereo and 44.1 kHz mono fixtures.
They check the exact 1,601-sample transfer at an NTSC seam, independent fades and
Hard precedence, right-prefix growth, dormant activation, absent audio, offsets,
prior symbolic resumes and chronological reanchors. Repeat gaps, opaque Preserve
output and the remaining suffix keep their absolute positions. Literal phase
and support oracles distinguish wrong-phase controls; reads are bounded at 256
samples. New filtering support can change samples inside the sinc halo without
changing retained phase. The correction below tests those separately.

Five store tests register two distinct measured MP4 Originals. They verify the
literal six-frame pair, seam at frame 3 and shared clamp `[-2,2]`; independently
reject invalid receipts, asset records and ownership on either side; validate
zero/stale/resource requests; and inject a failure after history insertion with
an existing redo branch. Failed requests preserve the database state. Successful
Roll saves one transaction and survives reopen, fresh-revision Undo and Redo.
Two CLI tests compare cold and live dry runs, one-commit behavior, zero refusal,
metadata rejection and durable history through the shared admission path.

## Review and corrections

Independent reviews of the staged core, staged store path and integrated
store/CLI tests found no actionable issue. These are static reviews; runtime
results are recorded separately.

The first selected run failed to compile because a new mark test named
`AnchorLossPolicy::Drop` instead of `DeleteOwned`. No tests ran. The corrected
selected run passed 57 tests and failed one reverse-Roll PCM expectation.

That test compared a probe only 99.6 samples beyond the old support boundary
against the old zero-extended support. Roll had legitimately exposed more
samples inside the resampler's 128-sample radius. The correction retains the
probe with an independent expanded-support oracle and requires it to differ
from the old oracle. A second probe 199.6 samples beyond the old boundary must
match the retained body exactly. Before runs once at chunk size 193; after runs
at sizes 193 and 239. The corrected test passed, and an independent review
confirmed the support/phase distinction. No production change or tolerance was
needed.

The full workspace then failed only on the CLI doctor's stale core-schema
expectation, 40 instead of 41. Only that test file changed afterward; its exact
rerun passed with the same workspace feature graph. Original failures and source
inventories remain recorded in the evidence.

## Results

| Check | Result |
| --- | --- |
| Full workspace with `deadpan-app/ui-harness` | 3,275 unit/integration tests passed, one stale doctor assertion failed; both documentation tests passed. No ignored tests. |
| Corrected doctor assertion, same workspace feature graph | Passed. Only this test file changed; production sources match the full run. |
| Selected Roll checks | 57 passed and one PCM oracle failed; its corrected focused rerun passed, followed by the full run above. Counts overlap. |
| Strict all-target workspace Clippy with `ui-harness` | Passed with warnings denied. |
| Final formatting | Passed. |

Together the full run and corrected doctor test cover all 3,276 workspace
unit/integration tests and both documentation tests. The full invocation remains
failed in the evidence; it is not represented as an entirely passing run.

Full-run source manifest:
`22f7a05f72e5a579869abd9c0ec79cf01c374e846d331b4f0f5eb6f2ef367dfb`.
Corrected-test/lint/final-formatting manifest:
`c0d07d19964a564ff4cd5df9ab634fb9d171a0497e73de594bf792544b662b06`.
The retained source comparison identifies only
`crates/deadpan-cli/tests/project_commands.rs`. The existing debug linker
`__eh_frame` warning remains. Default-feature app checks, Python suites,
release build and painted replay were not repeated for this backend increment.

[Evidence](../../tools/media-qualification/evidence/2026-10-01-source-roll/README.md)
retains command output, exit codes, timings, before/after source manifests,
reviews, fixture hashes and the exact test-only correction.

## Remaining scope and cleanup

No native GUI was opened for this increment. The final process scan found no
executable whose name starts with Deadpan.
Full Trim still needs one temporary In/Out/Slip/Roll draft, explicit
ripple/overwrite policy, paired boundary pictures, waveform and audition,
one Enter commit and Escape restoration. Mixed draft controls require a combined
timing and root-sound transformation. Audio-only picture lead/tail, treated or
nested targets and Repeat occurrence isolation remain unsupported. These checks
do not qualify physical display, device playback, export or release packaging.
