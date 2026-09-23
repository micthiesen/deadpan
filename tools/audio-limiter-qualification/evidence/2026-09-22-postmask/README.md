# Retained post-mask experiment

See the [qualification decision](../../../../docs/qualification/audio-limiter-postmask-2026-09-22.md).
All 16 outputs pass the two finite detectors; four named cases (two distinct
outputs) fail the complete finite-sinc reconstruction check. The prototype is
not adopted. `producer/README.md` is the original pre-audit note; its references
to a pending audit describe that earlier point in the experiment.

`producer/` retains original scripts, reports, coefficients and WAVs.
`audit/` retains the earlier independent oracle and this run's driver, metadata,
results and high-precision witnesses. The parent performed this run after fresh
review agents hit a usage limit. `retention.json` binds byte-identical copies to
their scratch sources and records pre-run hash rechecks. It excludes this later
README. `SHA256SUMS` covers every retained file other than itself.

To verify integrity, run `shasum -a 256 -c SHA256SUMS` in this directory.
Raw f32 payloads equal the retained WAV bytes after the 44-byte header. Audit
input WAVs equal the retained producer WAVs, so neither is duplicated here.

To repeat the complete-sinc audit, use a fresh scratch directory. Copy only
`audit.py` and `finite_sinc.py` into it, then change the copied driver's `SOURCE`
to the absolute path of this archive's `producer` directory. Run:

```sh
uv run --python 3.12 --with numpy==2.5.3 --with mpmath==1.3.0 python /tmp/NEW_AUDIT_DIRECTORY/audit.py
```

The driver exclusively creates `inputs/`, verifies producer hashes before and
after the audit, and snapshots all inputs. Exit zero means the audit completed;
`summary.json.failures` contains the ceiling violations. Results in this archive
must remain immutable. Do not rerun the producer here: its scripts write outputs
and require an explicitly selected compiled meter. Reproduce a producer only in
a separate copy with paths reviewed. The captured executable hash establishes
identity but does not provide a reproducible-build recipe.

Python, NumPy, mpmath and uv are development tools, not end-user requirements.
