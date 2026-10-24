# Target-following creep, 2026-10-24

The implementation, listed local checks and independent evidence verification
pass. DP-08 and the full normative goal remain open.

## Implemented behavior

A whole ordinary Source with a fixed Follow accepts Keep-center `:creep` and
`,c`. One optional `FollowScaleTransition { factor, curve }` changes scale on
the owner's declared clock while the center follows the target at the actual
provider picture time. Explicit target creeps keep their existing sampled-point
behavior. Visual ranges, including full-length ranges, and occurrence or other
owner authoring retain explicit refusals. This is an incremental DP-08 change;
broader authoring and remaining target controls stay open.

Live and fallback scales have independent bases and terminal products. The
existing Q32 round-even interpolation applies to their actual endpoint scales.
Fallback centers and exact endpoint scales remain exact; resolved live poses
retain whole-pose quantization. Lost samples supply their retained region while
scale continues changing. Existing projection, span and captured-Hold fallback
rules remain. Raw validated transitions work on the supported owner types.

Camera retains the complete entry, scales both bases proportionally and checks
both terminal endpoints. Explicit `t` releases the installed pose to Static and
announces removal of animation. Escape restores entry geometry. Target saves and
switches preserve the draft transition and clock. Ordinary Keep creep records
semantic Zoom intent for invocation-relative named replay and direct-edit dot.

The optional absent field is omitted from existing JSON. Core 58 / SQLite 87
remain, following the additive-field convention. No Store, renderer, tracker,
dependency or structural algorithm changes are needed. Aggregate admission
charges a fixed Follow as one record and an animated Follow as two, including
escaped document keys. Command, subtree, slice and stored-patch validation keep
their own actual boundaries; document preflight is not claimed for every
structural ingress.

## Executed checks

These lanes overlap; their counts are not a sum of unique tests.

| Lane | Result |
| --- | --- |
| Complete first Core/Plan run | 2,020 passed; none ignored. |
| Focused Core Follow and scanner tests | 26 passed; 711 filtered. |
| Framing ingress, aggregate slice and patch | 23 passed; none ignored. |
| Follow plan and structural tests | 18 passed; 103 filtered. |
| App binary unit tests | 1,294 passed; 11 existing opt-in tests ignored. |
| CLI library unit tests | 532 passed; six existing opt-in tests ignored. |
| CLI framing integration | 12 passed; 46 filtered. |
| Strict all-target Core/Plan/CLI/App Clippy | Passed, including `ui-harness` and `synthetic-worker`. |
| Release native zoom replay | 213 workflow checks and one Kestrel check passed. |
| Actual release-media case | First and final fresh builds each pass one case; 66 filtered; no skip. |

Structural witnesses cover independent raw Step/Linear/Smoothstep poses,
nested Source/Repeat/Sequence clocks and gaps, raw Retime output versus mapped
provider time, captured Hold fallback and provider changes, Split, crop/copy,
Duplicate, actual Trim/Roll/Slip, occurrence isolation/Explode and inserted
captured freezes. Forward/inverse and JSON roundtrips preserve the relevant
fields. Source-edit tests compare exact source points and audio-plan mappings;
they are not separate emitted PCM runs for every structural operation.

CLI cases exercise explicit `from=` proportional rebasing, new target/base
resolution on replay, dry-run, durable Undo/Redo, portable copy, full and partial
Visual refusals, invalid live/fallback endpoints, missing targets and malformed
curves without document/history/register writes.

## Native behavior and appearance

The final release App is
`2eb7d88f3b56ade6d98e6a347ea7ee1f2fa7d1988a7e46607d66cef3ca26b5b5`.
Its replay has 766 semantic steps and 213 passing workflow checks. The local
Kestrel source matches the pinned fixture (`368c01df…`): 62 reservations,
21,885,504 routing cases and no conflicts. No binding changes are introduced.

The replay checks complete no-change Enter, valid proportional scale edits,
later-endpoint refusal, target-save continuation and switching, release/Escape,
exact installed-pose Static release, release-adjust-refollow, Reset, Undo,
recording, named replay and direct-edit dot. It also runs the existing zoom,
range, sampled-target and pending-entry cases.

Root inspected actual final offscreen Camera and editor pictures at 1280×820
(`zoom-049.png`, `zoom-118.png`). The picture remains dominant, the focused pane
and temporary Camera state are visible, and the inspector names the target,
both scale endpoints and curve. The Camera explains continuing fallback scale
and teaches explicit release. Primary Apply/Cancel controls and keyboard hints
remain visible. Secondary Camera controls use the existing scrolling inspector.
This is an offscreen native Metal/event replay, not a physical display, input
method or VoiceOver qualification.

Three warnings remain in the raw final report: intermediate screenshot capacity,
19 frames settling on their second layout retry, and ten runs of consecutive
retrying frames with distinct input causes. Ignored retries and repeated
same-cause loops fail the harness; both guards pass. These are not latency
measurements or a responsiveness-target pass.

## Real media and history

The qualified counter Original has 120 pictures at 30000/1001 and 192,192
stereo samples at 48 kHz. Deterministic target samples move, retain a Lost
region, accept a later correction and use fallback outside their span. Every
picture pose is compared with independent bounded integer/Q32 arithmetic.
There are 20 fallback, 60 tracked, 20 Lost and 20 manual-correction rows.

All authored/limited PCM and limiter-gain IEEE bit arrays match the pre-creep
baseline, including with different read partitions and after portable copy.
Both channels contain the retained click; 2,816 sample pairs are nonzero.
Undo/Redo and seven-revision replay/copy reports retain the complete authored
transition. These compare the existing canonical DSP against itself before the
picture-only edit; they are not an independent DSP oracle or listening check.

The emitted MP4 verifies all 120 pictures with no failures or nonzero picture
offsets. Its full stereo [0,192192) audio range has no uncovered samples and
`verified_zero` offset at zero samples. Final movie SHA-256 is
`fb5a58fbce268699889f67208433616e3aeb50d3f5fdbb7d81f84992fb5b6238`.
The first emitted movie remains retained separately (`171a9872…`). The final
fresh CLI is `e9ad52a23d3f91ac198007f18e69811d0ec09797e24c53230255f4e083150701`;
its four retained executable copies and test executable match their recorded
hashes. All eleven frozen runtime/harness source paths are unchanged before and
after the final build/run. The independent retained-result audits reread raw
reports, pose rows, PCM arrays, copy receipts, source and movie hashes. Target regions are authored over the
counter footage; this does not measure tracker accuracy on a moving subject.

## Failures retained

- Initial compile passes caught new test constructor omissions and Root test
  draft type/field/path errors. An initial exact test filter selected zero tests;
  it is not counted as verification.
- CLI assertion drafts initially expected one refusal string for both full and
  interior ranges, and supplied an extending selection with the wrong cursor.
  Correct inputs now reach and verify the actual distinct refusal paths.
- The first release native replay exposed a real Camera bug: restoring an
  already exact target center returned `None`, discarded the original scale
  change effect, and skipped complete validation/preview. The fix retains
  `PoseChanged` on successful unchanged restoration and preserves rejection;
  direct regressions and the final native replay cover it.
- The debug replay later expected dot after a named Run, contrary to the
  documented policy that authored named runs clear dot authority. Its corrected
  setup establishes a fresh direct Apply and uses Undo's preservation proof.
- A Trim/Roll oracle compared the entire Picture record even though trimming
  correctly changes its visible selection. It now asserts preserved asset,
  exact source point/full affine span/endpoints/poses and separately expected
  selection bounds and audio-plan mappings.
- The first broader CLI lane lacked a debug media helper. The corrected lane
  uses the already built release helper explicitly and passes all 12 cases.
- The debug App linker reports an oversized `__eh_frame` compact-unwind warning.
  It is retained separately from the passing strict checks and release build.
- The first real-cap slice capture hit the existing audio-binding capture
  complexity budget before framing admission. The final test asserts that
  independent refusal and tests standalone raw-reader admission separately:
  a small valid physical Hold/binding plus a zero-duration framed Sequence part
  admits exactly 100,000 records and rejects 100,001. It intentionally cannot
  prove capture provenance against the original small document. A draft that
  replaced the physical Hold with a Sequence correctly failed the physical
  audio-recipe invariant. The worker wrapper then used zsh's readonly `status`
  variable, so that attempt's exact Cargo exit was not captured (wrapper exit 1).
  Corrected worker and Root runs pass. No product budget is weakened.

## Retained evidence and review

[The evidence package](../../tools/media-qualification/evidence/2026-10-24-follow-creep/files.json)
has 185 inventoried payloads /7,389,224 bytes plus its inventory. Inventory
SHA-256 is `7d51eb530df1abb5577de6972f14f3147c6b91567aa917cc4941cfb147e0fcf2`.
It retains complete raw native reports, all failure/check streams, both actual
media trees, five safe project/media archives, thirty changed Rust snapshots as
`.rs.txt`, six build inputs and two selected final PNGs. Other screenshots and
derived HTML remain in task scratch. Ten ordinary payloads use gzip; archives
use tar+gzip and every member has a byte/hash ledger. Empty SQLite WALs and their
SHM files are omitted only with exact omitted hashes recorded. The first
collector run passed, with staging in task scratch and exclusive publication.

The independent verifier's first execution passes: all 185 payloads, 180 encoded
records, five safe TARs /82 regular members /33,432,800 authored bytes, fourteen
explicitly omitted sidecars, source/input/binary bindings and actual raw results
match. Primary bytes remain unchanged. The
[verification supplement](../../tools/media-qualification/evidence/2026-10-24-follow-creep-verification/files.json)
has ten payloads /51,827 bytes, inventory `548d3e18…`, and carries the verifier,
receipt, raw collector/verifier streams and a separate actual formatting
execution receipt. The final verifier receipt is
`564ad268a05ac43e5d5662f5dee85e089e0327b545de5d45501b7fde9e8ae4d6`.
Formatting's empty raw logs alone were not treated as an exit-status proof.

The current 2,062-file Rust map is
`05af18a4354a89d5528bd27ea8731630c71453d3de9829fb685a9ae35e00f3f8`.
It identifies the tree at collection, not the source of every earlier draft
build. The eleven-path runtime freeze is independently checked around the final
media execution. The preceding compound and one-leaf replay source files remain
byte-identical to their qualified versions (`e4b21f6a…`, `f2500485…`).

General and invariant reviewers found no additional confirmed production defect;
the runtime Camera failure and its correction are recorded above. Test reviews
led to independent Retime clock assertions, routed Camera cases and real
aggregate slice/patch/retained-union witnesses. The Lost-sentinel suggestion was
withdrawn after checking that Lost records intentionally carry the held region.
The final aggregate review reports no surviving finding. Standalone slice-reader
and patch-result validation are distinguished from capture provenance and early
allocation admission throughout.

Authored code/documentation diff checks pass. The complete staged diff reports
nineteen blank-at-EOF warnings in preserved raw stdout and the unchanged command
reference snapshot. Those evidence bytes are retained exactly rather than
trimmed to satisfy a source whitespace check.

No requirement, release gate, performance target or full-product acceptance
claim follows from this checkpoint. Coverage remains 89%, eight Complete and
sixteen Partial, while the full normative goal remains active.
