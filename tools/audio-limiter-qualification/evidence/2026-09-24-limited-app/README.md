# Limited audition application verification

The [qualification report](../../../../docs/qualification/audio-limited-2026-09-24.md)
describes this implemented boundary. Numerical corpus and compiler corrections
are retained separately in [native evidence](../2026-09-24-native/README.md).

All five required repository commands passed with 1,265 tests, zero failures and
zero ignored tests. `gate/final-3/report.json` preserves the original runner's
false all-file freeze result: two missing spaces were corrected in a crate README
during that run. The before/after hashes and retained README bytes isolate this
documentation change. No executable source or fixture changed. The later optimized
native build held all 400 selected paths unchanged.

`gate/final-1` and `gate/final-2` retain earlier test-reference deadline failures.
The references now use the existing sixty-second production preparation budget.
The production deadline was not increased. The first runner omitted failed test
binaries from its summary count; `count-correction.json` records the correct
988 passed and one failed without replacing the original report or raw log.

`native` records the optimized executable identity, compiler/host information,
unchanged logical project dump and native keyboard/visual observations. Screenshots
were inspected in the session and are not published here. These observations do
not qualify listening, physical devices, VoiceOver or all proposed interface states.

Verify retained files and parse command/test results without running the app:

```sh
python3 tools/audio-limiter-qualification/evidence/2026-09-24-limited-app/verify.py
```

`manifest.json` pins all evidence files. Logs use gzip with a fixed timestamp.
`retain.py` records the original host capture procedure and absolute scratch paths;
it is not an end-user setup requirement or a portable test runner.
