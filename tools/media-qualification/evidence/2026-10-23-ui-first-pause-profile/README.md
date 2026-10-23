# UI first-pause service profile evidence

See the [qualification note](../../../../docs/qualification/ui-first-pause-profile-2026-10-23.md)
for results, measurement boundaries, and limits.

This package retains the instrumentation patch and notes, build and run
receipts, source/binary/input identities, original and proposed source
snapshots, exact final formatted source overlays, all nine trial input maps and
compressed reports/stdout/stderr, aggregate phase data, and parser inputs.
`inventory.json` contains the byte count and SHA-256 for every retained file
except itself. It excludes executables, Cargo targets, and fixture databases.

The final formatted copies come from the isolated instrumented source tree
that produced the recorded instrumented executable. Their hashes are checked
against the final source identities. Draft copies remain separately labeled;
they are not presented as the exact built source.
