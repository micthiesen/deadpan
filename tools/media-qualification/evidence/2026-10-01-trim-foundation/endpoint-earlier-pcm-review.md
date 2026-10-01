# Earlier source-endpoint PCM review

Scope: static review of `disjoint_earlier_endpoint_uses_signed_old_origin_with_offset_once` and the endpoint binding/source mapping path after the reported first-sample mismatch. I did not edit the checkout or run Cargo/native/media code.

## Finding

The failure is caused by the test oracle using the wrong filter-support start. The expected phase `4968/5` is correct for the retained endpoint binding. Production does not need a phase change based on this failure.

The current `SourceAudioMapping::Placement` has start `-5000/8008` and `audio_offset = +7` mix samples. At 30,000/1001 fps, the offset adds `7 * 5/8008` frames, so the effective placement start is `-4965/8008`. Its full extent is `40985/8008` frames.

The endpoint binding reports retained root origin `-1`, local boundary `2`, and reference-local delta `4005/2002`. At output sample 1602, its retained reference sample is:

`B(-1) + 3204 + (1602 - B(3)) = -1602 + 3204 + (1602 - 4805) = -1601`.

One reference sample at index `-1601` lies `3/8008` frames after the exact `-1` origin, or `3/5` of a mix sample. Applying the effective mapping start gives source phase `993 + 3/5 = 4968/5`. This differs from the unbound current-tree phase `4967/5`; the latter comes from the raw query and is not the retained binding's oracle.

Filter support is independently constrained by the current Source's intrinsic local range `[0,6]`. Intersecting it with the effective placement `[−4965/8008, 36020/8008]` maps to source ticks `[993,8197)`: the local-zero boundary maps to `(4965/40985)*8197 = 993`, and the placement end maps to the measured source end 8197. The diagnostic raw span confirms support `[993,8197)`.

The current oracle instead uses `[0,8197)`, allowing taps before the placement's authored support. Keep the expected phase `4968/5`, but use support `[993,8197)` for both it and the wrong-phase control `4963/5`. Keep `[0,8197)` as a near-edge support negative control. A second comparison at output sample 1802 uses phase `5968/5`; the 128-sample left filter extent is `1065.6`, beyond support start 993, so full and clipped support should agree there.

## Verification boundary

This conclusion follows from the exact mapping and bound coordinates plus the emitted span's literal support. It is not an executed confirmation that the proposed test correction passes; the root agent owns reproduction and runtime checks.
