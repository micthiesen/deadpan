# Wider PCM sound import, 2026-10-09

DP-02 / DP-16, specification §16.3. WAV sounds now admit unsigned8,
signed16/24/32 little-endian and IEEE float32 through the normal catalog,
placement, history, audio preparation and Render paths. Implementation and
review belong to the sole agent. The full source-format and operation matrix
remains open; this record does not close either requirement.

## Container and sample contract

Before FFmpeg opens the immutable descriptor, bounded reads require one RIFF
WAVE file with exact length, one format before one aligned nonempty data chunk,
and complete odd-byte padding. Plain `fmt16`, `fmt18` with zero extension length,
and extensible `fmt40` are admitted. The extensible form requires exactly 22
extension bytes, equal container/valid widths, the PCM or IEEE float GUID,
and a nonzero known speaker mask with one bit per channel. Rate, channels,
byte rate, block alignment, sample count and header/packet budgets are checked.
One optional four-byte `fact` before data must equal the actual per-channel
sample count. FFmpeg cannot silently repair an interleaved or forged count.
`JUNK` is the only other admitted chunk.

The native decoder rechecks codec/block alignment and caps WAV packets using
their encoded sample width. It exposes unsigned8, signed16, signed32 or float32
as the actual decoder representation. Signed24 is left-aligned in signed32.
The owned f32 copy centers unsigned8 at 128 and scales integer samples by their
power-of-two full scale. Every normalized signed24 value is exact; signed32
rounds once to f32. Float32 is copied without leveling or clipping, preserving
signed zero, subnormals and finite values outside ±1. Non-finite samples fail
copying, poison that decoder, and cannot become a qualified PCM cache or source
receipt. Sample-format/codec pairs are checked again when reading receipts.

Unlabelled channels remain unlabelled. Registration requires the existing
explicit Mono or Stereo L/R choice; declared extensible speaker layouts need
no inferred assignment. Conversion, downmixing, resampling and final limiting
continue through the shared audio path when the sound is placed.

Primary format references are Microsoft's
[WAVEFORMATEX](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex)
and [WAVEFORMATEXTENSIBLE](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible).
The pinned FFmpeg 8.0.3 `libavformat/riffdec.c`, `libavformat/wavdec.c` and
`libavcodec/pcm.c` establish the measured decoder behavior. In particular,
FFmpeg can interpret 24 valid bits in a 32-bit container as its legacy float24
format. Reduced valid widths are explicitly refused here. Float64, RF64/W64,
RIFX/AIFF, LIST/bext and other metadata/container grammars remain unqualified.
These limits do not reduce the normative common-format requirements.

## Fixtures and boundary tests

`native/deadpan-source/tests/generate_pcm_fixtures.py` directly writes fourteen
RIFF files and independent scalar f32 references using Python's standard
library. No codec, resampling or third-party media is involved. Each contains
8,197 samples per channel, with unequal channel signals, integer extremes,
one-bit detail, zero, and float excursions. The matrix covers all five sample
representations, mono/stereo/5.1, 8/44.1/48/96/192/384 kHz, all three format
headers, optional `fact`, odd data padding and odd `JUNK`. The manifest records
source/reference hashes and exact declarations; regeneration is byte-identical.

Both native decode modes match every scalar reference bit for bit. Tiny packet
budgets preserve all samples and exact PTS while respecting encoded widths.
Malformed widths, alignment, rates, masks, GUIDs, extensions, fact counts,
truncation and metadata fail. NaN and both infinities fail and poison copying.
The existing container mutation campaign automatically includes the new small
WAV seeds. Media cache tests cover all references, half-open endpoints,
receipt round trips and invalid format/clock receipts. Public CLI tests cover
all fourteen catalog registrations, required speaker choices, no-write dry
runs/refusals, placement, limited audio preparation, undo/redo, and unchanged
picture structure/duration. Non-finite registration leaves history and receipt
counts unchanged.

During test development, one mutation changed float32 into valid signed32 and
another rewrote a byte with its existing value. Both incorrect test expectations
were corrected; the implementation was unchanged. The initial CLI test build
also required the explicit sibling-module path used by that test target.
The original failure logs remain in the evidence directory.

The workspace gate ran 5,568 tests: 5,566 passed and two failed, with twelve
existing skips. The new receipt-mutation test assumed two decoded frames;
one valid WAV fits in a single frame. Its invalid-duration mutation now works
for either packetization without changing production behavior. The existing
proxy retry test injected one refusal and assumed a real VideoToolbox retry
would succeed. That second encoder instead reported packet 56 at PTS 57057
with DTS 56056, where PTS 56056 was required. Production correctly refused
those bytes and exhausted its single retry. The test now deterministically
checks that a second, distinct error survives, exactly two workers start, and
both temporary outputs disappear; two repeated refusals also stop at two runs.
Existing real-encoder retry success and output-verification tests remain.
The updated targets are checked separately before resuming the unfinished gate
phases. No timing tolerance, packet validation or production retry limit changed.

The debug app linker emitted its existing `__eh_frame section too large`
warning, also present in the prior MP3 workspace build. It was not suppressed.

## Execution evidence

Evidence is retained under `/tmp/deadpan-pcm-20261009`. `identity.json` records
the starting revision and reviewed source/fixture hashes. `focused.json`,
`checks.json`, their command logs, and `before`, `release` and `packaged` reports
record the actual invocations and outcomes. Each public run retains its project,
requests, replies, movie bytes, exact revision and executed binary hash.

The preserved pre-change MP3 release CLI refuses signed24, signed32 and float32
registration without changing authored documents. Its SHA-256 is
`4acb7c778eab1778fac3217098e7f847a3088f58ac8449dfcf513f2ad45210ca`.

On Apple M5 Max / macOS 26.5.2 (25F84), Rust 1.97.1 and the pinned FFmpeg
8.0.3/libopus 1.6.1 prefix:

- All 5,568 workspace cases are covered by the full run and the 27-test
  media/proxy correction run. Twelve pre-existing tests remain skipped.
- Workspace and UI strict Clippy pass, as do the focused correction lint,
  1,097 UI tests (two existing skips), and documentation tests.
- All 151 native source tests pass under ASan/UBSan in
  `/tmp/deadpan-pcm-source-asan-20261009`. This instruments the native C adapter
  and target C dependencies, not Rust, FFmpeg or libopus.
- The correction run reported delayed-stdio `LEAK` diagnostics for
  `a_second_stall_is_reported_without_another_retry` and
  `a_sidecar_for_other_bytes_or_index_is_refused`. Their assertions passed.
  The stall test checks that its recorded children are gone; a later process
  snapshot found no remaining `sleep 6001` helper. That does not establish the
  cause of nextest's diagnostic. Both tests passed in the serial follow-up
  without warnings; both logs are retained and no timeout was relaxed.

The first release stress-file export passed picture and amplitude checks, but
its unsigned8 sequence repeats every 256 source samples (32 ms at 8 kHz).
The independent verifier correctly reported `audio_offset_unobservable`:
zero-lag correlation was about 0.999996 while ±1,536 output samples were equally
plausible. This is not zero-offset timing evidence. The original project,
published movie and failed verification remain under `release/`; no source
signal, verifier threshold, event alignment or export behavior was changed.

`tools/media-qualification/generate_pcm_export_signals.py` separately creates
six nonperiodic probes with the stress files' exact format headers, rates,
layouts and 8,197-sample lengths. Defined unsigned integer mixing supplies
independent channel values, interpolated between 431 knots per second for a
distinct timing signature that survives the existing resampler/encoder. The
script, scalar references and byte-identical regeneration establish their
provenance; generated files/hashes live in `export-fixtures/`. The fourteen
committed conversion-stress fixtures remain unchanged.

The release CLI registers all twenty files and places/renders each of the six
timing probes: unsigned8 mono at 8 kHz, signed24 stereo at 96 kHz, signed24 5.1
at 48 kHz, signed32 mono at 192 kHz, signed32 stereo at 384 kHz, and float32
stereo at 48 kHz. Independent emitted-file verification passes all 720 pictures
and 24 audio windows, with zero measured offsets. `release-events/report.json`
records the actual reports and movie hashes. The executed CLI's SHA-256 is
`fae1c8b49b56720aadaf495f74a4fbbefe7fb43c81c49f4eb42632e3d63db2b9`.

The ad-hoc relocatable bundle and full `bundle-verify` pass, including native/AI
runtime smoke checks, relocated dependencies and the negative tamper/missing
resource cases. From an unrelated directory with fresh HOME and PATH restricted
to `/usr/bin:/bin`, its CLI registers all twenty files and verifies unsigned8
mono, signed24 5.1 and float32 stereo exports: 360 pictures and twelve audio
windows, all with zero measured offsets. The evidence is
`packaged-events/report.json`; the executed CLI SHA-256 is
`55f953e8f7f0ef9d9095722939096935ed152b15882eb3fc427e23b8ff6884e7`.

Final formatting and diff whitespace checks pass. Reviewed production, test
and fixture hashes remain unchanged through qualification; `identity.json`
retains the two reviewed test corrections and the separate timing-probe source.
No clean-machine, physical listening or GUI interaction claim is made by these
headless checks.
