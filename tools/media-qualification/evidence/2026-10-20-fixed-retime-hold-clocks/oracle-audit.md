# Fixed Retime finite-source oracle audit

2026-10-20. Read-only production investigation; test-only correction. Original `pcm.log` retained unchanged. No Cargo execution by this agent.

## Cause

`deadpan-audio/src/sequence.rs::source_recipe` intersects the exact current structural Source support with the authored media selection, taking ceil at both support endpoints before resampler I/O. The resampler zero-extends outside that finite selection. Consequently moving a Source against a fixed Retime crop changes its filter endpoint, even when its historical entry phase is correctly retained. Comparing the whole new clipped output against an old, longer Source prefix was invalid near the new crop. The shrink test also made this mistake BEFORE the duration command by comparing a cropped old Source against an uncropped reference.

`Resampler::radius` is128 source samples for step<=1 and ceil(step*128) otherwise. The full kernel includes taps -radius+1..radius. Its first potentially changed sample has floor(source_position)+radius >= new_selection.end. This explains the actual first differences exactly; no phase/alignment correction or tolerance is justified or added.

## Observed first differences

- Minimal growth: old phase3100, source step147/160. Current one-input-frame crop ends at ceil(3100+1471.47)=4572. First affected offset1463 has source floor4444; 4444+128=4572. Actual failure new11073 versus old9471 was [0.8974001,0.8974001] versus [0.8974002,0.8974002]. Independent first256 passed. The potential endpoint window is138 output samples (1463..1601).
- Shrink setup failure, before edit: old two-frame crop end6043 versus uncropped four-frame end8986. First difference offset3064, old output11072, source floor5915; 5915+128=6043. Values .17744906 versus .177449. All139 final positions differed; maximum recorded absolute difference .03129134. Current AFTER shrink support is three frames, ceil(3100+4414.41)=7515.
- Nested nonzero affine growth: combined slope3/4 gives source step49/40; radius ceil(128*49/40)=157. Current two-input-frame crop ends6043. First difference offset2275, new9482 versus old8281, floor5886; 5886+157=6043. Values .11468128 versus .114681266. There were127 bit-different positions among the128 remaining output slots; maximum absolute difference .4612686.
- Owned Repeat branch, first outer play: old rounded phase9000+147/400, step147/160, current one-input-frame crop end10472. First difference offset1463, new20682 versus old19081, floor10344; 10344+128=10472. Values .04530058 versus .04530056. All139 remaining positions differed; maximum absolute difference .3277556. Second outer play has literal old phase9000 at root25/sample40040 and uses the same current end10472. Its expected new interval starts root26/sample41642.

The vector differences above were extracted from bounded parsing of `pcm.log`; the log itself was not rewritten.

## Corrected assertions

Keep the independently literal first256 samples and every old-prefix sample whose sinc kernel is unaffected by the changed crop: minimal1463, shrink3064, nested2275, each owned branch1463. Then compare EVERY sample of the COMPLETE current allocation against a bounded independent `ResampleRecipe` carrying the literal historical phase and the CURRENT ceil source crop:

| Case | Current selected source samples | Historical phase at new rounded output origin | New output interval | Source step |
|---|---|---|---|---|
| Minimal grow |3100..4572|3100 at9610|9610..11211|147/160|
| Shrink old fixture |3100..6043|3100 at8008|8008..11211|147/160|
| Shrink after |3100..7515|3100 at6406|6406..11211|147/160|
| Nested affine |3100..6043|3100 at7207|7207..9610|49/40|
| Owned outer0 |9000..10472|9000+147/400 at19219|19219..20821|147/160|
| Owned outer1 |9000..10472|9000 at41642|41642..43243|147/160|

The oracle helper takes only these literal constants, the canonical provider and bounded chunks. It reads no plan, binding, current placement or duration state. All comparisons remain bit-exact. Current audible Source support is required to remain unsuppressed, Holds remain zero/suppressed, all unchanged outside PCM/suppression comparisons remain, and complete alternate chunk schedules remain equal. Preserve/Shift ancestors remain outside this test qualification.

## Retained discrete allocation endpoints (second focused run)

The source-crop correction passed minimal growth and shrink. Two remaining failures occurred only at the terminal live slot: nested output9609 was0 versus the unmasked native-source oracle .2497756; first owned outer output20820 was0 versus .23803729. These are a second independent boundary, separate from the native source sinc crop above.

`AudioBound::raw_domain` places the CURRENT local support on the OLD lattice. Its root grid uses round-even for both endpoints. `DomainSignalTransfer` then transfers this admitted discrete root signal with its half-open support; exact reference points outside it are exhausted and explicitly suppressed even if the native Source still has filter support. These samples must be zero, rather than newly sampled source data. This is the same discrete-allocation exhaustion contract exercised by the earlier ordinary root-ripple regression.

Let B(f)=roundEven(f*8008/5). These are literal authored origins, scales and current supports, with no plan-derived values:

| Case | Current local support on historical lattice | Historical discrete allocation | Live discrete allocation | Result |
|---|---|---|---|---|
| Minimal grow | origin5, scale1, local[0,1) | B(5)..B(6)=8008..9610,1602 samples | B(6)..B(7)=9610..11211,1601 samples | All live slots admitted; no final zero |
| Shrink | origin5, scale1, local[0,3) | B(5)..B(8)=8008..12813,4805 samples | B(4)..B(7)=6406..11211,4805 samples | All live slots admitted |
| Nested affine | origin15/4, scale3/4, local[0,2) | B(15/4)..B(21/4)=6006..8408,2402 samples | B(9/2)..B(6)=7207..9610,2403 samples | Final slot exhausted |
| Owned outer0 | origin11, scale1, local[0,1) | B(11)..B(12)=17618..19219,1601 samples | B(12)..B(13)=19219..20821,1602 samples | Final slot exhausted |
| Owned outer1 | origin25, scale1, local[0,1) | B(25)..B(26)=40040..41642,1602 samples | B(26)..B(27)=41642..43243,1601 samples | All live slots admitted; no final zero |

Nested derivation: (15/4)*(8008/5)=6006 exactly; (21/4)*(8008/5)=42042/5=8408.4 rounds8408. Live origin9/2 maps36036/5=7207.2 rounds7207, live end6 maps48048/5=9609.6 rounds9610. At output9609, retained reference6006+(9609-7207)=8408 equals the excluded historical end. Prepare2402 current-crop samples with source selection3100..6043 and old phase3100, then require one independently derived zero and true suppression mask at9609.

Owned outer0: B(11)=88088/5=17617.6 rounds17618, B(12)=96096/5=19219.2 rounds19219, B(13)=104104/5=20820.8 rounds20821. Output20820 references17618+(20820-19219)=19219, exactly the excluded historical end. Prepare1601 current-crop samples with source selection9000..10472 and old phase9000+147/400, then require zero and true mask at20820.

Owned outer1: B(25)=40040, B(26)=208208/5=41641.6 rounds41642, B(27)=216216/5=43243.2 rounds43243. Output43242 references40040+(43242-41642)=41640, strictly below41642. The entire1601-slot live allocation remains audible/unsuppressed, including43242. The partial-binding branch has the same authored Source geometry and phases, so it has the same independently expected lengths and masks.

The tests now assert the literal historical/live lengths explicitly, check every live PCM sample and suppression flag, assert the two terminal zero/true pairs separately, assert the second outer terminal is NOT suppressed, and retain unaffected prefixes, outside PCM and alternate chunk schedules. No source-selection endpoint or phase constant changed, no tolerance was added, and no observed actual output was used to infer the expected length.
