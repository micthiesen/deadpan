# Borrow cached Authored dependencies

The Authored EQ reader now extends its dependency result from borrowed cached entries, cloning each AssetId and copying its 32-byte fingerprint. It previously cloned the entire BTreeMap before extending the result. Entry order, owned keys, values and destination overwrite behavior are unchanged.

This removes the explicit temporary map construction. Retained dependency storage remains a BTreeMap and still needs its own admission proof. No allocator-call, peak-memory or speed improvement is claimed. Work/residency formulas, limits and numerical processing are unchanged.

The parent reviewed the complete six-line expression change and its concrete key/value types. Optimized library suites pass 370 Main/351 isolated, with three ignored tools each. These suites include both complete recursive modes against the frozen seven-case native matrix, exact f64 output/dependencies and old-plus-new-index work accounting. Strict cargo xtask check --no-test passes all-target formatting/lint.

The preceding ordinary Stage/definition/history suites and three-pass/four-failure Split matrix were not rerun for this mechanical change. They remain historical evidence, not new execution credit. General sound-bearing Split stays disabled. No requirement or gate closes.

All five guarded receipts pass with unchanged recorded source/executable guards. The one staged Rust file equals both compiled source variants; all other 1,496 Main and 1,267 isolated initial inputs are unchanged. Eight immutable fixtures are unchanged. Scoped source patch SHA-256: f448cd1c7f8be57470a1c17fcdd5ab09dcd1952c24b7d18433ae34c715a088c9.

Base revision: 689f9e97cf09544729b7e2a9b79ba7fec7863c3a. Scratch: /tmp/deadpan-authored-dependency-borrow-2026-10-29.

The [inventory](../../tools/media-qualification/evidence/2026-10-29-authored-dependency-borrow/inventory.json) contains 43 compressed records, 983,325 stored bytes and 3,891,373 expanded bytes. Its SHA-256 is a2c81a6b28f61a076cdbdfdd6d75626eab69cf199cace024d2990a121800ca4b. Stored and expanded hashes were verified before publication. The preceding tile inventory is linked by its exact digest.
