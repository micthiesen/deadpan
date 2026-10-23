# Compact audio bindings and full-capture fallback, 2026-10-23

Core 58 / SQLite 87 candidate: exact default binding fields are omitted/restored,
bounded raw preflight accepts absent paths, and nearly complete scopes fall
back before constructing a projection that cannot be smaller. Nondefaults and
the independent frozen-context/sound-clock formats remain unchanged.

All 1,513 Core tests (zero skipped), 3,499 broader selected tests (740 skipped)
and 1,292 UI-feature tests (eleven skipped) pass. Strict changed Core/Store and
explicit UI Clippy/format checks pass. Debug/release verify 15 pictures / 24,024
samples, and release verifies 19 pictures / 30,430 samples, all at zero offset with
canonical PCM/history controls. Both nine-file extracted portable packages
validate and doctor with unchanged paths/bytes. Native replay passes 197 checks
plus Kestrel; the 51 second-retry frames and 7 distinct-cause retry runs remain
warning evidence. It is an offscreen replay, without physical display or
listening claims.

Five quiet current-format pairs compare the prior qualified compact-edge
implementation to this candidate. First store-and-refresh medians are 114.587 /
93.633 ms, second 32.896 / 28.906 ms. These exclude picture display and do not
qualify a visible-Hold or percentile target. Full CLI replay validates all five
edited candidate packages unchanged. Perf reopening also checks head equality;
its receipt-assisted shortcut is distinguished from the full replay.

Retain the failed initial private-test borrow build, malformed nested negative
fixture and corrected 1,513-test run, old literal range expectation failure and
corrected 3,499-test run, passing output-handle warning with same-archive isolated
pass and clean full rerun, and old-code witness default-toolchain mismatch.
The old binding-codec witness uses a separately built Core 57 codec in an early
fallback prototype: its binding source matches qualified Core 57, while the
unrelated capture guard differs. It gives one expected omission failure and
one passing nondefault control, not a shipping-binary comparison.

Each phase retains exact Rust/input/executed-binary identities and source
overlays. Only the Store expected-value fixture changed after the final Core
pass. All production source stayed unchanged through subsequent qualification.
Source overlays use .rs.txt, and large text may be stored as deterministic
gzip. Binaries, Cargo targets/nextest archives and generated benchmark packages
are excluded. Real emitted files and portable package archives are retained.

Future visible/large-offscreen measurement plans are planning documents only.
They do not establish implemented instrumentation or measured picture latency.
See docs/qualification/compact-audio-bindings-2026-10-23.md for the current result.
