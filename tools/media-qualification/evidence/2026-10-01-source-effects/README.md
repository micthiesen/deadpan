# Source effect clock evidence

Implementation base: `3df3724d91811457c2ac1772688308047453f50a`.
See [the contract](../../../../docs/SOURCE_EFFECT_CLOCKS.md) and
[qualification](../../../../docs/qualification/source-effects-2026-10-01.md).

Command reports bind the base revision, diff, complete source manifest, duration
and exit status. Logs retain focused and full checks separately; `summarize.py`
counts unit/integration and documentation tests without combining duplicate runs.
`review.md` records the independent draft review; `integration-review.md` covers
the durable storage test and current schema integration.

All 3,046 workspace unit/integration tests and both compile-fail documentation
tests pass with none failed or ignored. Formatting and strict workspace/all-target
Clippy with `deadpan-app/ui-harness` pass. See `runs.json` for each exact command,
duration and separate focused counts. Full tests, formatting and strict lint share
source manifest `fb43d12fb6353e69e3f6fb2a1cdae2d02c8124e570a3f9c13514c191267a5051`.

`host.json` records hardware, OS, decoded PCM fixture hash and the final native
process check. `SHA256SUMS.json` seals every retained evidence file. No native app
was opened for this model increment. Native Trim, device playback, GPU/export and
release qualification remain open. The retained debug link warning is described
in the qualification page.
