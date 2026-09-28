# Hold audio command evidence

See the [qualification record](../../../../docs/qualification/hold-audio-2026-09-27.md).
Exact command records, terminal logs and source manifests retain focused and
integrated results, including the invalid legacy-fixture-hash failure.
`summary.json` contains terminal status and test totals; `manifest.json` records
byte hashes. A source manifest covers tracked and new Rust/native code,
configuration, Python tools and all files under `crates/` and `native/`.

The DB37 fixture's producer, chronological command log, SQL and old-binary
provenance are retained separately in
[`crates/deadpan-store/tests/fixtures`](../../../../crates/deadpan-store/tests/fixtures/).
The corrected room-tone design board and prompts belong to
[`docs/design`](../../../../docs/design/README.md). They are design targets,
not native implementation evidence.

No native widgets changed in this backend increment. Optional UI replay,
physical keyboard/IME, display, audio device and listening checks were not rerun.
The real PCM witness uses bounded headless reads and a qualified local fixture.
