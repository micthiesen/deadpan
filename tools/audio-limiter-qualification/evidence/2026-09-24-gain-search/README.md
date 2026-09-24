# Retained master gain experiments

See the [qualification report](../../../../docs/qualification/audio-limiter-gain-search-2026-09-24.md)
and [mastering boundary](../../../../docs/AUDIO_MASTERING.md). Neither the global
gain optimizer nor the finite-context candidate is adopted. Their passing and
failing results are research evidence, not app functionality.

`experiments.tar.gz` preserves original bytes under the scratch directory names:

- `producer`, `producer-v2`: failed 32-solve pilots, source and partial results.
- `producer-v3`: 16 numerical peak passes, including unwanted near-Nyquist muting.
- `producer-v4`, `producer-v4-full`: spatially distributed cuts, with the muting failure retained.
- `producer-v5`, `producer-v5-full`: reference-gain floor pilot and full 16-case run.
- `finite-producer`: three 23-case runs, two longer cases, gain/seek evidence, dependencies and sealed report.
- `design-audit`: original mathematical reviews, full v3 audit, v5 pilot/full audits and exact reference checks.
- `finite-review`: independent final-output and longer failure review.
- `native-meter-v3`, `native-meter-v5`: 32 reports from the retained compiled Rust meter.
- `dependencies`: exact published-FIR source and unchanged numerical sinc oracle.
- `gate`: the repository gate output. `docs-review.json` records the independent documentation review and applied correction.

`retention.json` records each entry's scratch source, byte count and SHA-256,
plus the archive checksum. Raw f32 files duplicate WAV payloads and are omitted;
all output WAVs and saved finite gains are retained. Earlier repeated/subset v3
audits, Python caches, uncompressed duplicate logs and the meter executable are
also omitted. Compiled meter identity is recorded in both run summaries. The
source seal checks file identity, not a reproducible build or formal numerical
certificate. Reported paths inside original files describe the actual run host.

Verify without extracting or running any producer:

```sh
python3 tools/audio-limiter-qualification/evidence/2026-09-24-gain-search/verify.py
```

This checks every tar entry, parses Python/JSON/WAVs, joins inputs/outputs/reports,
reconstructs PCM from saved gains, checks compiled-meter input identity and
retains the decisive failure assertions. It does not rerun the numerical oracle
or independent exact-FIR audit. `verification.json` records the observed result;
`SHA256SUMS` covers the archive, manifest and surrounding files except itself.

For inspection or reproduction, extract into a **new scratch directory**:

```sh
mkdir /tmp/deadpan-gain-inspection-new
tar -xzf tools/audio-limiter-qualification/evidence/2026-09-24-gain-search/experiments.tar.gz -C /tmp/deadpan-gain-inspection-new
```

Do not run the producers in that evidence copy: several intentionally overwrite
their own `results.json`. Copy only the needed scripts into a separate empty run
directory, preserving `finite-producer` beside `producer-v3` when using its corpus.
Update the copied scripts' absolute `REPO` constant for the current checkout;
their newly recorded script hashes will then differ from the retained originals.
The current `true_peak.rs`, old post-mask inputs and old `finite_sinc.py` must
match the retained dependency identities before comparing results.

For example, copy `producer-v5-full/experiment.py` into a new empty directory,
update that copy's `REPO` if necessary, then run:

```sh
uv run --python 3.12 --with numpy==2.5.3 --with scipy==1.18.0 --with mpmath==1.3.0 python /tmp/NEW_RUN/experiment.py
```

The v5 full run exits zero for its retained corpus. The initial LP pilots and
the finite R256/R4096/long runs retain expected failures; their original README
and failure records explain exit codes and incomplete scope. The independent
auditor can be copied to another scratch directory and given `--producer` to
read an extracted producer without re-running optimization. Retain its exact
script/dependency identities when comparing reports.

All audio is generated test material. Python, uv and scientific packages are
developer tools, not end-user or application requirements. These fixtures are
short numerical controls; they do not qualify listening, physical converters,
AAC, native GUI behavior or production latency.
