# Combined Trim audio and picture review

Root reviewed frozen patch
`b04555e2dfec39baccb10e3cca7eb8e3766ec48ade95786cf842b485a6edea55`
before integrating it. The patch adds three test modules and their module
declarations, with no production changes.

The mixed 48 kHz case retains A's symbolic resume and two chronological
reanchors, then independently checks four literal phases across distinct A/B
physical prefixes. The expectations distinguish the retained binding phase,
Slip's exact linked offset and permitted filter support. Both uneven chunk sizes
and a one-sample wrong-phase control are exercised. The Hard edge expectation
does not infer a fade from the implementation's returned spans.

The 44.1 kHz overwrite case checks a retained Preserve or Repeat operand after
partial consumption. Its full input is reconstructed with physical support
`[100,218)`; the cropped-support negative control uses `[159,218)` and must
differ. Preserve keeps the full stretched context. Repeat checks retained voice,
room-tone gap and the second voice. Entry and inherited opposite-edge fades use
literal sample-centered widths. Equality to the pre-edit tail supplements the
independent expected values and verifies fixed absolute placement.

The root-bus case starts with a previous Insert route and applies equal In/Out
in one command. It requires one new Trim record, exact silence around the old
gap and new gap, independent offset and root gain once, and the retained final
sample. A separate sequential Delete/Insert control is required to reach the
different one-sample phase. The shared reconstruction kernel is not independently
qualified by these expectations.

The indexed-picture cases use positive and negative source origins, irregular
PTS and literal source ordinals. They cover direct Sources and unity crops with
both prefixes, unchanged suffix pictures, and a surviving B containing only
terminal padding. That padding must select the last included source frame,
excluding the existing frame at the half-open selection end. These are indexed
plan tests, not native decoder or GPU image measurements.

The authoring resolver supplies resource counts, not expected PCM or picture
coordinates. Successful command fixtures check exact inverse restoration. No
concrete defect was found by this inspection. Runtime results are recorded
separately in the invocation logs and source inventories.
