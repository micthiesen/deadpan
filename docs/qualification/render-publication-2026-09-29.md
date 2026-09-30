# Verified destination publication, 2026-09-29

The [publication host](../RENDER_PUBLICATION.md) now takes a private verified
candidate and an explicit MP4 destination. It stages a bounded sibling partial,
checks exact destination bytes, publishes historical provenance, then atomically
renames the movie without replacing an existing entry. Both published files are
rehashed before success. The movie and report have separate commit points.

## Actual published media

Measured on Apple M5 Max with 128 GiB RAM, macOS 26.5.2 (25F84), SDK 26.5,
Apple Clang 21.0.0 and pinned Rust 1.97.1/FFmpeg 8.0.3. The FFmpeg development
prefix retains the existing LGPL configuration with networking disabled.

All seven fresh project encodes passed isolated file verification and production
destination publication. The independent reader then inspected their final names,
comparing complete decoded pictures and authored PCM with direct committed inputs.

| Case | Pictures | Authored sample frames | GOPs |
| --- | ---: | ---: | ---: |
| Structural edit | 108 | 172,973 | 7 |
| Nonzero one-frame range | 1 | 1,601 | 1 |
| Explicit software encoder | 43 | 68,869 | 5 |
| Odd authored canvas | 1 | 1,602 | 1 |
| 60 fps markers | 120 | 96,000 | 4 |
| Accepted Generated provider | 30 | 48,048 | 2 |
| Encode after cancellation | 1 | 1,602 | 1 |
| **Total** | **304** | **390,695** | **21** |

All 912 planes, comprising 116,984,106 component codes, passed the established
lossy-stage bounds. Maximum component error was 43 codes; maximum per-plane mean
absolute error was 0.824 codes. Complete fixed-coordinate PCM comparisons passed
through manual FFmpeg, ordinary FFmpeg and AVFoundation: maximum error 0.106 and
maximum RMS error 0.000981. Both channels' events at samples 100, 48,000 and
95,800 were exact in all three readers, yielding 18 observations with zero sample
error. No event-based alignment or AAC-block tolerance was used.

A separate Python audit reopened all final movies and reports. Their sizes and
SHA-256 identities matched the receipts; all were regular files with mode 0600
and one hard link. Every report matched its encoded manifest, verifier result,
historical SQLite document hash and captured range. Original receipt references matched
retained database rows and actual Original SHA-256. The Generated case recorded
exactly `[0, 30)` and honestly retained its lack of Original qualifications. The
other cases each reported one qualified catalog entry. All seven reported
explicit engineering encoder selection. No successful attempt left a partial.

The full Metal/worker/encode/reference/publication run took 429.00 seconds, and
the independent reader run took 8.39 seconds. These individual development runs
are not performance acceptance results. Existing hardware B-frame limitations
and automatic encoder policy work remain unchanged.

## Regression coverage

The 26 focused tests pass: 15 filesystem tests, five provenance tests, two host
tests and four native-candidate integration tests. They cover exclusive creation,
existing and racing destinations, directory/symlink/ancestor replacement,
descriptor identity, owner/mode/link-count changes, bounded reads and writes,
cancellation, deadlines and injected synchronization failures on both sides of
the movie rename. No failure path deletes a retained or foreign entry.

Historical capture tests check the complete document hash after live edits,
receipt bindings and private-label omission. Effective Generated intervals cover
sparse Repeat plays, gap overrides, Retime, blank seams and complete artifact
identity. A huge compact Repeat uses only the selected output interval; explicit
catalog/artifact/interval/report limits reject excess work without truncation.

The native-candidate tests reuse actual retained MP4 bytes through a test-only
encoder transport fixture. Their background document does not claim picture
fidelity. They establish candidate ownership, destination identity, report
binding, project immutability, retry, cancellation at every exposed precommit
stage, and changed movie/report rejection. Fresh real project rendering and
independent readers provide separate content evidence.

## Review and retained failures

Independent source review found two issues, both corrected:

- Cancellation/deadline errors lost their stable codes through provenance and
  controlled I/O. The host now preserves typed diagnostics across these layers.
- Rename legitimately changes ctime. A same-length write during post-rename
  synchronization, followed by restoration of mtime, could pass metadata checks.
  The host now compares actual published bytes with their trusted hashes. A
  deterministic regression recreates the mutation and requires
  `published_hash_mismatch`. The final movie remains present and is reported as
  `PublishedUnconfirmed` if integrity or durability cannot be established.

The first focused build caught a diagnostic fallback lifetime mismatch; its
compiler log is retained alongside the correction and passing run. No lint
suppression was added. Source review of historical provenance found no
actionable issue.

## Final checks and retained evidence

- `cargo test --workspace --locked`: 2,248 passed, zero failed or ignored,
  including the compile-fail documentation test.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.

The [evidence package](../../tools/media-qualification/evidence/2026-09-29-render-publication/README.md)
retains actual candidate and published MP4s, local reports, direct I420/PCM
references, independent reader outputs and SQLite backup-API project snapshots.
It also retains command journals, initial compiler failure, source/binary hashes,
review findings and final dispositions. Every archive member is read back and
checked against its recorded size and SHA-256.

Focused tests, native build/run, independent readers, publication audit, workspace
tests, Clippy and formatting share source inventory
`8332c9d07a73d66364e6ff1468399a369355cefedcb7b68de5d91db4231c790d`.
Only documentation and archived evidence changed afterward.

## Scope

Verification is cooperative around synchronous filesystem and database calls.
After the movie rename, required durability work ignores late user cancellation;
final readback has a separate shared ten-minute deadline and exact byte bounds.
The sidecar records evidence prepared before movie publication. Its presence
alone does not establish that the movie committed or passed its final checks.

This increment adds Rust host/filesystem code and qualification calls, with no
native C decoder or encoder change. Native C sanitizers were run at the preceding
[verifier checkpoint](finished-file-verification-2026-09-29.md); they were not
rerun for this publication boundary. Cross-filesystem destinations are supported
by destination-side staging, but removable/network filesystems and crash/power
loss have not been qualified. Linux requires separate platform qualification.

Durable render jobs and recovery, native Render, public headless render commands,
automatic platform policy, the generated-footage disclosure reminder, complete
mastering/effects, HDR, full-size capacity, performance and release coverage remain
required. Every DP requirement and Gate A through G remains open or partial.
No native UI behavior changed.
