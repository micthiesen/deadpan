# Package head-storage correction and static review

Scratch only. No repository edits, Cargo, product command, network request or
actual retained project access. The original 1304f07218a07702763e9c45f34e67c67e8fff69f933266a8cf4003ea7d1ca7c
driver and previous raw-report binding preparation/review/handoff are preserved in
package-head-storage-before with pins in package-head-storage-before.json.

Current package-checks.py: 39432 bytes, SHA256
82ea0022ccee613672b911e6bafa402e274807946d9a0a1de485396e5eb225d7.

## Static finding and correction

The prior driver incorrectly treated revisions.document at state.head_revision
as the reconstructed Core document. Store revision_storage.rs explicitly uses
the exact JSON text "null" for ordinary edit and Undo/Redo revisions, with
periodic complete keyframes. validation::read_validated_bounded reconstructs
elided revisions through revision_storage::reconstruct.
The final atomic region:N Redo may legitimately be elided.

sqlite_snapshot now preserves raw authoritative head_revision, history cursor,
and head_storage (elided/keyframe, exact stored text SHA256/bytes, parsed stored
value). It admits the exact elision marker or an object keyframe; a keyframe's
revision must equal the head. It does not reimplement Rust patch reconstruction.
The full read-only transactional SQL dump, every table count, persistent project
file hashes and existing final/failure conservation checks remain unchanged.

Before proposal cases, raw SQLite head must match the pinned baseline revision.
The bundled CLI project dump uses ProjectStore::snapshot, which owns validated
reconstruction. Its complete document must exactly equal the actual final durable
document in the successful raw regions NativeReport, and its revision must equal
the raw SQLite head. The post-case CLI dump must equal this same document.
Exact SQL dump and persistent byte conservation cover both storage formats,
history, cursor and all operational rows.

## Remaining contract review

The actual failed regions report and current successful-run source contract agree
with the retained private Documents check label, expected/actual retention flags,
direct Documents/Deadpan package discovery and final durable check label.
The regions-only replay sets fixture_identity_file to canonical MP4 and pins its
bytes in fixture_sha256. The final durable check serializes the whole Core
ProjectDocument through ProjectStore::snapshot; its required fields exist.
The later audio conservation check does not edit the final document.
No second executable schema mismatch was found in this review.

The failed native-regions-first report remains unchanged and is never accepted as
a successful binding. No synthetic successful NativeReport wrapper was created.

## Verification and limits

Python syntax compiled. A retained Python helper over fabricated minimal SQLite
storage passed: exact elided head/cursor, complete keyframe, read-only database
bytes, changed storage detection, and three invalid marker/keyframe rejections.
See package-head-storage-helper-check.py and its stdout/stderr; this is helper
verification only, not product or retained-project qualification.

package-head-storage-review.diff retains the full patch.
package-head-storage-preparation.json and package-head-storage-source retain
hashes and exact source snapshots used for this static review.
The config schema/example did not change. Official bundle-verify remains skipped
for its unconditional AI-runtime checks. Actual packaging execution still needs
Root authorization and the actual successful release regions report/baselines.
