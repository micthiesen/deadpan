# Root sound events

See [the qualification record](../../../../docs/qualification/root-sounds-2026-09-27.md)
for the implemented boundary, review corrections and outstanding product work.

`verification.json` records exact commands where retained, results, durations,
log hashes and test summaries. Gzip logs preserve both failed development checks
and successful checks. `source-identity.json` identifies the initial workspace
gate; `remaining-source-identity.json` identifies its separate continuation.
`final-source-identity.json` records the state when this evidence was retained.
Changes between these states are explicit in the report.

The first workspace run failed two worker-supervisor pipe-EOF assertions. The
single diagnostic retry did not establish a fix. The separate
[process-launch qualification](../../../../docs/qualification/process-launch-2026-09-27.md)
records the deterministic race witness, production correction and affected checks.
The remaining packages and documentation tests passed independently. These files
do not claim an uninterrupted green workspace run.

No GUI, physical audio device, listening, export-equivalence or performance
acceptance was performed for this backend increment. The new sound-placement
imagegen board is an implementation target, not test evidence.
