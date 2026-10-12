# Native EQ evidence

See the [qualification](../../../../docs/qualification/native-eq-2026-10-12.md).
All work was performed by the primary agent alone.

`source-pins.json` and `binary-pins.json` identify the final implementation and
executed helpers. `initial-*` records the first frozen build, used by `eq-first`
and `gain`; these passed before the presentation labels were corrected.
`equalizer`, `gain-final`, `audio-treatments` and `equalizer-performance` ran the
final app. Complete reports and logs are compressed without content changes.
The three PNGs are the inspected final EQ captures.

`app-tests.log.gz` records the full app suite. `gain-tests`, `eq-tests` and
`registry-tests` cover final source changes. The three `registry-update` logs
retain the intentional snapshot-regeneration failures. Final registry tests
passed without regeneration enabled. Nine opt-in AI/Generated/stress tests
remain explicitly ignored in the full app run.

Run `python3 check-evidence.py --source` here to check hashes, final test/replay
results, shortcut audits and source identity. Without `--source`, it verifies
the historical evidence even after implementation changes.

The retained `run.py`, `checks.py`, `freeze.py`, `replays.py` and `collect.py`
record this machine's exact commands and scratch layout. The frozen helper
baseline came from `/tmp/deadpan-gain-waveform-20261011/bin-release`; its bytes
are pinned, and app/CLI executables were refreshed from `target/release`.
These scripts intentionally refuse overwriting command logs. For a fresh
checkout, build the current helpers with the project's replay tooling and run:

```sh
cargo xtask replays --scenario equalizer,gain,audio-treatments
```

For a separate performance run, use a fresh output path and a frozen release
app with its helpers beside it:

```sh
deadpan-app --ui-check --scenario equalizer --mode performance --hz 120 --output /tmp/deadpan-eq-performance-new
```

This evidence exercises offscreen Metal and production input/service paths.
It does not establish listening quality or physical keyboard/display behaviour.
Shared PCM and emitted-file evidence is retained in the preceding
[`2026-10-12-shared-eq`](../2026-10-12-shared-eq/) qualification.
