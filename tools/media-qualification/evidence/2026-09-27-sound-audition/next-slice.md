# Read-only planning for authored sound placement

The migration worker mapped the next complete slice after catalog audition:
headless add/update/remove sound placement, reversible history, preservation through
existing structural edits, and ordinary Sequence playback. This is a proposal,
not implementation or qualification evidence.

Keep sound state separate from beat/provider vocabulary, keyed by owner and
durable logical continuity identity. Retain qualified SourceAudio, exact source
mapping, local placement, sample offset, edges/gain and SoundRoute. Sound length
must never extend picture duration. Treat placement as a timed basis edit.

Integrate command patches, Subtree copy, occurrence isolation, Split, delete,
insert, move, group/ungroup and Repeat/Retime overflow policy together. Transparent
Split fragments share processing continuity; later fragment-only edits require
isolation. Settle this edit scope and the durable identity for allowances on
implicit Repeat gaps before publishing a schema.

The anticipated boundary is core 28 to 29 and database 34 to 35, with a frozen
legacy_v28 adapter. FrozenAudioContext is currently schema 3. A new context version
must retain sound recipes/dependencies while old schemas reject even empty/null
new fields. Preserve an authentic DB34 fixture from the retained old CLI, including
history, abandoned branches and undo/redo. Never fabricate legacy JSON.

Compile owned voices in root, definition, processing-input, physical-domain,
bound and retained-context readers. Child sound enters enclosing time/pitch stages;
sound on the Retime itself enters afterward. Repeat-owned sound spans its output,
while child-owned sound repeats. Preserve the continuous Original voice and full
processing history. The borrowed AudioSignalMix/AudioStageProjection primitives
alone do not install these semantics in normal authored plans.

Resolve Hold suppression, explicit sound allowance and source exhaustion per
voice. Temporary mix indices are not durable policy identities. Process time/pitch,
owned edges and gain before ordered mixing and one final limiter. No-sound behavior
must remain bit-identical. Adding sounds after LimitedAudio would be incorrect.

Reuse existing revision-bound qualified assets and source admission. Include all
retained recipe dependencies, even when currently gated. Sequence playback should
hear normal planned events through LimitedAudio, while catalog Sound audition stays
independent. Playback's 16-source lifetime cache needs its own scheduling work;
the CLI LRU does not solve that boundary.

Candidate first semantics: natural-rate selection, arbitrary sample onset, scalar
gain/offset, explicit reject-overflow and a sound-specific Hold allowance. Tails,
pinned events and creative effects remain separate required extensions.

Coverage must include real 44.1/48 kHz PCM, independent sums, arbitrary onset,
exhaustion versus silence, parent/child Repeat/Retime ownership, all admitted edit
transforms, historical reopen, shuffled reads, stale receipts, resource limits,
and summation before one limiter. Native placement controls follow this shared path.
