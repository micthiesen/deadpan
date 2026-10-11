# Local Original and sound acceptance, 2026-10-11

Source revision and executed binary are in `source-identity.json`. The sole
working agent ran the real release CLI on the reference Mac. No live GUI was
opened. Repository fixtures were read only; fault injection used a scratch WAV.

`checks/` retains commands, stdout/stderr, authored dumps, admission requests,
full quality reports and the 36-test store run. `output/` retains both emitted
movies and their verified publication reports. Large textual files use gzip;
`compression.json` records their original sizes and hashes. `SHA256SUMS`
covers every retained file. The original dump invocation omitted `--json`;
its usage refusal is retained beside the corrected successful command.

The run began with `cargo build --release --locked -p deadpan-cli`, then
`project create-original` from `cfr-bframes.mp4`, `project dump --json`, and
linked retention of a scratch copy of `pcm-stereo-48000.wav`. `scripts/accept.py`
continues from those recorded outputs: register, place, remove, corrupt,
restore, render, copy-portable, remove the external sound and render again.
`scripts/verify.py` checks every emitted picture and three audio windows.
`scripts/admission.py` tests deduplication, explicit container stream selection,
wrong-stream refusal and unchanged picture/mix after catalog-only registration.
The scripts use exclusive log creation; choose a fresh scratch directory when
repeating and replay the recorded setup commands with that directory.

The two portable ownership records are managed, without linked paths. Missing
and corrupt sound renders fail with `render_audio_failed`, confirmed cleanup,
no publication and identical authored state. Both successful renders publish
120 pictures / 192,192 canonical samples. Three quality comparisons pass
360 picture checks, six zero-offset signal windows and three silent windows;
the third comparison reuses the portable movie after catalog-only import.

The portable project and source package remain in the recorded scratch path.
The source package deliberately has its linked sound unavailable; only scratch
files were removed. No clean-machine, provider-eviction or physical-input result
is implied. The qualification document maps the complete DP-15 acceptance scope.
