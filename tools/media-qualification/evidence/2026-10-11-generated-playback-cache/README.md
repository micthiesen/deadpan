# Generated playback cache evidence

See the [qualification](../../../../docs/qualification/generated-playback-cache-2026-10-11.md).
`manifest.json` hashes the retained logs, commands, failed fixtures, source
snapshots, review, scripts and executable identity. Logs and source snapshots
are gzip-compressed with a fixed timestamp. `identity.json` records the base
revision plus exact changed-source, test-executable, tool and library hashes.

`generated-playback-4` is the final passing two-test run; `resources` repeats
it from the copied release executable under `time -l`. Earlier numbered runs
are retained failures. `app-worker` runs the ordinary worker suite. The extra
large-package seek-storm test is the one unexecuted ignored worker test.
The synthetic fixture establishes actual acceptance and decoding, not real
model quality or full-resolution realtime performance.

To recreate in a new scratch location, select the qualified FFmpeg prefix
documented in `run.py`, build the `deadpan-media-worker` and `deadpan-track`
release binaries, then run:

```sh
DEADPAN_REQUIRE_SYNTHETIC_WORKER=1 DEADPAN_GENERATED_PREVIEW_EXPORT=/tmp/new-accepted.deadpan cargo test --locked --release -p deadpan-cli --features synthetic-worker --test generated_joins accepted_ai_pause_meets_the_original_with_exact_hard_cuts -- --nocapture --test-threads=1
DEADPAN_GENERATED_PREVIEW_PACKAGE=/tmp/new-accepted.deadpan cargo test --locked --release -p deadpan-app --bin deadpan-app worker::project_tests::generated_playback_tests -- --ignored --nocapture --test-threads=1
```

The app tests make verified portable copies and loop the accepted timeline
without editing its conditioning context. The retained fixture and executable
are in `/tmp/deadpan-generated-lookahead-20261011`; their pins are in the
manifest. `python3 check.py` checks the current source and evidence;
`python3 check.py --committed` checks source against HEAD at this milestone.
