# Production evidence collector draft review

Status: findings. Static review only, before any proposed manifest exists. No generator/copier functions, application, build, measurement, media command, Git or Main writes were executed. This is not final manifest approval.

Reviewed stable bytes:
- package.py: 9a3331d2280bcd14d347afffd3f852312bc998ba8f159bb0e41bff8a35c12c38
- prepare_manifest.py: aad815c0a137787bf8c25ce9c4e88800964c675622a7c9fc9b834021bc6b9633
- static-packaging-plan.md: 8e32032f8c6f40a163ccd28140e2897d6a024bce56f1bf4b3a221387de8bd426

The independent analysis-review.json gate and removal of the duplicate static plan payload are correctly applied. Preserve their existing history.

## 1. Retain the actual correction snapshots and draft history

prepare_manifest.py:17 and :50–63 skip top-level .txt and do not enumerate the packaging-draft history files. They therefore omit the existing analyze-before-root-review.py.txt and analyze-before-schema-fix.py.txt, as well as package-before-review-fix.py.txt, prepare_manifest-before-review-fix.py.txt and packaging-draft-history.json. The notes that explain corrections would survive while their exact referenced bytes disappear.

Add these exact known history artifacts as explicitly named payloads. Keep future review/fix snapshots explicit too; do not recurse over source copies, target directories or binaries. Record these as pre-execution static corrections, never runtime failures. The old discarded source-copy attempt can remain represented by its existing pinned maps/provenance unless Root separately requires specific bytes.

## 2. Complete the identity and approval bindings advertised by the plan

package.py:149–178 verifies candidate metadata but never hashes the executed candidate, control or qualified helper binaries. control_app_sha256 is generated but not consumed. No helper manifest/digest is bound in the proposed manifest. source-review.json is hashed but its no_findings status and candidate map binding are not checked.

The source_changes rows are compared to each other and payload rows, but their before_sha256/after_sha256 are not compared to base[path]/candidate[path]. The live map walk is useful; also assert exact equal 2053-path sets and the two declared changed paths, and compare all six inputs with the qualified baseline root, not only the candidate.

Add narrow identity checks, not another timing evaluator: pin the binary manifest, baseline and qualified helper manifest; hash each actual executed binary and verify those immutable identities. Require source-review status/findings/map, and bind each retained source snapshot to its actual map entry. The independently audited analysis can remain responsible for metric/test/media semantics.

At final publication package.py:215–219 rechecks plan/manifest/script/review but does not recheck PACKAGING-GO.json against the frozen envelope. Add one loop over every envelope source, including GO, and require unchanged bytes immediately before exclusive rename. Do not publish a stale approval after the current GO bytes change.

## 3. Bind the complete media ledger, including the actual two export files

package.py:170–173 compares manifest.media_files to the media inventory only by relative path and digest. It never proves those rows have corresponding payloads. Omitting a movie payload while leaving its media_files descriptor would pass this check.

For every exact inventory file, require its destination in the payload ledger with equal bytes, hash and intended source path, and compare the actual complete regular-file path set against the inventory. Require the two cached/uncached export payloads to match media-terminal movie hashes. Keep all 35 actual files, including original/render-candidate/package/PCM/history/verification bytes, rather than substituting the descriptor list for retention.

## 4. Restore payload and total-package bounds

The only declared bound is MAX_FIXTURE_BYTES. verify_payload reads an arbitrary-sized file before checking its expected length; there is no payload cap, aggregate approved-source cap or final staged-byte cap. The generator's comment that top-level records are bounded does not impose a limit.

Use explicit per-payload and total package limits appropriate for this known evidence set. Check declared sizes and regular-file metadata before reading payloads, bound fixture aggregate bytes before staging/compression, and check final inventory bytes before rename. Preserve the existing exact-byte comparisons and per-fixture bound. No new data generation is needed.

## 5. Reject normalized destination aliases and constrain fixture roots

Raw destination uniqueness plus safe_rel does not reject two spellings of the same output path: a/b and a/./b normalize to the same path. It also does not reject a payload file that is a prefix of another destination. The later inventory can describe the overwritten file while copied metadata still claims two payloads.

Validate uniqueness over canonical PurePosixPath components and reject every file/directory-prefix collision across payloads, fixture archives, package metadata, inventory and the approval envelope before staging. Check fixture names as single safe components and roots as the exact ten expected rotation-{1..5}-{control,candidate}.deadpan paths under S. snapshot currently permits an arbitrary directory anywhere on disk; an explicit expected-root set is simpler and stronger than accepting arbitrary manifests.

## Additional implementation notes

- Preserve source snapshots as plain .rs.txt. The current two source files are below the generator's 100,000-byte compression threshold, but payload(..., compress=True) does not encode that policy. Pass False for source-text snapshots.
- Gzip payloads retain their original .json/.html names. This is decodable using manifest.encoding but differs from the preceding prototype package, where .gz made encoding visible. Prefer append .gz when compressing, with fixture-map references derived from actual returned destinations, or document the encoding-ledger-only policy explicitly. Do not create compatibility aliases.
- Sidecar after/live equality, SHM32768 and WAL0 checks, authored before/after/live comparisons, regular TAR members, readback equality, private sibling staging and Darwin RENAME_EXCL are good foundations.
- Keep exactly ten timing fixture plans, no memory lane, six inputs and all 35 media files. Preserve the raw completed report/check semantics via the independent analysis review. Do not infer runtime outcomes while measurement remains active.
- The final manifest review must reread every approved file, map, fixture and movie and bind the exact final script/manifest/review/plan/GO. This draft review supplies no GO and claims no package was produced.
