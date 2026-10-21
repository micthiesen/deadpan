from pathlib import Path
import difflib
root=Path('/tmp/deadpan-scoped-audio-setters-2026-10-21/proposed')
changes={}
p=Path('crates/deadpan-core/src/semantic/planner.rs')
old=p.read_text()
anchor='''                        | SemanticInstruction::SetAudio {
                            change: crate::AudioChange::Step { .. }
                                | crate::AudioChange::Mute { .. },
                        }
'''
assert old.count(anchor)==1
new=old.replace(anchor,'')
anchor='''                        | SemanticInstruction::Call { .. }
                )
            {
'''
assert new.count(anchor)==1
new=new.replace(anchor,'''                        | SemanticInstruction::Call { .. }
                )
                && !matches!(
                    instruction,
                    SemanticInstruction::SetAudio { change } if scoped_gain::supports(*change)
                )
            {
''')
anchor='single-play macro authority supports only Zoom, whole-owner gain steps and mute state, freeze or black pause intent, Hold duration or sound and named calls'
assert new.count(anchor)==1
new=new.replace(anchor,'single-play macro authority supports only Zoom, whole-owner gain, mute, saturation, balance and EQ, freeze or black pause intent, Hold duration or sound and named calls')
changes[str(p)]=(old,new)
p=Path('crates/deadpan-core/src/semantic/planner/scoped_gain.rs')
old=p.read_text()
new=old.replace('//! Whole-owner gain steps and explicit mute state use each staged Play target.','//! Whole-owner audio treatment intent uses each staged concrete Play target.')
anchor='use super::*;\n'
assert new.count(anchor)==1
new=new.replace(anchor,'''use super::*;

/// These changes use whole-owner treatment values and introduce no range,
/// independent effect-tail routing or modal proposal coordinates.
pub(super) fn supports(change: crate::AudioChange) -> bool {
    matches!(
        change,
        crate::AudioChange::Step { .. }
            | crate::AudioChange::Mute { .. }
            | crate::AudioChange::Trim { .. }
            | crate::AudioChange::Saturation { .. }
            | crate::AudioChange::Balance { .. }
            | crate::AudioChange::Equalizer { .. }
    )
}
''')
anchor='''        if !matches!(
            change,
            crate::AudioChange::Step { .. } | crate::AudioChange::Mute { .. }
        ) {'''
assert new.count(anchor)==1
new=new.replace(anchor,'''        if !supports(change) {''')
new=new.replace('single-play audio macros support only whole-owner gain steps and mute state','single-play audio macros support only whole-owner gain, mute, saturation, balance and EQ')
anchor='''                if matches!(change, crate::AudioChange::Mute { .. }) {
                    "the beat already has that mute state; no edit was made"
                } else {
                    "the beat already has that gain and saturation; no edit was made"
                },'''
assert new.count(anchor)==1
new=new.replace(anchor,'''                match change {
                    crate::AudioChange::Mute { .. } => {
                        "the beat already has that mute state; no edit was made"
                    }
                    crate::AudioChange::Step { .. } => {
                        "the beat already has that gain and saturation; no edit was made"
                    }
                    _ => "the beat already has those audio treatments; no edit was made",
                },''')
anchor='''            if matches!(change, crate::AudioChange::Mute { .. }) {
                "single-play mute"
            } else {
                "single-play gain"
            },'''
assert new.count(anchor)==1
new=new.replace(anchor,'''            match change {
                crate::AudioChange::Mute { .. } => "single-play mute",
                crate::AudioChange::Step { .. } | crate::AudioChange::Trim { .. } => {
                    "single-play gain"
                }
                _ => "single-play audio treatments",
            },''')
changes[str(p)]=(old,new)
p=Path('crates/deadpan-core/src/semantic/planner/tests/scoped_gain.rs')
old=p.read_text()
new=old
for anchor in [
'''        AudioChange::Trim {
            gain: crate::GainDb::new(3000).unwrap(),
        },
''',
'''        AudioChange::Saturation {
            drive: Some(crate::GainDb::new(3000).unwrap()),
        },
''',
'''        AudioChange::Balance {
            balance: Some(crate::StereoBalance::new(50).unwrap()),
        },
''',
'''        AudioChange::Equalizer { recipe: None },
'''
]:
    assert new.count(anchor)==1
    new=new.replace(anchor,'')
changes[str(p)]=(old,new)
patch=[]
for name,(old,new) in changes.items():
    destination = root / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(new)
    patch.extend(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name,tofile='b/'+name))
(root/'production-and-obsolete-negative.patch').write_text(''.join(patch))
