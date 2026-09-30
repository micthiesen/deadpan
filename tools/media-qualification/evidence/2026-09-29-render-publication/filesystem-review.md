# Publication filesystem review

Source-only independent review of `publication/filesystem.rs`, `filesystem/tests.rs`, and the host call sites that map errors and confirm published files. No source edits, builds, tests, formatting, native execution, or commits were performed by this reviewer. The parent owns runtime checks.

## P2 resolved in source: A same-length mutation can be accepted when ctime is reset after rename

At `filesystem.rs:520-530`, the first post-rename check deliberately compares `same_state(..., false)`, ignoring ctime because rename may change it. It then replaces the trusted baseline with the current stat. The comparison checks metadata, not content, despite the adjacent comment saying content must match.

A deterministic failure witness fits the existing `commit_with_sync` hook:

1. Write, seal and read back a complete partial; save its exact modification time.
2. During `SyncPhase::PublishedFile` or `AfterDirectory`, overwrite its descriptor with different bytes of the same length.
3. Restore the saved modification time with `File::set_modified`.
4. Let commit finish.

Device/inode, owner, mode, length, links and mtime still match. The mutation's ctime is ignored and then adopted as the new baseline. Both remaining confirmations pass, and the current host can return `Published` for content that differs from its pre-rename SHA-256 readback. The same gap applies to report publication, so a report can change before the movie commit while its later metadata confirmations succeed.

Require content identity across the rename boundary, for example by hashing the published retained descriptor against the already captured digest under a stable post-rename metadata observation. A mismatch or inability to complete that check must retain the fact that rename happened and yield a published-but-unconfirmed outcome. Add the same-length/mtime-restoration witness to the existing fault-injection tests. Current tests cover changed size/mtime and replacement identity, but not this reset window.

Disposition: independently inspected the implemented fix. `published_reader` retains the bounded reader and stable descriptor/name checks, rejects an unpublished file, and preserves the published flag on control/readback errors. The host compares the report digest after its rename before movie publication. Following the movie commit, `finish_committed` compares both movie and report digests against their trusted pre-rename identities, then confirms both entries again. That work shares a separate ten-minute deadline and fresh false cancellation token, with captured byte extents; every failure returns `PublishedUnconfirmed` rather than ordinary failure or success. The deterministic regression mutates the same-length payload during `PublishedFile`, restores the old mtime, confirms that metadata checks alone succeed, and requires the real host helper to return `published_hash_mismatch`. No actionable issue remains from this finding. Parent execution of the new checks remains pending; this reviewer did not run them.

## Positive source observations

- Final publication uses descriptor-relative `renameat_with(..., RenameFlags::NOREPLACE)`. The preflight absence checks are supplemented by the atomic no-replace primitive, so an entry created before the rename is preserved. Existing regular files, directories and dangling symlinks are rejected.
- Movie and report destinations share the same `Arc<Directory>` and descriptor. The pin retains both selected-path identities and a canonical directory-chain identity. It rechecks ancestors, aliases and the directory descriptor around creation, sealing and commit. Moving an ancestor and restoring only the leaf inode is covered by a focused test.
- Partials use exclusive creation, no-follow, nonblocking and close-on-exec flags, then exact owner read/write permissions. File validation requires a bounded regular singleton owned by the current effective user on the pinned filesystem. No writable descriptor escapes the layer.
- Reads and writes are limited to 64 KiB per call and a captured maximum of 64 GiB. Writer offsets are owned by the layer; sealing revokes writer access. Reader offsets stay within the sealed count and check retained descriptor and named-entry metadata before and after each nonempty read. A write failure poisons the writer or leaves a state mismatch that prevents admission.
- There is no unlinking destructor or cleanup branch. Incomplete partials, complete failed partials and foreign replacements remain on disk. Recovery paths are documented as observations that require fresh validation before later use.
- Successful rename sets `published` immediately. Every subsequent commit error is marked published, and late cancellation is intentionally ignored while required synchronization proceeds. Tests inject failures at every synchronization phase and distinguish both sides of the commit point. Pre-rename cancellation/deadline errors retain the partial; cancellation after rename does not undo publication.
- Host report confirmation now occurs before the movie commit and after a successful movie commit. Failure of the latter becomes `PublishedUnconfirmed`. A report that was committed before a pre-movie failure is recorded separately from partial paths.
- The prior host diagnostic finding is resolved in source: provenance errors expose their own stable codes, controlled I/O retains a typed `FsError` inside `io::Error`, and host normalization preserves cancellation/deadline codes for candidate copy, report write and readback errors.

## Scope limits

This review establishes source behavior only. It does not claim the real filesystem, injected failures, directory-replacement tests, or native publication integration have passed. The review does not extend to the separate provenance implementation. Files can always be modified after the last observation; the finding above is specifically an undetected mutation before the implementation's own final integrity checks complete.
