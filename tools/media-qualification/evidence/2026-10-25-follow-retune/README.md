# Follow retune qualification evidence (2026-10-25)

This immutable evidence bundle preserves the Follow retune qualification artifacts and generated counter-footage results. It contains harness source and file hash maps, all recorded run receipts and raw stdout/stderr (including failed and incomplete attempts), replay reports, selected final focus/minimum-size screenshots, and generated fixture projects, source footage, exported MP4s, PCM, pose and verifier records.

The 120 Hz replay is a behavioral visual replay against an explicitly supplied local Kestrel `Shortcuts.swift` path. Its debug run is not a performance measurement. The captured media runs use generated counter footage. Media verifiers report 120 decoded pictures and 192,192 presented samples for each exported movie; this is decoder/output verification evidence, not independent DSP or tracker-accuracy evidence and not listening qualification.

All JSON, logs and HTML are gzip-compressed losslessly; inventory records both original and stored byte counts and SHA-256 digests. Selected screenshots are retained as PNG. Every copied file was reopened and validated against the source bytes; compressed files were decompressed for source-equivalence validation.

Build caches, executables, full replay frame sets, and source archives are excluded. Lock files and SQLite SHM files are omitted; SQLite WAL files are retained only when nonempty. The complete omission ledger, including size and SHA-256 for each omitted lock/SHM/empty-WAL sidecar, is in `omissions.json`.
