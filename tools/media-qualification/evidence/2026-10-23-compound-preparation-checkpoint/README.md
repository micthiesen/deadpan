# Compound runtime preparation checkpoint evidence

See the [qualification note](../../../../docs/qualification/compound-runtime-preparation-checkpoint-2026-10-23.md)
for the change summary and exact verification boundary.

This package includes the six current Rust source copies identified by the
completed full UI source manifest; focused, broad, and UI identities and
receipts; complete compressed broad/UI archive logs; helper, input, binary,
build, and list manifests; and the initial zero-selection attempt. The partial
strict-check output is deliberately excluded. Executable archives, binaries,
generated packages, and Cargo target directories are excluded.

`source-copy-identities.json` records each copied file and its exact SHA-256.
`inventory.json` records the byte count and SHA-256 for every retained file
except itself.
