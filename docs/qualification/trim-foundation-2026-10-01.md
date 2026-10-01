# Combined Trim timing foundation, 2026-10-01

Implementation base: `3a5e7b71d098a1184fc84470b636bf12ed016ad1`.
The [foundation](../TRIM_DRAFT_FOUNDATION.md) prepares one accepted
In/Out/Slip/Roll draft with pure geometry, historical Source endpoint phase and
one old-to-final root-sound map. It does not yet provide the combined command,
overwrite structure or native Trim mode.

Core schema 42 and database 51 identify the new persisted timing vocabulary.
Unused development databases 39 through 50 are refused before writes or backups
under the approved session policy. Frozen audio context remains schema 6;
supported older binding and sound-route grammars reject the new vocabulary.

## Coverage

Fourteen new geometry tests cover complete-intent resolution, one-active-control
clamping, exact fractional padding and source handles, independent audio offsets,
dormant versus absent audio, both physical prefixes, wrapper budgets, signed
overflow and exact zero/reversal. Policy changes preserve the accepted values or
refuse. Overwrite resolution reports intervals before structural overlay and
does not authorize consuming neighboring beats.

Eighteen new endpoint tests cover closed Start/End selection, chronological
reanchors, origin rebasing, strict wire and historical grammar, definition
births and bounded billion-play resolution. Copying a slice preserves independent
historical aliases across two pastes and exact inverse patches. Its final
allocation-entry step retains a literal one-sample phase difference from the
control that removes the earlier endpoint. Empty structural allocation fails;
absent or dormant audio retains its structural clock.

Decoded endpoint tests use real 48 kHz stereo and 44.1 kHz mono fixtures with
independent phase and filter-support expectations. They distinguish exact closed
endpoints from last-sample approximations, retain prior resumes and offsets,
expose dormant material and make no media reads for absent audio. Reads use two
uneven chunk sizes and reverse queries. These tests construct atomic patches to
qualify the primitive; they do not establish a combined authoring command.

Twenty-five new root-sound tests cover the bounded three-interval map, true cut
flags, scalar edge equivalence, extreme arithmetic, history/resource limits,
identity preservation, prior routes, allowances and support exhaustion.
Sampling, retained support and envelopes consume the same normalized projection.
Contiguous retained intervals with equal translation merge before allocating
sample origins or fades. Slip/Roll alone do not move the independent sound bus.

At NTSC rate, the In-only witness maps new sample 3203 to old 4804, with offset-7
source phase 4797 at 48 kHz and `705159/160` at 44.1 kHz. Equal In/Out maps final
sample 9609 directly to old 9609; sequential deletion/insertion instead reads
old 9608 for a full sound and exhausts a separately isolated terminal event.
Eight decoded-PCM tests check these phases, real versus artificial seams,
sample-centered ramps, exact Hard precedence, prior gaps, tiny selections and
originally sampleless intent. The reconstruction kernel is shared with the
independent phase/support oracle; this does not independently qualify the kernel.

## Review and corrections

Independent static reviews cover geometry, endpoint semantics and lifecycle,
the core/plan sound projection and the PCM expectations. The retained reports
distinguish static review from execution.

Initial attempts found test integration defects: an ambiguous legacy validator,
Sources with neither picture nor audio, and missing integration-module paths.
They were corrected without changing production behavior. The copied-slice
test initially expected the earlier endpoint after capture had appended a newer
allocation-entry step. Its stronger replacement verifies exact phase and a
negative control. That control also needed to retain the second paste's binding
when retaining its timing records.

One earlier-endpoint PCM expectation allowed filtering from the entire file.
The physical owner's local start maps to source sample 993 after its signed
placement and offset, so permitted support is `[993,8197)`. A single diagnostic
reproduction and independent review confirmed the unchanged retained phase
`4968/5`. The corrected test rejects both the wrong phase and the extra filter
samples, then requires whole-file equality 200.6 samples inside the valid
support. It passes at both chunk sizes without tolerance or production changes.
The temporary diagnostic block and original failed runs remain in the evidence.

## Results

Focused verification covers all 57 new tests and existing core, plan and PCM
behavior through passing runs. Earlier failed invocations remain failed in the
record; counts from repeated scopes are not added.

The complete workspace with `deadpan-app/ui-harness` passed all 3,333
unit/integration tests and both documentation tests, with none ignored. Its
source inventory remained unchanged throughout the 1,820.265-second run:
`bec83fe4b40a1cc4931059b0462b3535b58af69f626b279435f285415afc9a6f`.
Strict workspace/all-target Clippy with `-D warnings` passed in 901.441 seconds;
final formatting passed in 1.700 seconds. Both retained the same source inventory.
The existing debug linker `__eh_frame` warning remains.

The [retained evidence](../../tools/media-qualification/evidence/2026-10-01-trim-foundation/README.md)
includes every passing and failed invocation, source inventories, independent
reviews, fixture hashes, host details and a checksum seal. The final process scan
found no Deadpan executable running.

## Remaining scope

No native GUI was opened for this increment. The combined atomic command,
qualified overwrite through neighboring contexts, native boundary pictures,
waveform, audition, Enter commit and Escape restoration remain required. These
checks do not qualify physical display, device output, exported files, packaging
or a complete requirement/gate. DP-02/DP-05 remain partial.
