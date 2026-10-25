# Native review follow-up

Native review edits are frozen. No Cargo/native runtime ran during this follow-up;
Root reserved verification until integrating the shared protocol fixes.
Scoped rustfmt --check passed for the3 changed Rust files.

Only these paths changed:
- native/deadpan-track/src/vision.rs: remove the2 duplicate proposal-engine
  string constants. Tracking/landmark engine constants remain unchanged.
- native/deadpan-track/src/picture_proposals.rs: consume canonical
  deadpan_jobs::picture_proposals::{FACE_ENGINE,SALIENCY_ENGINE}; add1 focused
  geometry regression covering partial overflow/all rotations/exact edges,
  wholly outside empty results, and invalid numeric/size errors.
- native/deadpan-track/tests/picture_proposals.rs: require exact canonical engine
  equality for all real detector requests instead of substring matching.

Shared constants are now present under the agreed names and exact previous
strings, owned by single_leaf_net_impl. No native literals remain duplicated.
No production geometry/face/cancellation/decoding policy changed.

Bounds rejection finding was refuted before implementation by authoritative
VNRectangleObservation SDK documentation. Root explicitly kept intentional
clipping. See native-review-triage.md and the exact SDK header snapshot/hash.
The new test protects that documented policy instead of treating legitimate
outside coordinates as malformed.

Original review sources: native-review-before, native-review-before.json.
Final sources: native-review-final-source.
Complete reviewed diff: native-review-final.diff.
New freeze: native-review-freeze.json SHA256
39f56aa50084c2436fae1d48e3fcf426fb63a24b0d12b3bdfc9e3555331f97bb.
Previous native-handoff.md/native-final-freeze.json remain truthful records of
the earlier fully tested source set, and were not overwritten.

Owed after Root integrates fixes:
cargo test --locked --package deadpan-track -- --nocapture --test-threads=1
cargo clippy --locked --package deadpan-track --all-targets -- -D warnings
with the allocated shared target and qualified FFmpeg8 prefix. Expected package
count is36 after the new unit;4 real proposal integration tests are unchanged
apart from stricter exact engine equality. Root owns final verification receipts,
release/UI/packaging and progress artifact.
