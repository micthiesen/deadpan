# Native Move, 2026-09-30

An edited register can now enter Move in `:splice`. `m` switches Copy/Move,
`s` inspects the removal join, and `f` inspects the insertion join. Each join
has a local Before/Proposed comparison and audition window. Enter applies one
atomic `MoveRange` command and selects the complete moved forest in its
destination scope. The [atomic-move contract](../ATOMIC_MOVES.md) defines the
pre-edit coordinates, ownership and unsupported scopes.

This record covers the native implementation over backend commit
`21e714f3b191ed94b660837e6a5e4e1d432492e3`. It separates service, decoded-media,
pure comparison and rendered interaction evidence. Integrated, release and
isolated native results are recorded separately.

## Service and admission

Move is an explicit proposal operation. A current edited capture supplies the
source parent and refined half-open range; the destination remains in the saved
pre-edit clock. Preparation retains one exact `Command::MoveRange` request with
only the required split identities and timing allocation. It does not manufacture
a copied wrapper or perform independent delete and insert commits.

The service reports the source range, insertion range and removal join separately.
`Prepared::validate_result` verifies the complete contiguous destination forest,
including several direct children. Preview leaves history unchanged. Applying
the retained request commits once; duplicate completion returns the saved receipt.
The receipt carries the destination scope, first moved child and full moved range.
Undo restores the authored structure under a fresh revision.

Original registers, Move with Replace, stale source revisions and unsupported
interiors are rejected. An exact native no-op reports that the material is already
at that position and leaves Apply disabled. Boundary reparenting can still move
ownership without changing global time, retaining an empty donor group.

Copied source admission runs before destination preflight. A historical register
therefore keeps inspectable endpoints and remains valid for Copy even when it
cannot authorize Move. The store admits Move through its normal validated
command preparation, sealing the exact base and proposed document. There is no
historical-import exception for Move. The existing edited picture and PCM paths
consume that seal; ordinary Original proposals retain their stricter admission.

Worker regressions reject a substituted document or seal, cancellation, a changed
base identity and an old source revision paired with a new expected revision.
Closing the store revokes temporary edited views on warm and cold access;
reopening does not revive them. Already admitted ordinary committed Original
pictures retain their existing warm private-decoder behavior after close.

## Independent picture and PCM checks

The qualified 120-frame, 30000/1001 A/V fixture exercises both directions within
one Source. Moving `[20,30)` to saved frame 60 produces the independent old-frame
order `[0,20), [30,60), [20,30), [60,120)`. Moving `[70,80)` to saved frame 10
produces `[0,10), [70,80), [10,70), [80,120)`. Every proposed ordinal is checked
against these authored permutations.

At both changed sites, real decoded pictures are compared with direct Original
decodes for exact source identity, PTS, interpretation metadata and RGBA bytes.
The proposal and committed result also agree on framing and captured context.
Shuffled cold/warm reads exercise the existing `Work::EditedProposed` path.

Canonical limited PCM, limiter gain and context/suppression metadata match the
committed result around the joins and a known displaced impulse. Cold final-window
and reverse warm reads agree. A separate raw-PCM oracle uses the fixture's
48 kHz samples and absolute round-to-even boundary `B(f) = round_even(f * 8008/5)`;
its source phases come from authored islands, not from the generated plan or
timing bindings. These native fixtures cut on multiples of five frames. The
earlier backend tests provide the separate one-sample allocation evidence.

A cross-parent case moves two partial Sources and a complete five-frame Freeze
as three roots. Its independently authored frame mapping checks the removal,
group boundaries, insertion and Hold edges. The Hold identity, captured geometry
and owned context survive. Source and destination group framing retain their
own live clocks; moved audio receives the destination's -6 dB treatment while
retained audio keeps the source group's +6 dB treatment.

An independent root SoundEvent remains at root frame 30. Its recipe and routes
are unchanged, and an explicit PCM sum combines the new underlying picture
source's audio with the event's own source and gain. The sound does not follow
the moved visual material or inherit the destination group's gain. Join PCM and
pictures still match the commit. Root sounds can make Before/Proposed audio
different even where moved source samples correspond.

These tests prove decoded source bytes and canonical framing/context separately.
They do not compare final composed GPU pixels, device output or encoded files.
They add no decoded historical Generated-Hold Move fixture or model qualification.

## Paired-site comparison

Pure tests define each comparison independently. At removal, saved `[a,b)` pairs
with an empty proposed join. At insertion, the empty saved seam pairs with the
proposed inserted range. Prefixes preserve offset from the start; suffixes
preserve offset from the end; an interior facing an empty interval maps to its
join. Local context stops at the other changed site, project bounds and the
configured lead/follow limits. It does not audition the entire distance between
two remote joins.

Frame and sample comparisons use the complete absolute boundaries. For the
30000/1001 case moving `[2,4)` to saved frame 6, `B(2)=3203`, `B(4)=6406` and
`B(6)=9610`. A saved comparison sample 9510 maps to proposed sample 6306 by the
paired local clock; the physical source mapping can differ by one sample. The
comparison is not a universal promise of identical PCM, especially with root
sounds. Boundary-only reparenting preserves identical global frame and sample
coordinates. A stopped terminal cursor retains `B(N)` while its picture is
clamped separately to frame `N-1`.

## Review and corrected failures

Independent review found that a receipt arriving after its workspace could lose
the saved destination scope. Matching unconsumed receipts now restore that scope
even when the revision is already visible, rebuilding rows before selecting the
first child and full moved range. Replay covers workspace-before-receipt,
receipt-before-workspace, manual navigation and duplicate delivery. Selection is
installed only for the matching session, project and visible saved revision.
Saved-refresh failure retains the old workspace and explicit reopen guidance.

Review also found that comparison clamped a legal stopped terminal sample to
`B(N)-1`. The sample boundary is now retained; picture clamping remains separate.
Pure and replay checks cover temporal moves in both directions and reparenting
without a global timing change.

The first rendered attempt used supplied repeat flags without retaining held
keys. egui derives repeats from its own key state, so that fixture could deliver
fresh operation keys. It now holds modified initial presses across frames,
releases the modifier, then supplies repeats and explicit key releases. An earlier
structure assertion also incorrectly rejected every Retime: endpoint splitting
legitimately creates transparent partitions. The corrected assertion forbids an
artificial Sequence wrapper while allowing those partitions. These were fixture
corrections, not relaxed Move semantics. Initial compilation also exposed a
closure ownership error in control-label metadata, which was corrected.

Image inspection of the first rendered attempt exposed actual clipping at
960×640: the copied-slice timeline extended five points below the window.
Controls now wrap compactly, and the main-picture
labels and footer text are measured before allocating the viewer. The same text
geometry is painted, with space retained for both timeline rows. The passing
run checks complete painted bounds, clips and later opaque coverage. Its two
44-point timeline rows occupy y=532..576 and y=584..628 at 960×640, and
y=712..756 and y=764..808 at 1280×820. The second attempt then failed a harness
lookup that treated an accessibility Label value as a control label. Its corrected
lookup retains exact text, geometry and painted visibility checks. Earlier failed
outputs remain retained.

## Rendered interaction and measured checks

`visual-third` passes all **751 `place-slice` checks**, covering existing
Copy/Replace behavior as well as Move. The final label/capture replay,
`visual-final`, passes **752 placement checks**. The separate Kestrel audit exercises
**11,904 routing cases** and 62 reserved bindings with no conflicts or source
drift. The run uses real qualified endpoint decoding and offscreen Metal SDR
rendering. It completes in 38.76 seconds, including 13.10 seconds of build time;
the final replay takes 37.83 seconds. These debug replays are not release
performance results.

Final captures 132/133 show Move at minimum/default size; 134 shows the committed
full range, 135 the explicit no-op with retained endpoints, and 136 boundary
reparenting with full scopes. Capture 129 retains the minimum-size copied view.
Visual inspection found these states fit, with a 177-pixel minimum Move picture
height. The final labels use the supported word `to` instead of the missing
arrow glyph.

Move replay covers production selection/copy keys, forward/backward placement,
local endpoint refinement, both join inspections, independent replacement and
insertion targets, no-ops, unsupported interiors, historical captures after Undo,
multi-root commit and destination selection. It also covers held keys, counted
motion, focused buttons, synthetic IME composition, session replacement and stale
proposal/receipt delivery. Site changes preserve the proposal identity; changes
to operation, source or destination advance it.

Audition delivery is injected in this replay. Actual canonical PCM is tested
separately above. The replay does not qualify physical device listening, OS IME,
complete accessibility, physical-display color or native picker behavior. There
is no new dedicated replay witness for a picture already pending when switching
sites; existing presentation identity tests remain the evidence for that boundary.

| Retained run | Result | Source manifest SHA-256 |
| --- | --- | --- |
| `store-move` | Focused sealed Move preview test passed | `a98ac01d7477abc4268a47463efe955dab9981d795349fed50a9d3ca3ac94545` |
| `app-second` | 375 unit + 3 headless tests passed, none ignored | `a909ecccbc0b4abbbadfcb57b71e5d94280ddf9dec20e3130f318184ae730dba` |
| `app-ui-first` | 411 unit + 3 headless tests passed, none ignored | `848467577be998008e582f6c6a4128379f1aaaa0b0dac98fd772c6df1c5995b2` |
| `visual-third` | 751 placement checks + Kestrel audit passed | `8eb75b399917579d3ad1bf104937aedda7407372205499c2675d41fbe2d1cb10` |
| `visual-final` | 752 placement checks + Kestrel audit passed | `f982a84aa492b73ee9b2b4994f72eab498735735f458d52d40ac129f42acbfac` |

These Rust 1.97.1 locked checks represent their recorded source manifests. The
app test runs precede the final rendered-layout corrections and are not a final
whole-workspace gate. Scratch reports and failed attempts are under
`/tmp/deadpan-native-move-20260930`.

## Final integrated verification

All **2,807 locked workspace tests** pass with none failed or ignored, in
759.14 seconds. Formatting and strict all-target workspace Clippy including the
UI harness pass on Rust 1.97.1, taking 1.50 and 240.61 seconds. These full checks
and both final replays use source manifest
`f982a84aa492b73ee9b2b4994f72eab498735735f458d52d40ac129f42acbfac`.

Final help review corrected one sentence from saying Undo performs the move to
saying Move is an undoable edit. That is the only implementation-input difference
in final source manifest
`55397cf671e1f7416b24c72ab3bc85c91363fcbeaa5a1a978dc6e6ffcf92c32c`.
All **414 final app/UI-harness tests** pass over that frozen correction, with
none failed or ignored, in 42.62 seconds. Formatting and strict all-target
workspace Clippy also pass, in 1.51 and 14.54 seconds. The previous app test
invocation overlapped the text edit and is superseded for final-source claims.
The collector verifies the exact single-string difference and rechecks all
1,345 final implementation inputs.

Debug app links retain the existing oversized `__eh_frame` warning; no lint
suppression was added. The release build has no linker warning.
[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-native-move/README.md)
includes failures, source identities, complete logs/replays and native snapshots.

## Release performance

The final full release replay passes all **3,314 checks** with no findings or
failed/timed-out timing samples. It took 119.69 seconds including the optimized
build. The optional accepted generated-picture scenario remains explicitly
skipped without its separate fixture. Source manifest is
`f982a84aa492b73ee9b2b4994f72eab498735735f458d52d40ac129f42acbfac`;
executed binary SHA-256 is
`63dc53b1009dfb29e315a5eaefeccdf80610234b20934aa5b840503f2427cbf7`.

On Apple M5 Max/macOS 26.5.2, warm navigation through offscreen GPU completion
has p95 **1.504 ms**, with input CPU p95 **0.153 ms** across 120 samples.
Cached Repeat p95 is **5.525 ms** and Hold fallback p95 **5.604 ms**, each across
40 samples. Navigation CPU p95 is **0.358 ms** across 160 samples in the existing
10,000-Background/Silence-beat fixture. Targets are unchanged. Power/thermal
state and OS file caches are uncontrolled; the report preserves full distributions
and scope limits. Physical display, acoustic and large-media claims remain open.

The final log contains two uncorrelated FFmpeg diagnostics: H.264
`decode_slice_header error` before the placement PASS and AAC `get_buffer() failed`
before Room tone PASS. Neither appears in the earlier successful release run.
Read-only decoder review found cancellation/preroll or allocation-limit paths
that could produce them, but the messages carry no request identity and remain
unexplained. Room-tone replay does not run its audition PCM path. Keep these
diagnostics alongside the passing scenario evidence; do not relabel them as
proven harmless cancellation.

## Native outcome

A unique temporary bundle opened a private copy of the closed replay package
through Cmd+O. Its executable hash matches the final release replay. Native keys
captured `[20,30)`, opened placement at Edit 60 and chose Move. Removal comparison
showed slate 030 proposed and 020 saved; insertion comparison showed slate 060
saved and 020 proposed. Captions retained the separate exact Edit coordinates.

Local refinement to `[21,30)` followed by Escape changed no cells in any of the
20 SQLite tables. Reopening retained the original register. Enter created exactly
one revision/history entry and selected the complete result `[50,60)` while total
duration remained 120 frames. Undo restored every authored field except the
required fresh revision. Historical Move after Undo rejected with an explicit
older-revision reason; Enter could not commit, and switching to Copy remained
usable. That inspection also changed no database cells. Sixteen unrelated tables
remain identical through all snapshots, taken with SQLite's backup API.

Cmd+Q exited the temporary executable and released its writer lock. The reserved
Cursor QA window and Space were never inspected or changed. Native screenshots
were inspected for actual slates and captions; the visible-screen capture clips
the large window's right side, so full layout evidence comes from Metal replay.
This native check adds no listening, native IME or complete accessibility claim.

No DP requirement or delivery gate is complete. Cut-to-register, persistent or
named registers, role-only placement and temporal occurrence interiors remain
outside this native Move milestone.
