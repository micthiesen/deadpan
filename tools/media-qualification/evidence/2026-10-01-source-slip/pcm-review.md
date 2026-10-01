# Source Slip PCM review

## Scope and result

Reviewed the corrected PCM witnesses in `crates/deadpan-audio/src/bound_reads/source_origin/gain/slip.rs`, the gain fixture/oracle in `gain.rs`, the prefix/rebase and source oracle helpers in `source_origin.rs`, and the downstream source recipe support clipping in `crates/deadpan-audio/src/sequence.rs`. No actionable correctness findings.

I did not run Cargo or native tests. The parent reports all six focused source-origin PCM tests pass.

## Independent arithmetic checked

At 30,000/1001 fps and 48 kHz, one project frame is exactly 8008/5 = 1601.6 samples. The Source mapping places its selected first sample at 2/5 of an input sample after the mapping start; its seven-sample audio offset is retained separately. The oracle's base phase 19967/5 = 3993.4 follows 4000 + 0.4 - 7. A resumed binding adds its exact integer sample phase, and a Slip of `delta` adds 8008/5 samples per frame, so the test's `phase(resume, delta)` is consistent.

The selected source interval before Slip is [3993, 5594.6). Translating both exact endpoints by 1601.6×`delta` and taking `ceil` yields the integer source support used by the oracle. This agrees with `sequence::source_recipe`, which clips integer sample reads to the exact half-open support before filter I/O. The dormant case checks 0..1595 taps and 1594 output samples at phase 0.4; its last sample is at 1593.4, inside the selected window. It checks no provider calls before activation and after inverse restoration.

The retained gain oracle keeps the owner envelope and mute coordinates fixed while changing media phase. Its mute indices 103..119 follow the exact [517,597) sample-coordinate mute interval at positions 2+5n. Rebased cases shift physical owner positions and owner-key coordinates together; comparing against the same effective gain curve is correct. The independent root sound receives the root trim/step once and remains unchanged by Slip. Chronological phase checks use B(1)..B(4) = 1602, 3203, 4805, 6406, then the +64-sample resume and -7-sample offset, yielding source sample 4861. The ±1 Slip expectation adds ±1601.6 samples.

## Fixture isolation and coverage

Hardening the leading Hold's `node_end` edge in the PCM fixture is justified. The independent root sound begins at the Source join, where the default Hold-end edge would add its own 96-sample fade. The fixture serializes a typed cloned Hold, so the default-omitted edge field is actually overridden. The same policy is present before and after Slip; this removes an unrelated join fade without hiding a change to Slip's media mapping. The witness still checks a separate root sound, gain, mute, output equality, and exact inverse restoration.

The cases cover unbound, captured, previously moved, resumed, and prefix-rebased bindings; dormant measured audio activation and restoration; cold chunked reads; and chronological resume/reanchor expressions. Expected phases/support are constants derived in the test helpers rather than read from RenderPlan or binding resolution. The oracle shares the canonical prepared resampler, but not the implementation's mapping or source-bound calculation, which is appropriate for an exact PCM witness.
