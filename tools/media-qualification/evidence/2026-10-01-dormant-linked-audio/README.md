# Dormant linked audio evidence

See [qualification](../../../../docs/qualification/dormant-linked-audio-2026-10-01.md)
for behavior, test scope, corrected setup and remaining work.

- `storage-before` selected zero tests because the exact filter was incomplete.
- `storage-before-qualified` reached a rejected QuickTime test container.
- `storage-before-mp4` and `audio-before` reproduce the intended old-code failures.
- `storage-after` and `audio-after` retain corrected test-helper failures.
- `audio-after-oracle-fix` verifies all three PCM regressions.
- `affected-tests` retains the obsolete unknown-context-version assertion.
- `affected-tests-corrected` verifies audio and CLI, then reaches the old
  frozen-layout empty-placement assertion.
- `remaining-tests` covers core, plan, media and store after that correction.
- `formatting-final` and `strict-clippy` cover workspace formatting and all-target lint.
- `source-*.json` binds each run to source hashes; command JSON includes base,
  diff hash, start time, process ID, duration and exit status.
- `runs.json` is recomputed by `python3 summarize.py` without hand-counted tests.
- `host.json` records hardware, OS, fixture identity and native-app cleanup.
- `SHA256SUMS.json` binds the retained evidence bytes.

No native application was opened for this increment. Tests do not extend GUI,
physical-input, performance or release qualification.
