# Reusing independent source projections, 2026-10-12

Nested Preserve with event and owner EQ no longer exhausts the preparation-stage
budget by rebuilding the same independent input. The failure discovered during
[partial Repeat work](partial-repeat-sounds-2026-10-12.md) occurred in the
pre-edit reference render. A temporary witness showed the same two nested stages
being prepared repeatedly through count 64. The limit correctly stopped that
work; increasing it would not fix the repeated preparation.

The checked occurrence builder now supplies a private cache key containing the
complete source recipe, concrete stable instance, resolved clipping boundary and
enclosing stage. The audio request cache pairs it with the owning immutable
plan's retained identity. Separate recipes, offsets, plays, stages, contexts and
plans cannot substitute for each other. General tape and mixed projections keep
their opaque identities and cannot claim this equivalence.

Preparation, preflight and active-recursion checks use the same key. Reuse still
checks depth and current source dependencies; existing memory, disk, stage and
frame limits remain unchanged. Key construction and bounded comparisons spend
plan work. No authored state, schema, command or DSP algorithm changes.

## Verification

The previously failing EQ regression now completes partial Repeat, count growth,
silent gaps, copy and recopy with its existing independent PCM oracle and 1e-6
treated-sample tolerance. Dry and equalized cases remain separate tests.

A real WAV-backed unit regression reconstructs an occurrence 80 times and gets
one prepared result, re-admitting its source on every reuse. A changed offset
produces distinct PCM and another preparation. Foreign plans reject direct use
and receive separate preparations in their own scope. Missing sources, excessive
depth and a one-stage budget still refuse. Plan tests distinguish nested stages,
changed recipes and complete-processing contexts and retain opaque identity for
arbitrary mixed projections.

The CLI fixture imports the existing Original MP4 and stereo WAV, wraps the
Original in 120-to-180-to-120-frame Preserve stages, then attaches the WAV with
event and owner EQ. Original audio is independently placed outside its beat, so
it cannot supply the attachment oracle. Three cold 64-frame reads match the
containing 256-frame authored-bus block with zero difference.

The prior qualified `0c445fc3` release binary also reads those small windows
correctly. Its full Render of the same project fails with
`render_audio_failed: audio stage preparation exceeds prepared stages per read`.
The new debug and final release binaries render that exact committed revision
successfully. Independent emitted-file verification passes all 120 pictures and four audio
windows, including 20 compared audio blocks and zero measured signal offset.
Minimum block SNR is 10.4370659402 dB and maximum block level difference is
0.2644696689 dB under the unchanged thresholds. The before/after small PCM
windows are bit-identical.

All 1,139 plan, audio and playback tests pass, with no failures or ignored tests
across 52 result groups. The complete suite includes source revocation, retained
clocks, nonlinear/shared processing, loops, resource limits and worker recovery.
Strict workspace Clippy, focused release cache/PCM/CLI checks, release app/CLI
builds and formatting pass.

## Scope

This resolves the nested EQ preparation failure recorded in the partial Repeat
qualification. Its separate 13-picture AAC fidelity failure remains open; reuse
does not authorize a codec tolerance change. Broader attachment transforms,
mastering qualification and the remaining product requirements remain open.

The primary agent implemented and self-reviewed this increment alone. No live
GUI, device output or listening check ran. No DP requirement or release gate is
newly complete. [Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-source-projection-reuse/README.md)
includes check receipts, source/binary identities, the complete project and
emitted movie, verification reports and the earlier failed diagnostic runs.
