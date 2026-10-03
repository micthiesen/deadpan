# Macro copy and paste evidence

See [qualification and limits](../../../../docs/qualification/macro-reuse-2026-10-02.md).

This directory retains complete compressed check logs, command metadata,
before/after source inventories, peer review notes, failed and corrected rendered
reports, and four inspected final captures. `metadata.json` identifies the
hardware, toolchain, final binary and passing test totals. `source-scope.json`
records the test-only changes after the full affected-crate run.

The first affected run, UI initialization-test race, empty-register replay
fixture error, and zero-test diagnostic retry are retained and explained in the
qualification record. They are not represented as passing verification.

Scripts are retained as `.py.txt` to avoid changing the checked source inventory.
Scratch projects, original media and authenticated discovery secrets are excluded.
`cleanup.json` records no running Deadpan app. `SHA256SUMS` covers every retained
file except itself.
