# Selected audio crops after exact speed changes, 2026-10-20

This increment admits an otherwise eligible exact-clock tape-speed update
when a Source's selected audio crop has no historical mix samples. It also
fixes retained silent Holds attempting an unnecessary rate-limited transfer.
DP-07 remains partial. Base `d0f675a5`, Apple M5 Max, 128 GB, macOS 26.5.2
(25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Executed old-code witnesses

The initial admitted graph has a one-frame Source and a 100000-frame silent
Hold inside an ordinary FollowSpeed Retime with output 2000 frames. Two public
pause insertions capture its historical audio clock and leave an exact stage
extent of 2003 frames. The Source uses 160 native samples `[3100,3260)` at
44.1 kHz, their natural mapping duration `16000/147147` project frames, and
selected local support `[1/2,501/1000)`. The project rate is 30000/1001.
Compensating authored endpoints exercise both positive and negative one-sample
audio offsets without moving the actual selected interval.

The complete old Source carrier is `[3203,3235)`, but its selected raw domain
is `[3219,3219)`. A partial-binding control uses the real capture factory:
the full fresh carrier is `[4805,4837)`, with selected domain `[4821,4821)`.
Both old-code core witnesses finish their geometry checks and then reject
the requested million-frame output with `retained audio transfer`.
The retained old core test binary has SHA-256
`15a56fa99b949863e76620ce0e76a9931e1e8658327a3f06e32886f255721406`.

The corresponding coherent test-only renderer candidate allocates selected
samples `[12801,12817)`. Its actual bounded PCM read already returns zeros,
full suppression and no provider calls before the public command refuses.
The old PCM binary has SHA-256
`faf7b06530c6c9ddff4be5cd8bcd3d690718555b41ec953be79613706a8532e5`.
That failed test stops at the first retained, positive-offset case; it does
not establish old-code execution of every later case in the loop.

An extended boundary case selects `[999/1000,103/100)`, intersected with the
Source's intrinsic `[0,1)`. The selected interval alone has a nonempty old
carrier, while the meaningful intersection has none. Its enclosing PCM read
crosses into the neighboring silent Hold and exposes a separate real failure:
`source step outside 1/64..64`. The retained silent Hold has a nonempty raw
domain and transfer `2003/1000000`. This failure and its diagnostic trace are
retained; a passing isolated Source read did not prove the enclosing read.

## Implemented boundary

Core preflight intersects current meaningful support with SelectedPlacement
audio endpoints, including the offset exactly once, before testing old
sample-boundary equality. Existing bindings use their retained lattice;
missing bindings use the actual pre-edit capture clock. A disjoint crop also
proves an empty raw domain. Recipes, historical timing rows and retained
bindings remain unchanged.

The candidate allocation check still uses the complete physical allocation.
Clipping that check to the selected crop would incorrectly skip a transfer
that the renderer still constructs for nonempty old support. Independent
review retracted that proposed shortcut after checking renderer ordering.

The plan exposes a narrow direct-silent-Hold predicate. After ordinary
cancellation, depth, plan-identity and request-size checks, StageAudio returns
its existing exact-zero block with a full suppression mask for that typed
case. It does not infer silence from sample values, classify composite stages
or Repeat gaps, or bypass Source/RoomTone transfer limits. No schema or
keybinding changes are needed.

## Verification and limits

All eighteen focused core/semantic and nine PCM checks pass. The required
`cargo xtask check --since d0f675a5` passes formatting, strict Clippy and all
2,375 core/plan/audio tests, with none skipped. Independent core and audio
reviews found no remaining concrete issue after the enclosing-read fix.
No app or keybinding behavior changes, so unchanged UI-feature tests and
painted replays are not repeated for this increment.

Both existing release H.264/AAC speed-update exports pass on the final source:
all 19 shortened and 34 lengthened pictures, both complete audio windows at
measured offset zero, and no verification flags. Maximum block-level
differences are 0.005109 dB and 0.007144 dB; minimum block SNR is 53.043 dB and
32.600 dB. This guards the existing finite export path; it does not establish
emitted-file evidence for the extreme crop fixture below. The executed CLI
SHA-256 is `99086229b972878f736778c7b10e2a919c5351fd212eeb432fa2f6a30e718f5d`;
the release export test is
`ff318a14b483137e20ec1b90cfbcfff19410cf80bcd5289dc8ded7622040e4ef`.
All 102 backend test executables are also pinned in the evidence.

Focused checks cover retained and newly captured clocks, both offset signs,
intrinsic-support clipping, disjoint crops, unchanged historical data, public
update, exact inverse and JSON roundtrip. Actual PCM checks compare complete
bounded reads and alternate partitions, masks and provider-call counts.
Neighboring nonempty Source support and a public change from Silence to
RoomTone still refuse the unsupported transfer before reading PCM.

The initial fixture attempts used an invalid no-media Source, an unsupported
native mapping and an incorrect full-allocation assertion. They were corrected
before the natural-rate old-code witnesses; their failed logs are retained
as superseded fixture attempts. Later boundary assertions incorrectly used
the validated range constructor for a disjoint intersection and a retained
constant for a fresh clock. Those failures and corrections are also retained.

The million-frame candidate is exercised through bounded canonical PCM reads,
not a full export. Physical input, listening and performance claims are
unchanged. A subsequent increment qualifies
[one inner Repeat level](inner-repeat-speed-2026-10-20.md). Nested Repeat and
processing families, fractional own output extents, non-Sequence ancestors
and the remaining Section 8 work remain open.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-20-source-crop-carriers)
records commands, failures, final results, source/binary hashes and independent
review. Its verified inventory contains 81 files totaling 277,869 bytes.
Progress-only commit `fb70355c` was pushed during verification; it changes no
Rust or dependencies. The root lockfile remains SHA-256
`1e39d18ae4de6c7b898edad39eb7104235fb3ffe3af84521ca94df2b6c25b71b`.
