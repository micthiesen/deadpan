# L archive and documentation audit

Verdict: GO. This independently audits the completed evidence archive, its Scope mirror, exact shipping source, final acceptance/reference chain, all40 immutable payloads, and four current documents. No source/evidence file was changed; no Cargo, compiler, test/native executable, or Git ran.

Machine reference verification: archive-documentation-review.json,
373,005 bytes/SHA256
1b7fd1a64a651af71c089036ad2b2cd9f533b56c35e07219be7e5410f35b90d5.

These two review files were produced after inventory freeze. They may be delivered as plain post-archive reports; they are not falsely represented as compressed records in the frozen inventory.

## Archive completeness

Root inventory is363,916 bytes/SHA256
930fa8fa0227994adc1683bb67dfa2cb02457486a58b46c05cc1f9abb7ca2ff8.
Its878 logical records resolve to605 unique content-addressed gzip payloads,
143,400,770 stored bytes. Logical expanded records total5,337,671,372 bytes;
deduplicated aliases are counted only once for physical stored verification.

Every unique compressed size/SHA256 and every expanded size/SHA256 was independently recomputed in bounded1MiB reads. No661MiB raw capture or other large object was materialized. Inventory deduplication tuples agree, payload filenames bind expanded hashes, paths stay inside the archive, no payload symlink is used, and no referenced or extra payload is missing. Scope's inventory is byte-exact; all605 mirrored compressed payloads independently match size/hash, hence decode to the same already-verified expanded bytes.

Exact17 shipping records match the current authoritative Scope files AND the same owned paths in Main. They also match the sealed acceptance's shipping_source_pins. The17 include only owned Audio paths and frozen40, with no Plan kernel expansion. Archive origins are17 Scope shipping files,198 curated scratch candidate/review records, and663 L scratch receipts/data/tooling. Reconstructed builder filters exclude main/before-main source snapshot directories. Main execution receipts, hash-only source manifests and actual fixture data are retained; broad Main source/WIP snapshots are not imported.

## Acceptance and reference chain

The archived after8-runtime-acceptance.json is byte-exact to the sealed independent GO:
89,631 bytes/SHA256
4a7bf9c40aa50fd084de33729a3e5fcfcb161c8e545787cfb5d466ef015151f9.

Twelve digest-bound runtime/host derivation scripts/results, residency authorities, compile/lint correction authorities and final static review references resolve to actual archived bytes. Context-relative static findings resolve uniquely inside their curated review prefix; none is presented as a failed native execution receipt.

All320 actual stdout/stderr references match their archived execution receipts. Sixty-two explicit acceptance receipt/static-record references resolve, including retained nonzero failures and controller/static reporting records with their actual distinct schemas. Every final capture's proof, raw body, canonical body and source manifest resolves with the sealed exact size/hash. Final source maps equal the guarded capture source_before/source_after; proof binary identities equal the receipt's actual before/after executable hashes. Persisted qualification receipt/native index/layout/provenance is retained in those complete raw captures and remains bound to the sealed provenance derivation.

For all42 Audio target receipts, the current build8 artifact hash equals the accepted actual execution binary hash. Archived actual stdout test-result lines independently yield:
- Main:1,152 passed,0 failed,34 ignored,7 filtered.
- Scope:1,146 passed,0 failed,34 ignored,0 filtered.
There are21 actual targets per copy. The retained-results join is byte-based, not an assumption that an old command ran current sources.

All40 physical oracle files were independently hashed in BOTH current copies against the immutable pinset. Frozen40 remains5,434,910 bytes/SHA256
7d0f5026041c3bc7c1dd33c5ec5052a9ec5325cae2d94113a8972c5d9ff61c42.
No payload regeneration or capacity/work/error normalization was needed.

## Four-document claim audit

Qualification report, AGENT_HANDOFF, REQUIREMENTS and projected-policy-source-admission match documentation-pins.json identically in Main/Scope. Their five new local L links resolve. The design retains its explicitly historical source pins and earlier unit boundaries; the current L status is separately identified.

Current claims agree with sealed actual evidence:
- Actual Signal/Tape/PreserveTape policy owners retain inventory/final allocation guards through their selected uses; input graph/recursive overlap and once-forwarded query work are bounded native claims. The discard-only seam introduces no output handoff.
- All six complete behavior bodies remain167,157,951 bytes/SHA256 988c44a96821f86d4bb25e84b0bea197e30028b857e505c4aeb8dd0438838712. Ordinary capacities/raw errors remain literal; active slice capacities and failed local work split remain unexposed.
- Non-residency114 captures have1,630 host brackets and ZERO accepted Plan/cache host units. Positive callback work is in the Plan lane; no positive optional-cache callback lane is claimed.
- The separate227 resident captures have12,485 host brackets,3,245 positive Plan-host units and zero cache-host units. These are actual ops phase7 residency bookkeeping, separate from query canonical/additional work; zero eviction-host delta is preserved.
- All1,362 resident factories succeed. Natural L-constructor reservation exhaustion is explicitly UNCLAIMED. The54 failed factory prefixes are53 named injected refusals plus one child cancellation, not rewritten as natural quota exhaustion.
- Actual parent/child lifetime sequence0→57→195→57→0 is separate from paired384 and resident297 admitted-ledger peaks. None is called physical memory/RSS or a full public residency threshold.
- Initial resident2048 held trial stays UNQUALIFIED because retry after drop also fails. Matched minima2051 dropped/2058 held and fixed2057 actual account10→3/refusal/drop/recovery remain recorded with fresh ReadWork per public call and same controller/cap.
- Build4/strict7 and the failed controller initialization remain explicit failures. Final8 passing evidence does not overwrite them. Source2653 shipping authority and excluded Main WIP remain explicit.
- No requirement/Gate credit, general sound-bearing Split enablement, whole preparation, Mix-own-policy, ordinary graph/PCM/dependency ownership, allocator/RSS or decoder-I/O claim is added.

No blocking archive/documentation defect found. Shipping/publication remains the parent's responsibility; this review does not execute or authorize an additional product capability.
