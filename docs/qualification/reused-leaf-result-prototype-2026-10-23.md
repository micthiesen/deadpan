# Reused leaf result prototype (2026-10-23)

## Result

A private release prototype replaced the two-step leaf application in `pause.rs` (`apply`, then `forward.apply`) with the existing `apply_with_result` call and used its returned document. No other production file or schema changed in this prototype. Charge accounting, current-document assignment, step recording, and child refresh order are unchanged.

Five alternating pairs of fresh 10,000-Hold packages passed the seven large-pause checks. The prototype median was 266.235 ms from input to commit and 268.738 ms through offscreen composition. The control medians were 270.122 ms and 272.685 ms. In this batch the differences were 3.887 ms (1.44%) and 3.947 ms (1.45%), respectively. These ten runs are diagnostic samples; they do not establish a percentile, target pass, or product latency guarantee.

This is a separate batch from the qualified shared-preparation comparison,
whose candidate composition median was 275.697709 ms. Each comparison keeps
its own control; the batches are not pooled. The private prototype app hash is
`460aa0fb4ecccd06cc24fb0046d766f0df7f4857ebbb634120f17c2558266e8a`;
the qualified control is
`69978abd09ffc52dcd281066bc615d7b3e1d5cb11653f5d888d0738015ba1496`.

## Build and replay evidence

The release build and `cargo fmt --all` passed on an isolated source copy based on commit `1e67fdb27bbb3a59d282e5240c65b860b4090b76`. The source-basis record identifies exactly one changed production file, `crates/deadpan-core/src/semantic/planner/pause.rs`, with before SHA-256 `9462060e367d8ca16ebbb415291db2129f4af83b165b8ab0421f0568ac05d749` and prototype SHA-256 `8b6e2f282b5d704afe6528e5dbdad5118028bb99d637fe0644410bb514fde15f`.

All ten fresh-package runs passed seven checks each. Source, binary and
authored-input guards passed, and every WAL remained empty. Each report also
retains Kestrel, two layout/retry warnings and three skipped-scope notes. These
scope exclusions are not skipped unit tests or hidden failures.

A read-only review before measurement found no concern with reusing the validated result or preserving the charge and mutation order. This review is separate from build and replay verification. A differential Core test and full production qualification remain a separate follow-up. The production candidate that root is preparing includes the same one-line change plus that differential test; this prototype record does not qualify that candidate.

The source identity map is named `instrumented-source-identities.json` by the reused build wrapper. It is byte-for-byte identical to `build-source-identities.json`; the prototype contains no timing instrumentation. The uncompiled test draft in separate scratch is not included and is not reported as passed. No physical-display, source-decoding, or PCM performance claim is made.

## Evidence

The bounded package is [2026-10-23-reused-leaf-result-prototype](../../tools/media-qualification/evidence/2026-10-23-reused-leaf-result-prototype/). It retains the original and prototype `pause.rs` snapshots, the exact formatted release source, one-site patch, source and binary manifests, build receipts, commands, input maps, compressed reports and logs, parser/audit scripts, and the static review. Executable binaries, Cargo targets, generated project packages, and the large source archive are excluded.

The independently re-read inventory contains 118 files totaling 1,356,170 bytes. `inventory.json` SHA-256 is `b85c335624ba10b58cb86f7855d771c93fe8e00c0485e095e23dfa4f9c7e2e46`.
