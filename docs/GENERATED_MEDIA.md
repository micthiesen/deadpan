# Generated-object storage

`deadpan-store` owns a bounded, content-addressed byte store below each project's
`Media/Generated` directory. It is the filesystem boundary for future canonical
generated masters and their manifests. It does not decode media, choose a
candidate, register an asset, or change a Hold's provider.

## Identity and authority

`GeneratedObjectRef` contains a positive byte length and a `GeneratedContentId`
with explicit `algorithm: "blake3"` and a lowercase 256-bit digest. Deserialization
validates these fields and rejects unknown fields. The reference contains no
worker path, absolute project path, model location, or executable dependency.

The store derives the filename `Media/Generated/blake3-<digest>` itself. The
caller supplies the expected identity after canonical conversion and validation.
The store independently hashes the streamed bytes before publication. BLAKE3
addresses internal objects; the worker protocol's SHA-256 checksums continue to
verify worker declarations. Neither digest establishes that the bytes are valid
FFV1 or that a generation is acceptable.

`ProjectStore` pins the project directory when opened. Each media operation opens
`Media` and `Generated` relative to that descriptor without following symbolic
links. Media operations check directory ownership and device identity and reject
namespaces writable by a group or other users. Object files must be read-only,
regular, owned by the project owner, on the same device, and have one link.
Ordinary database inspection can open a package with missing media directories;
a media operation then reports the missing resource instead of inventing it.
These checks protect filesystem resolution and accidental mutation; they are
not a sandbox against hostile code already running as the same user.

## Publication and failure ordering

`promote_generated_object` requires the project's writable owner. It accepts a
reader, the expected reference, and a positive byte budget. The copy uses a
fixed-size buffer and stops on excess bytes, unexpected EOF, hash mismatch, or
I/O failure. A uniquely named temporary file remains unpublished while copying.

After verification, the store makes the file read-only, synchronizes it, and
publishes it with an exclusive atomic rename. An existing destination is never
overwritten. A matching verified object can be reused without consuming the
supplied reader; a corrupt or unsafe destination is an error. The containing
directories are synchronized before
success is returned. macOS additionally uses `F_FULLFSYNC` to request a storage
cache flush, including after the namespace synchronization.

Failures before publication attempt to remove only the operation's own temporary
file, after checking its directory-entry identity.
Failure after rename can leave an unreferenced object, and is still reported as
a failure. The authored acceptance transaction must run only after this
method returns success. If that SQLite transaction fails, the object remains
unreferenced. No existing media is deleted to roll back an authored transaction.

This module performs no eviction or abandoned-temporary cleanup. A later cleanup
policy must trace every retained revision and active reader, and apply a grace
period. Objects in this directory are never disposable merely because they are
absent from the current document.

## Readback

`snapshot_generated_object` is available from read-only stores. It verifies the
stored file's type, size, identity, and stable metadata while copying into an
anonymous temporary file. The returned `VerifiedGeneratedObject` implements
`Read` and `Seek`; subsequent mutation or replacement of the project file cannot
change its bytes. Reopening a changed project file requires fresh verification.

These are blocking host I/O operations. Keep them away from the UI and audio
callback. They bound copying by the caller's maximum byte count, not elapsed
time; scheduling and cancellation remain host responsibilities. The future app
host must also keep ordinary command commits from waiting behind media copying;
this synchronous API does not establish interactive responsiveness.

## Integration status

Database schema 9 and authored document schema 5 retain
[generated Hold intent](GENERATED_HOLDS.md). Publishing bytes
does not create a revision, history entry, candidate receipt, or asset reference.
The current API is implemented on macOS and Linux. [Native bundle qualification](GENERATION_BUNDLES.md)
now composes media conversion, provenance and verified Ready publication. The
dedicated [acceptance API](GENERATION_ACCEPTANCE.md) rechecks all six dependencies
and commits derived assets and the selected provider in one reversible transaction.
History reference tracking, qualified application acceptance, cache cleanup,
portable project copying, and application rendering remain open.

The integration tests cover writer ownership, read-only coexistence, retained
worker snapshots, deduplication, relocation, immutable readback, corruption,
symlinks, hard links, and package-path replacement. The developer example
`qualify_generated_storage` exercises publication and readback in separate
processes. Its reference JSON is a qualification artifact, not a second mutable
project document.

[The measured storage run](qualification/generated-storage-2026-09-21.md)
publishes the actual 30-frame sampled FFV1 master and its 25-frame native master,
removes private input copies, relocates the package, and verifies readback in
separate processes. The independent decoder then compares every RGB pixel and
the frame/color metadata against the retained fixtures.

The repository gate passes 256 Rust tests, including 12 storage unit tests and
8 storage integration tests, plus 72 Python harness tests. Formatting, Clippy
with warnings denied, the workspace build, and headless diagnostics pass.
No native app lifecycle or UI code changed, so native smoke and GUI checks were
not repeated. Linux behavior, physical power-loss recovery, and filesystem
disk-full injection remain unqualified by this run.
