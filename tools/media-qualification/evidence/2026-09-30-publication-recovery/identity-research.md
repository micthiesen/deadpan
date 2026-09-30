# Restart-persistent publication identity research

## Recommendation

For the current APFS-only durable recovery path, retain a strict typed identity from the open descriptor:

`volume_uuid[16] + st_dev + st_ino + st_birthtimespec(sec,nsec) + optional st_gen`

and pair it with current metadata (owner, raw mode, link count, exact size, mtime, ctime) plus the host's full expected byte length and SHA-256. Require APFS and a real nonzero birthtime. Treat `st_gen == 0` as unavailable, not a failure. Keep ordinary publish available on other filesystems, but do not reopen/adopt after restart without qualified identity.

The tuple is a strong check against ordinary inode reuse and byte-identical replacement, not cryptographic proof against a malicious same-user process. The host hash confirms bytes, while object identity distinguishes an old same-byte object from a replacement. Same-user metadata forgery, deliberate lock bypass, or a filesystem violating its reported identity remains outside the guarantee.

## What the platform exposes

- Rust's Darwin `MetadataExt` provides descriptor metadata including `st_birthtime`, `st_birthtime_nsec`, and `st_gen`; `File::metadata()` obtains it from the open descriptor. This avoids adding unsafe code for the per-file stat fields. Apple documents `st_birthtimespec` and the stat fields in its [stat(2) manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/stat.2.html), and current Rust exposes the Darwin extension at [std::os::darwin::fs::MetadataExt](https://doc.rust-lang.org/std/os/darwin/fs/trait.MetadataExt.html).
- `st_birthtimespec` is creation time. Apple's stat manual warns that filesystems without birthtime may return ctime in its place, so recovery must reject unavailable/fallback birthtime rather than record it as identity.
- `st_gen` is a file generation field but Apple's stat manual says it is only available to superuser. The parent-owned APFS probe returned `st_gen: 0`. Apple's newer `ATTR_CMN_GEN_COUNT` is a different field: it tracks content modifications, needs `FSOPT_ATTR_CMN_EXTENDED`, and zero is invalid (including mmap cases). Do not use it as object identity.
- `ATTR_CMN_FILEID` is equivalent to `st_ino` and unique only within a mounted volume. Apple's `ATTR_CMN_OBJPERMANENTID` is documented as a persistent per-volume object identifier across mount/unmount when supported. It may be a useful optional extra if the implementer can qualify and safely parse it, but it is not required for this minimal increment and must not be conflated with `st_gen`.
- Apple documents `fgetattrlist(fd, ...)` as descriptor based and `ATTR_VOL_UUID` as the filesystem UUID. A narrow adapter around `fgetattrlist` (and descriptor `fstatfs` for filesystem type) is justified: std/rustix metadata lacks the volume UUID field, and resolving a path separately would reintroduce a substitution race. Keep all pointer parsing and unsafe calls inside one documented adapter. Apple's [current XNU getattrlist(2) manual](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlist.2) describes `fgetattrlist`, returned-attribute checks, volume attributes, `ATTR_VOL_UUID`, persistent object IDs, and the generation-count distinction.
- Apple Foundation also exposes `volumeUUIDStringKey` (persistent volume UUID or nil) and `fileResourceIdentifier`, but Apple explicitly says `fileResourceIdentifier` is not persistent across system restart. These URL resource values are not a simple safe Rust descriptor API, so they do not replace the narrow fd-bound adapter here. See [URLResourceKey](https://developer.apple.com/documentation/foundation/urlresourcekey) and [fileResourceIdentifier](https://developer.apple.com/documentation/foundation/urlresourcevalues/fileresourceidentifier).

## Local APFS probe

Parent-owned probe output: `/tmp/deadpan-publication-recovery-ams9kpds/python-identity-probe.json`.

It reports the current macOS temp filesystem as APFS by parent qualification. The created file had device `16777233`, inode `33781650`, nonzero birthtime `1790751229.016415`, and `st_gen: 0`. Rename retained device/inode/birthtime while ctime changed. Replacing it with an identical-byte file and restoring mtime produced inode `33781651` and a different birthtime. `/`, `/tmp`, `/private`, `/private/tmp`, and the probe directory all exposed nonzero birthtimes; `/tmp` is a symlink entry and the current destination code records selected and canonical chains separately.

This is a focused fixture, not proof of all APFS configurations. Keep recovery gated on each required field being present and the descriptor's filesystem being APFS.

## Path and file checks to retain

Current `filesystem.rs` already pins the selected path entries with `symlink_metadata`, resolves/captures its canonical path, opens the canonical directory chain with descriptor-relative `NOFOLLOW|DIRECTORY`, validates owner/mode/device, checks file entries through `statat(..., SYMLINK_NOFOLLOW)`, requires regular single-link files, and compares size/mtime/ctime around bounded reads. Durable evidence should preserve selected and canonical chains, including lstat identity for symlink components, and the destination directory's volume UUID. Reopen the path chain from scratch after restart and require exact evidence before opening content.

For movie/report files, open only the recorded basename relative to the revalidated directory using `RDONLY|NOFOLLOW|NONBLOCK|CLOEXEC`. Compare the open descriptor with the no-follow named entry before reading. A stored pre-rename ctime cannot be required to match after rename; compare the persistent object tuple and stable sealed fields once while reopening. Then make the actual reopened state (including ctime) the baseline for each read chunk. The host, not the filesystem module, hashes the entire content. No recovered-file API should provide rename, unlink, chmod, or truncate.

`File::try_lock` held on both files makes an old still-live publisher visible to recovery: an open file description keeps the advisory lock through rename, and process exit releases it. Acquire the pair in a fixed order and fail immediately on contention; do not add a persistent lockfile or PID protocol. This coordinates participating Deadpan workers, not malicious same-user code, because filesystem advisory locks can be ignored.

## Conservative fallback and guarantee boundary

- Existing `Destination::pin` and standalone publication should continue using their current platform behavior.
- Only durable evidence capture/reopen requires APFS, volume UUID, and non-fallback birthtime. If any are missing, record reconciliation as unavailable and leave artifacts unresolved; do not fail ordinary publication or infer ownership from equal bytes.
- Every evidence record needs an explicit schema version, `deny_unknown_fields`, and bounds matching the existing 4096-byte path / 256-component caps. Encode Unix paths and names as raw bytes so non-UTF-8 paths survive serialization without normalization.
- File evidence should include volume UUID, device, inode, birth sec/nsec, optional nonzero `st_gen`, uid, mode, nlink, exact byte length, mtime and captured ctime. Compare ctime only after reopen against a newly sampled current baseline. Keep expected digest and length in the typed host/store contract; the store should preserve filesystem evidence as bounded opaque versioned bytes.
- No stat tuple or hash can authenticate against an adversarial same-user process that changes metadata or deliberately bypasses the advisory lock. The contract is cooperative crash recovery with fail-closed replacement detection.

## Source references inspected

- `crates/deadpan-cli/src/encoded_render/publication/filesystem.rs`: `Destination::pin`, `Directory::confirm`, `create_partial`, `PartialFile::commit_inner`, `PartialFile::confirm`, `validate_file`, `same_state`, `path_entries`, `open_directory_chain`, and bounded `PartialReader`.
- `crates/deadpan-cli/src/encoded_render/publication/mod.rs`: report-first then movie rename, `finish_committed`, and post-rename host hashing.
- `crates/deadpan-store/src/object_storage.rs`: `File::try_lock` and named inode confirmation for retained namespace lock, lines around 1008-1060.
