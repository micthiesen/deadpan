# Shared Components outputs, 2026-10-27

Components can now emit several requested contributions into borrowed output
buffers during one common traversal. The production reader still requests one
output. Recursive EQ/wet graph batching remains disabled until its cold resource
peak is proved. General sound-bearing Split remains unqualified: the unchanged
seven real-WAV cases still total four passes and three work-limit failures.

## Execution contract

Each destination supplies its own selected voice and alias mask, interleaved or
planar stereo storage, and optional suppression storage. Every shape and empty
suppression vector is validated before callbacks or mutation. Multiple-output
suppression requires preallocated capacity for the whole window. The caller must
also admit destination PCM and selector headers alongside Components residency;
shape validation does not establish recursive allocation admission.

Constant and dynamic processing execute the common driver once. Final transfer
is applied once per route selected by any destination, then each destination
accumulates its own selected routes in original route order. Unrequested final
gains remain lazy. Alias masks and suppression remain independent per output.
There is no unioned output policy, subtraction from a total, reassociated sum,
prepared-prefix cache, retained query graph or new source authority. Send uses
the first borrowed destination as its existing common workspace, then writes
that common send to each destination without another owned PCM allocation.

The existing scalar `finish` signature wraps the new implementation with one
destination. Its final addition order and work charges are retained. Additional
destination validation, comparisons and walks are charged. Callback errors,
cancellation and work refusals propagate; partially written scratch is discarded,
never retried after doing prefix work. Limits and source admission are unchanged.

Six tests cover exact stereo/signed-zero/rounding order; 48 combinations with four
independent scalar oracles each (192 comparisons) through constant and dynamic
batch engines; one common callback sequence and less actual work than two scalar
passes; rejection of a later invalid destination before mutation; unused versus
required invalid final gains; and callback/cancellation/exact-work boundaries.
The fixture includes gain order, mute, send, skip/stop, aliases, absent voices and
merged suppression. Independent source review found no supported defect.

## Verification

Same qualified macOS/M5 Max/Rust/FFmpeg/ORT environment as the preceding index
experiment, locked dependencies, opt-level 2, overflow checks and debug assertions.

| Check | Result |
| --- | --- |
| Main three-package build | Pass, 85.07 seconds. |
| Audio library | 208 pass, zero ignored. |
| Audio definition | 171 pass, zero ignored, 258.48 seconds. |
| Complete real-WAV stages target | 207 pass, three fail, zero ignored, 609.73 seconds: all 203 prior cases pass, plus four of seven unfinished Split cases. |
| Strict check | `cargo xtask check --no-test`: formatting and strict all-target Core/Plan/Audio/Store lint pass. |
| Isolated six-file build | Pass, 84.88 seconds. |
| Isolated Audio library | 191 pass, zero ignored. |
| Isolated historical birth/processing PCM | Eight pass, zero ignored, 9.58 seconds. |

All three remaining Split cases refuse with `Stage(Limit("plan work per read"))`:
Source/Sequence `cut-three`, limited `0+511`; nested selected play without root
sound `prefix-two`, limited `0+4096`; and one selected play at that same prefix
and window. The cases and boundaries match the preceding experiment; two prior
errors were child query limits rather than this exact error. Processed root
refinement remains passing. No comparison completes for the three refused reads.

The main run guards 1,397 input files. The isolated copy starts with 1,167 inputs
matching the published-runtime placement receipt, then changes exactly four
tracked files and adds two files; all six match main. Its 1,169 inputs exclude the
inherited general-Split/index work. All receipts are terminal and source/binary
hashes are stable before and after execution. Main Audio, definition and stages
executable SHA-256 values respectively:

```text
21193011a0109fd8f7afda92f473917c87bc5d5b000e510d834df2ec4494f0e9
4b9cde2f59e9dccbc78159d0c8517f3684b5059c49f4e34b0bf68abd4f1412c1
3096c6735850ca4164a3e5d7172da8b5d07a3a3b83f95f8291e220af27f3976c
```

No native UI changes or GUI claim. Core/Plan tests, whole-product gates and
packaging are not rerun for this backend primitive. No requirement or gate closes.

## Continuation and evidence

Runtime batching needs a conservative structural demand/resource summary before
allocating simultaneous histories. An authored effect count or observed warm
peak is insufficient: instantiated clocks, permissions, interval support,
projected inputs, future retained tiles and recursive scratch all matter. The
retained resource audit proposes batching one current group while descendants
stay scalar; that policy is not yet implemented or qualified. Never revive the
rejected scalar-prefix snapshot/sibling-cache prototype.

A separate ownership audit identifies genuinely freed Rust wet-constructor
coefficient/network storage. Keep full preparation admission during construction
and the entire existing native filter bound afterward. This possible lifetime
correction has no implementation or batching acceptance in this milestone.

Raw records remain in `/tmp/deadpan-components-outputs-2026-10-27`. The
[inventory](../../tools/media-qualification/evidence/2026-10-27-component-outputs/inventory.json)
retains the exact six-file patch, four before-files, independent review, scalar
oracle notes, source/binary pins, isolated lineage, commands, output, failures and
resource research. All 51 gzip payloads verify stored and expanded hashes:
1,441,349 stored bytes, 5,973,873 expanded bytes. Inventory SHA-256:
`b5c537eaad3e75ece858c7f5f179c74d28cbed4cad3d512493b169791b89e3d1`.
