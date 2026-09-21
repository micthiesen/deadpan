# Original media ownership qualification, 2026-09-21

The store now retains complete managed originals, records linked locations,
verifies relinking against complete content identity, and supplies private
snapshots. The shared CLI and native host's headless entrypoint expose these
operations. This preserves source bytes independently of editorial registration;
it does not complete project import or qualify audio for playback.

The implementation starts from `c224c2c3d2dfd3decad86d279c72684c66962b82`.
[Recorded evidence](../../tools/media-qualification/evidence/2026-09-21-original-media/)
contains the changed-source manifest, required repository gate, native-adapter
sanitizer run, actual headless workflow, migration fixture provenance and review
regressions. The host is Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1, using
the previously qualified LGPL FFmpeg 8.0.3 prefix. Command durations are smoke
observations; power, cache state and competing load were not controlled.

## Ownership and persistence

[Original storage](../ORIGINAL_MEDIA.md) hashes the complete file with BLAKE3 and
SHA-256. Managed retention tries APFS descriptor cloning, falling back to a
verified positional copy when unsupported. Original and generated objects share
the same descriptor-relative verification, exclusive publication and durability
engine. Existing objects must verify before deduplication. A database failure
after publication retains the object for retry.

The actual headless workflow uses the committed H.264/B-frame plus AAC fixture.
It observed `cloned` on first managed retention and `existing` on retry, rejected
wrong content and stale relinking, removed the external source, relocated the
package, then verified every byte through `deadpan-app --headless`. Original AAC
packets and container metadata remained byte-identical. The owned file was
read-only and the authored document/history stayed unchanged. No native window
was opened for these operations.

Store tests cover independent snapshots, same-name/different-content originals,
bounded inventory, writer ownership, cancellation, byte limits, directories,
symlinks/FIFOs, database-trigger failure, tampered bytes/records and relocation.
Shared-engine tests force the copy fallback across multiple chunks and verify
cursor independence, source-change rejection and competing-object preservation.
Native fileclone tests exercise actual copy-on-write independence, exclusive
creation and existing file/symlink preservation. No external filesystem's clone
support was inferred from the forced-fallback test.

Database schema 10 adds operational original records and location versions.
Schema-1-through-9 migrations retain their complete authored and generation
chronologies. The new schema-9 fixture comes from the existing real six-object
acceptance qualification project, validated by a saved `c224c2c` CLI binary and
exported through SQLite's backup API. Tests preserve its admission receipt and
five revisions and reject legacy databases carrying an unexpected modern
original-media table. The fixture and validating binary hashes are recorded.

## Review findings and verification

Independent general, storage and native reviews covered all changed source,
including the shared storage extraction and new crate. Two storage regressions
were fixed and tested against isolated copies of the earlier behavior:

- A rejected cloned entry could survive validation before its cleanup guard was
  installed. The guard now captures the pending entry before opening/validation,
  checks the opened inode, and removes only the matching pending name. The test
  uses an actual APFS clone with an unexpected second link and preserves that
  other name. Unidentifiable entries are deliberately retained on inspection
  failure; same-user hostile namespace manipulation is outside this boundary.
- Linked retention/relinking checked only inode identity after hashing. They now
  compare final named/held metadata with the post-hash state, detecting same-inode,
  same-length modifications in that interval. Future reads still reverify because
  an external file can change after a successful location check.

Original failures use original-media protocol codes; shared display messages use
neutral media wording. The native audio inventory reports observed stream clocks,
codec, start/duration and sample-rate/channel metadata. Review questioned implicit
AAC probing. The pinned FFmpeg source propagates the existing `h264,ffv1` allowlist
to its decoder-opening paths, and actual test logs show AAC rejected before
initialization. No audio samples or exact decoded audio bounds are claimed.

The prior source-preview CI run
[failed in process-group cleanup](https://github.com/micthiesen/deadpan/actions/runs/35614496932).
Its boolean assertion discarded the precise error, so the original cause remains
unproven. Cleanup now gives Darwin's partially signalled/exiting groups a bounded
grace period, retains the unreaped leader, and preserves deadline/cancellation
classification. The expanded inherited-pipe regression launches 32 descendants
and checks that none writes its delayed marker. A green local run does not
retroactively make that earlier CI run successful.

The required fmt, Clippy, workspace test/build and doctor commands, all four
Python qualification suites, and native source/media sanitizer checks are
recorded in the evidence directory with exact counts and logs. Sanitizers cover
the selected C adapters and target C dependencies; Rust and FFmpeg libraries are
not instrumented. Initial compiler, formatting, migration-prefix and regression
failures remain recorded. The original-byte operations do not change app layout,
keyboard routing or startup, so native GUI checks were not repeated. Existing
[visual and keyboard observations](source-preview-2026-09-21.md) remain limited
to the source-preview interface.

## Remaining work

All product requirements and gates remain open or partial. Authored import still
needs qualified selected-stream receipts, exact audio indexing, presentation-basis
selection, atomic registration/insertion, native dialogs, progress/retry/relink
UI and bookmark resolution. Reference collection, portable-copy workflow,
recovery UI, full format/color coverage, playback/export and distribution also
remain outstanding. No original eviction or automatic deletion was introduced.
