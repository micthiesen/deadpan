# Rate witness carrier correction

First execution passes the public pause baseline, exact geometry, audible
256-sample literal old phase, live candidate allocation, reference entry3203
and transferstep103. It fails the diagnostic's raw-domain support assertion:
actual[3203,163363), expected[3203,8008).

The proposed assertion confused visible left-partition support with the
retained Source carrier. The alias references the original full Source,
historical root[2,102), whose RoundEven endpoint is163363. The current left
partition ends at oldroot5, but that does not redefine the raw carrier grid.
The corrected assertion uses the independently calculated full endpoint;
current finite support remains a separate constraint. No production change,
rate-bound policy, phase comparison or accepted-error assertion is changed.

First log and executed test/source hashes are retained separately. First
execution did not reach the actual StageAudio rate error; the rerun must do so.
