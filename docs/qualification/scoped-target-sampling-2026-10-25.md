# Sampled target framing in an explicit play

This increment admits saved-target Smash and Creep over exactly contained Time
ranges with This play selected at every enclosing Repeat. It retains the root
Edit interval through fractional Retime mapping and commits framing plus nested
occurrence isolation in one transaction. It introduces no document format or
semantic-program extension.

## Contract

- Smash samples the target on the actual root Edit In picture. Creep samples
  Out minus one frame. Neither substitutes the stopped cursor or rounded owner
  coordinates for that picture.
- Projection uses that provider's exact source point, retained Hold geometry
  and inner framing. Owner and ancestor framing are not applied twice.
- The sampled center is fixed, including a Time range covering the whole owner.
  A newly sampled range never silently becomes live Follow. Existing Follow
  Keep-creep behavior, outside-path refusals and sampled-target recording
  refusals remain.
- Creep preserves the prefix, interpolates over the selected window and holds
  its arrival pose afterward. Interior interpolation uses the framing Q32 grid;
  the held suffix retains the exact authored terminal scale, here `11/5`.
- Native frame motions and End admit the selected play's exclusive Out. At an
  internal seam the viewer shows the right-hand picture. Finish the range and
  step back inside before framing when that picture does not show the owner.
  Finished selection endpoints stay fixed while the cursor moves.
- Missing, clipped or out-of-span targets, stale command captures, partial
  fractional boundary support and unavailable occurrence authority refuse
  without a partial edit.
- Detected targets compose through explicit Camera proposal acceptance, save,
  Escape, range recreation and a named saved-target command. Direct scoped
  `face:N`/`region:N` commands remain unavailable. Scripted detector observations
  exercise native admission and authored saves; they make no detector-accuracy
  claim.

## Independent fixture checks

The native fixture has two outer plays and three inner plays, gaps at both
levels, and a 36-frame Retime mapping 24 source frames. Its complete edit is
240 frames at 30000/1001 and 384,384 stereo sample frames.

For the selected owner `[164,200)`, the Time range is `[170,188)`. Smash In
samples a 30% center, Creep Out minus one samples 70%, and the stopped Out
picture would sample 40%. The oracle uses fixture arithmetic and integer Q32
interpolation rather than the production owner map or framing builder. It
checks every picture/provider/context/layer, the complete audio plan, original
nodes, clone identities and both isolation edges. Partial forward/backward and
full-owner forward/backward ranges are included. One Undo removes the framing
and isolation together; reopen/Redo retains the exact authored state.

Seven focused units separately check root In/Out minus one versus cursor,
fractional Retime source points, captured Hold plus live inner geometry,
missing/clipped/out-of-span refusal, full-owner fixed samples, preserved Follow
and outside-path refusals, and interpolation versus exact terminal scale.

## Verification

The initial corrected debug replay passes 530 scoped checks plus the live
Kestrel audit. Its App SHA-256 is
`2750c4a0edc1d346689ca9efd61f2f23cd616ca5babe7d0fbc8d8672f204272d`.
The two named 960×640 inspector/help captures were inspected. The selected
normal lane passed 111 tests and both strict lint configurations. The UI lane
passed 240 tests with one generated-reference failure; after regeneration,
the reference and the formerly leaky passing audio-setters test both pass in
an isolated same-build continuation without the leak report. Final strict
normal/UI lint, formatting and all eight focused/reference tests pass.

The release visual run at 60 Hz passes 530 workflow checks; the separate quiet
120 Hz run passes 529. Each also passes the live Kestrel audit, on the same App
SHA-256 `d17c384dabadcae98e78d0296923e0a15394fa5172e963cf3c7ea29339ff94ba`.
Final named inspector/help captures were inspected at 960×640. Screenshot-budget,
second-layout-retry and consecutive-new-input retry warnings remain in the raw
reports; no failed paint/retry assertion is hidden. These offscreen runs do not
qualify physical display timing or the broader performance targets.

The driver uses the retained native-authored package and obtains the static
control with genuine Undo in a disposable portable copy. It authors no recipe.
The read-only helper is linked only against artifacts from the successful final
Cargo JSON stream, with executable and library hashes retained.

| Check | Result |
| --- | --- |
| Exact provider, source point, context and framing oracle | All 240 pictures pass |
| Actual native Creep export and static Undo control | 240 pictures each; zero failed pictures/audio windows; measured audio offset zero |
| Minimum luma/chroma PSNR, both exports | 55.2405 / 53.5930 dB |
| Complete authored/limited stereo PCM and limiter-gain records | All 384,384 sample frames bit-identical using different block partitions |
| Nontrivial audio support | 19,578 nonzero sample frames; peak 0.4698212444782257 |
| Shared canonical reference pixels | Exactly frames 170–199 change; the other 210 pictures are bit-identical |
| Emitted unchanged region versus static reference | All 210 pictures pass |
| Changed-region negative controls in both directions | All 30 pictures fail at unchanged verification thresholds |
| Independent FFmpeg 9.0.2 decode | All 240 emitted pictures compared per plane against each reference |
| Portable history | All columns of 69 source revisions and 33 history records retained through Undo/Redo, checkpoint and fresh-process reopen |
| Source-package preservation | Every persistent path, byte count and SHA-256 unchanged, including nonempty SQLite WAL and media |

Ten unchanged emitted pictures have codec residual differences; reference
identity and passing verification are separate claims. Repeated pictures also
leave some index-neighbor comparisons unobservable; no artificial neighbor
margin substitutes for the complete exact provider oracle. The root SQLite SHM
is separately hash/size observed with no persistence assertion.

An independent metadata oracle checks all original audio lineage, bindings and
timing tables, exact transparent-clone lineage seeding, historical aliases and
live ancestor/birth remapping. All 14 deliberately corrupted metadata controls
are rejected. The final driver and this addition received independent static
review without actionable findings. Independent retained-evidence audit passes
all stored/decompressed hashes, source/binary pins, output controls, source
preservation and portable history. A separate current-tree check matches all
2,206 qualified source pins. The collection has 269 payload files and 546
hash/size-ledgered omissions, retaining 599,504,325 source bytes in 55,498,951
stored bytes. Inventory SHA-256:
`fa6eda8871768845926194c256245af4538bbff642c2ba460a2cade9cce104c9`.

## Strict DP-08 closure

The earlier DP-08 source/evidence reconciliation found this scoped saved-target
composition to be the remaining local blocker. Existing qualifications cover
keyboard point/rectangle and proposal saves, live Follow and parameter editing,
chosen-boundary returns, targeted escalation transforms, cutaways, reusable
off-center freezes, black punctuation and captions. This qualification closes
the missing contained-range composition, including the explicit detector-save
workflow. DP-08 is Complete under §29.1. Its eight coarse criteria already had
full credit, so coverage stays 84.5238%, rounded to 85%, while strict completion
rises to 10/24. All seven delivery gates remain open. General scoped Split and
structural/audio transforms remain DP-04/05, framed-child Ungroup DP-03.3, full
effects DP-16 and performance DP-24; their scope and scores do not change.

## Retained failures

The evidence preserves the fixture's initial `Workspace`/`Arc<Workspace>`
compile mismatch; two unit-oracle failures that used frame starts instead of
actual picture centers; the first native oracle's incorrectly quantized held
terminal scale; and the native full-owner endpoint failure that exposed the
scoped cursor clamp. The two intentional command-reference generation panics
and intervening stale-reference failure remain recorded. The first media run
also rejected legitimate isolation additions to audio lineage and bindings;
the corrected exact metadata oracle and its tamper controls replace that
incorrect unchanged-table expectation. The earlier driver is reconstructed
byte-for-byte against its preexisting source hash, with that provenance explicit.
Corrected runs do not erase those attempts or relax picture/audio thresholds.

The [evidence collection](../../tools/media-qualification/evidence/2026-10-25-scoped-sampling/)
is frozen after its final audit. This is primary-Mac qualification, excluding
physical key/OS IME delivery, display timing, acoustic listening, independent
DSP/renderer validation and arbitrary detector accuracy.
