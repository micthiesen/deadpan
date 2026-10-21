from pathlib import Path
import difflib
repo=Path('/Users/michael/Code/deadpan')
out=Path('/tmp/deadpan-scoped-mute-2026-10-20')
changes=[]
p='crates/deadpan-core/src/semantic/planner.rs'
s=(repo/p).read_text()
old='change: crate::AudioChange::Step { .. },'
new='change: crate::AudioChange::Step { .. } | crate::AudioChange::Mute { .. },'
assert s.count(old)==1
n=s.replace(old,new,1).replace('only Zoom, whole-owner gain steps, freeze','only Zoom, whole-owner gain steps and mute state, freeze',1)
changes.append((p,s,n))
p='crates/deadpan-core/src/semantic/planner/scoped_gain.rs'
s=(repo/p).read_text()
n=s.replace('//! Whole-owner gain steps resolve against each staged explicit Play target.', '//! Whole-owner gain steps and explicit mute state use each staged Play target.')
n=n.replace('!matches!(change, crate::AudioChange::Step { .. })', '!matches!(change, crate::AudioChange::Step { .. } | crate::AudioChange::Mute { .. })')
n=n.replace('single-play audio macros support only whole-owner gain steps', 'single-play audio macros support only whole-owner gain steps and mute state')
n=n.replace('the beat already has that gain and saturation; no edit was made', 'the beat already has those audio treatments; no edit was made')
n=n.replace('''            "single-play gain",
''','''            if matches!(change, crate::AudioChange::Mute { .. }) {
                "single-play mute"
            } else {
                "single-play gain"
            },
''')
changes.append((p,s,n))
patch=''
for p,s,n in changes:
 target=out/'proposed'/p
 target.parent.mkdir(parents=True,exist_ok=True)
 target.write_text(n)
 patch+=''.join(difflib.unified_diff(s.splitlines(True),n.splitlines(True),fromfile='a/'+p,tofile='b/'+p))
(out/'production.patch').write_text(patch)
