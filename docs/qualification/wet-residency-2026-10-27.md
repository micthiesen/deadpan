# Wet processor storage lifetimes, 2026-10-27

The shared audio graph now stops charging wet-constructor Rust storage after it
drops. Full construction admission, the complete native filter bound, work
charges, DSP and limits stay unchanged. Recursive multiple-output batching is
still disabled. The full real-WAV stages target passes its 203 existing cases;
the seven unfinished general Split cases remain four passes and three work
failures. No requirement or gate closes.

## Ownership and admission

`finite_wet_coefficients` returns only two coefficient vectors. Its local boxed
impulse network drops before native filter construction starts. Native creation
synchronously copies the coefficients into owned spectra and work arrays; the
Rust vectors drop before `FiniteWetEffect::new` returns. The installed processor
stores only its effect descriptor and native filter. All native FFT, spectra,
time, spectrum, product and staged-output arrays remain alive.

The new `retained_bytes_bound` keeps the entire existing
`CausalStereoFilter::native_bytes_bound`. The old preparation formula still adds
both Rust coefficient vectors and the 1 MiB temporary-network allowance. No
measured native-bound slack is released.

| Effect | Full construction admission, bytes | Retained bound, bytes | Dropped constructor allowance, bytes |
| --- | ---: | ---: | ---: |
| Reverb | 38,723,600 | 34,603,008 | 4,120,592 |
| Delay | 75,350,032 | 68,157,440 | 7,192,592 |

These are owned-storage bounds, not RSS measurements. Runtime caller histories,
outputs, tiles and caches keep their separate reservations. Existing ordinary
owner EQ/decoder/source-halo accounting policy is unchanged.

The private constructor helper preserves the exact order: spend history work,
`make_room` for full preparation, synchronous construction, cancellation/deadline
check, then publish the processor with its retained charge to `active_frames`
and `Graph.resident`. A populated effect slot charges neither construction work
nor another retained allocation. Existing graph cleanup subtracts the actual
retained amount on success or failure. There is no work refund, deadline reset,
new cache, changed source authority or reduced constructor headroom.

Five tests cover both effect formulas; exact full-preparation admission and
first/repeated retained charges; retained-only and one-unit-short preparation
refusal; cancellation/timeout/work refusal without publication; and the actual
Tone-plus-wet `read_shared_input` path's cleanup. The last test uses a fresh cold
reader with one less unit than a successful schedule, forcing a late refusal
after wet construction, as well as an entry-time refusal. The nonidentity send
and subsequent transfer dispatch supply billed work after the wet callback.

## Verification

Locked dependencies, opt-level 2, debug assertions and overflow checks, on the
same qualified macOS/M5 Max/Rust/FFmpeg/ORT environment as the Components
milestone. Main guards 1,398 inputs and changes exactly two source files plus
one test file from that milestone. The isolated published-runtime copy guards
1,170 inputs, changes the same three files and matches their main bytes.

| Check | Result |
| --- | --- |
| Final three-package build | Pass, 89.18 seconds. |
| Audio library | 213 pass, zero ignored. |
| Strict check | `cargo xtask check --no-test`: formatting and strict all-target Core/Plan/Audio/Store lint pass. |
| Complete real-WAV stages | 207 pass, three fail, zero ignored, 693.66 seconds. All 203 prior cases and processed root refinement pass. |
| Isolated three-file build | Pass, 87.08 seconds. |
| Isolated Audio library | 196 pass, zero ignored. |
| Audio definition | 171 pass, zero ignored, 258.33 seconds. |
| Isolated historical birth/processing PCM | Eight pass, zero ignored, 10.84 seconds. |

The remaining failures are Source/Sequence `cut-three`, limited `0+511`,
`plan work per read`; nested selected play without root sound at `prefix-two`,
limited `0+4096`, `frozen projection work exhausted`; and one selected play at
that same prefix/window with the same frozen-projection refusal. Cases and read
boundaries match the preceding milestone; the two selected cases previously
reported the read work limit. None of those three comparisons completes.

The strengthened late-cleanup test initially expected the accepted work counter
one unit below its ceiling. Actual execution fills the ceiling before the next
charge is refused. That test-only assertion is corrected; the failed 212/one
result and initial passing 213-test revision are retained. Production source bytes do
not change through the correction. The embedded PCM engine identity includes
test source, so the earlier integration executable also changes. Its running
stage suite is explicitly terminated after 112.80 seconds, retained as
interrupted, then replaced by the complete final-source run above.

Independent source review found no supported defect, including the test-only
correction. No new post-constructor cancellation injection or native allocation
failure/sanitizer run is claimed. Seven checked ownership sources, including the
native adapter, FFT and impulse network, match the
[existing native witness](finite-wet-2026-10-12.md). That witness observed native
peak equal to retained allocation; it supports keeping the full native bound,
not lowering it to measured bytes. No GUI, whole-product gate, packaging or new
throughput/RSS claim follows from these runs.

## Continuation and evidence

A cold resource fold still has to account for instantiated clocks, exact masks,
future required tiles/outputs, retained native plans and recursive scratch before
simultaneous histories allocate. The retained research identifies a shared
policy-only callback driver as the next code boundary. A first planner can keep
intrinsic stretch/projected preparation scalar, but its gate must inspect live
and historical compiled operands, not just authored pitch. Neither that planner
nor runtime batching is implemented by this storage correction.

Raw records remain in `/tmp/deadpan-wet-residency-2026-10-27`. The
[inventory](../../tools/media-qualification/evidence/2026-10-27-wet-residency/inventory.json)
retains exact patches/before-files, source and binary pins, main/isolated lineage,
commands, all results, the failed assertion and interrupted run, independent
review, prior-native hash comparison and cold-demand research. All receipts are
terminal. All 87 gzip payloads verify stored and expanded hashes: 2,451,404 stored
bytes, 10,053,466 expanded bytes. Inventory SHA-256:
`e2430bc97098086250c7318f949ba37da408d70dba452b6f4f79e00d350fd216`.
Keep unrelated general-Split, input-index and foreign fuzz/Cargo.lock/.claude
work uncommitted.
