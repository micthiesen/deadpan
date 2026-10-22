# Independent source and evidence audit

Audited read-only against commit 32598481ba9e66a0a37c79df3967d5e3fd170ebe. No repository files, Cargo commands, formatter, or Git metadata were written.

The release-final source manifest is internally reproducible from its retained tar: 1,990 paths and hashes match. It does not bind the requested commit: that commit has 2,000 Rust paths. The ten unmanifested paths are the new range_sets/range_set modules and tests. Thirty-three manifest-listed files also have different content at the requested commit. The manifest therefore reflects earlier source, not the target commit.

The six release-final input hashes match four of six files at the target commit. baseline-routing.txt and docs/COMMANDS.md changed to document the ranges selector. release-final-lock.json has no mismatches for the four unchanged inputs.

Retained run evidence does not support the requested broad/UI counts: broad-fourth.log.gz reports 4,570 passed and 781 skipped, while broad-final-build.log.gz records a compile failure in deadpan-core tests. ui-final.log.gz reports 1,269 passed, 11 skipped; ui-complete-final-result.json reports 1,273 passed. I found no retained result/log for 4,596 or 1,281.

Native visual and performance receipts both report no failed scenario and include the Kestrel shortcut scenario (one check each). The selector scenario reports 143 checks (visual: 546 frames; performance: 677 frames), with no failed scenario. Finding arrays contain warnings/diagnostic findings, so “no failures” is the supported claim.

The retained media receipt passes with 32 pictures and audio range [0, 51251), offset 0, verified_zero, no flags and no failures. This differs from the requested 30-picture [0,48048) claim.

The owner fixture archive has nine safe relative members; normalized member paths and SHA-256 values match its receipt. validate and doctor both exited 0. The evidence inventory I inspected had 322 entries; all listed byte counts and SHA-256 values matched. Its inventory SHA-256 was 52726858f0b5bff26726264a9e9a88af0a3b68e39c68a4a1d38fb2.

The audit is partial because the retained release manifest and lock do not bind the requested commit. The inventory digest covers only the 322 entries observed at audit time and does not include this /tmp receipt.
