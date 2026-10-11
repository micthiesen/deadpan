# Selection-loop decode-ahead evidence

See the [qualification record](../../../../docs/qualification/loop-lookahead-2026-10-11.md).
`manifest.json` pins all retained inputs, scripts, logs and reports, including
uncompressed hashes for gzip files. `measurements/identity.json` identifies the
executed benchmark; `final-source.json` identifies the final implementation.

The two `before-tail-fix` files retain the measured-era app source. Subsequent
review added the Original final-picture trigger and its passing regression
test; the measured Edit benchmark and shared scanner did not change.

`check.py` verifies hashes, final check exits, all four runtime comparisons and
local documentation links on the qualification machine. Run it with
`--committed` after committing to compare final source against Git HEAD.
