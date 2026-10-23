# Fine source freeze provenance

Root ran the freezer as an inline Python tool command, not as a freeze.py file.
No named freezer script was executed.

After rustfmt with skip_children=true on the nine owned paths, that command:
1. Read coarse-frozen-sources.json and hashed every Rust file in source/.
2. Required identical path sets and exactly 2,054 files.
3. Required the changed set equal the nine paths now listed in overlay-audit.json.
4. Checked every one of the six coarse-baseline-inputs.json hashes.
5. Read each before file from the immutable coarse source and each after file from this fine source.
6. Wrote source-retention/before and after copies with .rs.txt suffixes.
7. Constructed overlay.patch with difflib.unified_diff using a/ and b/ path labels.
8. Wrote frozen-sources.json and overlay-audit.json with map and patch SHA-256.
9. Copied baseline-inputs.json from the coarse input map.
10. Extracted the eight helper test names and wrote expected-helper-tests.json.

Coarse source map SHA-256:
12acb33168d093a788cba673ba98c532c6729c6f73564ee75d57d7c473e8ab41

Fine source map SHA-256:
e58e62f56f0859cb184e54c56ac0def09b6ec9315b72838feafe1f9ae6c76351

Fine patch SHA-256:
246588331baeb9664b6c701d13b9cc935fd9f5e60c100338a0a69aecd8ec38a8

All 2,045 nonowned Rust files and six inputs were unchanged. The independent
source review verified the full map, scope and retained source copies.
This prose records the actual inline procedure and does not claim a later
reconstruction was an executed producer.
