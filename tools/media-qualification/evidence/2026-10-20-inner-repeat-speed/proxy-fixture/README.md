# Proposed deterministic worker-write disk-full fixture

Status: proposed only, checkout untouched. No Cargo or Git executed.

Observed broad-run failure: phase1 expected ProxyBuildError::Conversion(ConversionError::Worker{..}), received ConversionError::Io(StorageFull), ENOSPC28. Existing error.is_disk_full/is_environmental properly classify either, but the phase-specific assertion is correct and must stay strict.

Call path: phase1 fills the private mounted APFS volume before encode(). encode_proxy_retrying invokes the factory first. stage_output calls ProxyCache::stage(), whose first allocating operation is mkdirat(.staging,random-private-dir); it then creates movie.mp4 via openat(CREATE|EXCL), opens descriptors and clones the movie. The full cache volume can reject either directory or movie metadata before a worker is launched. Neither API adds context distinguishing those two syscalls to this error, so the reported raw error does not prove which one failed. It does prove the asserted worker-report phase was not observed. The reserve file releases nominal48KiB but APFS metadata/freeing behavior means that is not a guaranteed staging budget.

Proposed repair: create the first complete (ProxyStaging,File) through the production stage_output before filling. The factory then consumes this pair exactly once. Any production-requested retry must call stage_output anew, retaining the fresh-output contract; no retry reuses a movie, directory or descriptor. Remaining phase2/phase3 calls passNone and preserve existing production staging behavior.

After the precreated pair is yielded, encode_proxy checks descriptor metadata and clones descriptors, and the worker receives that empty regular file as stdout. There are no additional host movie/directory allocations on this APFS volume before isolated-worker writes. Keep the existing Worker-only assertion, disk-full/environmental classification, no published/staged entries assertion, actual full-volume publication and read-marker phases, and recovery publication. A host staging failure or a different worker failure remains a failing test; no generic Io acceptance, fake ENOSPC or skipped real-volume case is introduced.

Files: proposed.patch and formatted full proposed proxy_disk_full.rs; source-sha256.txt pins checkout content used to construct the patch. Root should review and approve before this worker applies it, then run the isolated real test with actual media-worker. Expected first phase specifically Worker{code:disk_full}; phases2-4 and final cleanup/recovery unchanged. No runtime claim yet.
