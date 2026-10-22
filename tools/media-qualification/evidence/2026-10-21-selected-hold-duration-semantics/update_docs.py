from pathlib import Path

def once(text, old, new):
    assert text.count(old)==1,(old,text.count(old))
    return text.replace(old,new,1)

p=Path('docs/REQUIREMENTS.md')
s=p.read_text()
section='''## Selected-play Hold duration semantics, 2026-10-21

[Selected duration qualification](qualification/selected-hold-duration-semantics-2026-10-21.md)
adds one atomic duration edit for 2..=1024 captured Hold bodies across one
selected Repeat axis, retaining later Default/Play choices. Actual staged
isolation maps every counterpart and the final primary cursor; no-op members
allocate nothing, and opposite changes can leave total duration unchanged.
Native controls, recording, calls and direct-edit dot use invocation-relative
length intent. Browsing revokes mapped selection, requiring explicit reselection.
The valid old-code entry witness fails; all 15 focused, 3,446 broader,
196 affected app and 198 UI-feature checks pass, with strict lint and the
generated command reference. Both final release replays pass 1,318 scoped
checks plus Kestrel; three compact captures were inspected. Quiet input-to-state
p99 is 1.418ms. Four release exports verify all 240 pictures and 384,384 audio
samples at measured offset zero. Retained compiler, fixture, capture and replay
corrections include the intentional mapped-selection revocation. Mixed/selected
owned gaps, Repeat/Retime combinations and wider selectors remain open.
DP-06, DP-07 and DP-24 stay partial; no requirement or gate is newly complete.

'''
s=once(s,'## Mixed Repeat Hold duration semantics, 2026-10-21\n',section+'## Mixed Repeat Hold duration semantics, 2026-10-21\n')
s=once(s,'offset zero. Several-play durations, mixed gap timing\nand Repeat/Retime temporal combinations remain open.','offset zero. At that checkpoint, several-play durations, mixed gap timing\nand Repeat/Retime temporal combinations remained open.')
old='preserve literal Default/Play choices, actual isolation and dormant authoring through native controls, recording, calls and direct-edit dot.'
s=once(s,old,old+' [Selected-play Hold lengths](qualification/selected-hold-duration-semantics-2026-10-21.md) map every captured counterpart through staged edits and named calls, resolve the final primary boundary after all leaves, and retain duration-only dot intent for an explicitly reselected batch.')
old='verifies all 240 emitted pictures and 384,384 audio samples at zero offset.'
s=once(s,old,old+' [Selected-play duration](qualification/selected-hold-duration-semantics-2026-10-21.md) resizes captured Hold bodies in one atomic history entry, preserving each physical audio clock and mapped counterpart even when total duration is unchanged; four selected-body release exports also verify all 240 pictures and 384,384 audio samples at zero offset.')
p.write_text(s)

p=Path('docs/spec/AGENT_HANDOFF.md')
s=p.read_text()
section='''Selected-play Hold duration now captures 2..=1024 Hold bodies with one varying
explicit Repeat axis and identical earlier prefixes. Every changed member uses
its exact staged duration path; actual leaf proofs map all targets and the
primary presentation. Final continuation resolves only after the whole
Compound, including zero-net duration changes. Native Apply, recording, named
calls and direct-edit dot retain duration intent. Browsing revokes mapped
selection; explicitly choose a new play set before dot on another pair.

All 15 focused, 3,446 broader, 196 affected app and 198 UI-feature checks pass,
with strict lint and the generated command reference. Both final native release
replays pass 1,318 scoped checks plus Kestrel; three compact captures were
inspected. Quiet input-to-state p99 is 1.418ms. Four release exports verify all
240 pictures and 384,384 presented samples at measured offset zero.
See [selected qualification](../qualification/selected-hold-duration-semantics-2026-10-21.md)
for the valid old-code witness, retained corrections, review, source/binary
identities and full evidence. Next: mixed/selected owned-gap duration, then
remaining Repeat/Retime combinations and selectors. The owned-gap design and
unexecuted old-code witness are in
/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21; positively dormant
all-Play members also need the reducer's old-boundary assumption addressed.
No requirement or gate is newly complete.

'''
s=once(s,'## Current continuation, 2026-10-21\n\n','## Current continuation, 2026-10-21\n\n'+section)
s=once(s,'Several-play durations, mixed gap timing and Repeat/Retime temporal combinations\nremain the next editing work.','At that checkpoint, several-play durations, mixed gap timing and Repeat/Retime\ntemporal combinations remained the next editing work.')
p.write_text(s)
