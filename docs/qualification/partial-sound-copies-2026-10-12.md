# Partial attached-sound copies, 2026-10-12

Eligible partial edited-range copies now retain attached sounds. Capture keeps
the complete sound owner and recipe, and paste selects delivery through an
exterior Partition. Recopies append the current placement to the immutable
sound-clock journal. Deleting the original owner leaves the copied attachments
and their retained input available. No persisted schema changes.

The range selector still requires Source, ordinary Hold or supported-fragment
endpoints. Partial composite Preserve ranges remain refused. This does not
implement splitting a surviving attached-sound owner, Repeat transforms,
root-owned attachment transport or native attachment placement.

## Processing and delivery

Frozen layout resolution derives each occurrence's complete processing origin,
visible allocation and ordinary delivery anchor. Transparent crop ancestors are
eligible for correspondence. PCM and event envelopes use the same chronological
sample translations and interval intersections. Copy boundaries do not invent
new event fades, and later expansion cannot revive lost delivery samples.

Temporal treatment preparation retains the complete input of its paired owner.
An NTSC recopy exposed a boundary loss: original input sample 9609 must reach
preparation sample 20821, although the new nominal allocation ends at 20821
exclusive. Dropping it changed shared-EQ output by 0.00187165. Preparation now
retains that sample; ordinary delivery still applies its own mask afterward.
A new outer wrapper consumes delivered sound and cannot expose the copied
owner's hidden history.

Integral transport also preserves ties-to-even ownership. At an exact half
sample, an odd integral translation followed by fresh rounding can move an edge
again. Transported preparation grids retain the original parity, including
through domain rebranding. New ordinary grids retain their previous behavior.

Current structural gates are evaluated on the retained preparation phase.
Owner gain clocks remain independent and current. Candidate lookup has a
bounded rounding margin; the exact route alone admits samples. Source admission,
work, cancellation, deadline and PCM residency limits remain enforced, including
warm-cache reads. Public owner-input requests remain limited to 256 frames.

## Verification

The broad debug run passes 2,859 core, plan, audio, playback and store tests,
with none ignored. Command-created copy/recopy/deletion cases compare dry PCM
exactly and event EQ/reverb plus shared-owner EQ within the existing 1e-6
tolerance, with hard and automatic edges. An independent new-wrapper EQ oracle
uses only delivered input. A direct owner-input test checks the nonzero hidden
terminal sample, cold/warm equality and failure after source revocation.
The grid matrix and a canonical half-sample EQ regression cover odd translation.

Store tests retain an immutable register after original-owner deletion and
reopen, force a failed transaction without changing snapshot/history/registers,
compare preview with commit, recopy, and exercise durable Undo/Redo and read-only
reopen. Core tests retain the complete recipe and chronological clocks after
copy, move and recopy. Foreign owner contexts and invalid work budgets reject.
All seven focused audio regressions and the store persistence regression also
pass in release. Formatting and strict workspace/all-target Clippy with
`ui-harness` pass, as does the app/CLI release build.

The public CLI fixture starts with the full Original Source, moves its own audio
outside the allocation, and registers a real stereo WAV. Both the event and its
owner have nonneutral EQ. It copies frames 2..6 to 120..124, recopies 121..124
within the copied group, and deletes the original 120-frame owner. Four PCM
comparisons before/after these edits pass without changing the 1e-6 tolerance.
The resulting seven-frame H.264/AAC export passes all seven picture comparisons,
19 audio blocks and one verified-zero-offset window in debug and release. Its reported
minimum block SNR is 1.9900 dB and maximum level difference is 1.5286 dB under
the existing verifier thresholds; this is not acoustic-quality qualification.
[Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-partial-sound-copies/README.md)
includes the movie and fixture project, source and executed-binary hashes,
command logs, the implementation patch and self-review. Its integrity checker
also compares the source checkout with the retained per-file hashes.

## Failures and limits

Retained failed attempts exposed missing context remapping for a new wrapper,
the shared-EQ terminal loss, unintended admission of owner splitting, a store
test API typo, and an older independent EQ oracle that clipped full input to
nominal delivery. The corrected oracle reads the complete finite filter window;
no waveform tolerance or resource limit was raised.

The attempted partial nested-Preserve authoring test reached the existing
range-endpoint guard and is now an explicit refusal regression. Initial export
fixtures also used an ineligible shortened wrapper or omitted the recopy group.
A mixed Original/attachment PCM oracle differed before isolation. The final
fixture tests attachment timing with the Original's audio outside the allocation;
it does not assert that a combined bus is unchanged across new Original cuts.
The [mastering-context follow-up](copy-mastering-context-2026-10-12.md) resolves
that difference: the authored mix is unchanged and current master gain explains
the limited output exactly.

The primary agent implemented and self-reviewed this increment alone. No
independent review, live GUI, physical audio or listening evidence is claimed.
Broader scope transforms, incoming tails, persistent
filtered caching, mastering and performance remain open. No requirement or
release gate is newly complete.
