# Native limiter evidence retention review

Review scope: `tools/audio-limiter-qualification/evidence/2026-09-24-native/{README.md,retain.py,verify.py}` and their stated joins. Read-only review; no repository changes made.

No remaining concrete findings. The collector constrains paths to normalized relative names, rejects symlinks and duplicate entries, hashes captured content, bounds total bytes, and writes deterministic regular-file archive entries without overwriting an existing seal. The verifier reads the archive without extracting it, bounds member count and expanded size, rejects unsafe/nonregular entries and metadata, and checks the exact manifest file set and per-file hashes.

The verifier's cross-report assertions now justify the published joins: exact candidate/native finite outputs and gains, 14 unity-gain cases, cold/shuffled parity, six sinc witness coordinates/channels, baseline and cached provider-call counts, and the aggregate before/after and artifact counts. The qualified WAV grammar now excludes source fixtures while their hashes remain checked. The compiler correction is supported by the retained build metadata and binary-embedded Rust compiler paths: the original build used 1.98.0, while the pinned 1.97.1 rebuild and pipeline metadata identify 1.97.1; native PCM and gain outputs match across builds.

`retain.py` can leave an archive without a manifest if a post-write source recheck or manifest write fails, and refuses to overwrite that orphan on retry. The README now documents this as a failed seal and directs a retry into a new scratch directory while preserving the failed attempt. This is a documented operational limitation, not a remaining finding.

The owner reports the latest `verify.py` run passed; I did not rerun it. This review is an attestation based on the retained files and that reported result, not raw verifier stdout captured by me.
