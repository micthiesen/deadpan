from pathlib import Path
p=Path('/tmp/deadpan-fixed-retime-owned-sound-2026-10-21/output_owned.rs')
s=p.read_text()
def replace(old,new):
    global s
    assert s.count(old)==1,old
    s=s.replace(old,new)
replace('fn output_sound(previous_journal: bool) -> (ProjectDocument, SoundId) {\n    let before = processing(PitchPolicy::Preserve, 1, false);',
'''fn output_sound(previous_journal: bool, grouped: bool) -> (ProjectDocument, SoundId) {
    let mut before = processing(PitchPolicy::Preserve, 1, false);
    if grouped {
        before.nodes.insert(
            id("group"),
            BeatNode::sequence("Group", vec![id("processing"), id("tail")]),
        );
        before.nodes.insert(
            id("root"),
            BeatNode::sequence("Root", vec![id("lead"), id("group")]),
        );
        before.validate().unwrap();
    }''')
replace('fn assert_fixed_output_resize(length: i64, previous_journal: bool) {\n    let (before, effect) = output_sound(previous_journal);',
'''fn assert_fixed_output_resize(length: i64, previous_journal: bool, grouped: bool) {
    let (before, effect) = output_sound(previous_journal, grouped);''')
replace('''        assert_eq!(journal.scope(), &id("processing"));
        assert_eq!(journal.clocks().len(), 1);
        let clock = &journal.clocks()[0];
        assert_eq!(clock.scope(), &id("processing"));''',
'''        let scope = id(if grouped { "group" } else { "processing" });
        assert_eq!(journal.scope(), &scope);
        assert_eq!(journal.clocks().len(), 1);
        let clock = &journal.clocks()[0];
        assert_eq!(clock.scope(), &scope);''')
replace('''    assert_eq!(
        after.audio_bindings().sound_clocks(),
        before.audio_bindings().sound_clocks()
    );''',
'''    if grouped && previous_journal {
        // This valid older capture includes the surrounding Sequence group.
        // The output-only proof narrows its dependency without inventing a new
        // clock or changing the historical owner, timing identity, or layout.
        let old = &before.audio_bindings().sound_clocks()[&id("processing")][&effect];
        let new = &after.audio_bindings().sound_clocks()[&id("processing")][&effect];
        assert_eq!(new.scope(), &id("processing"));
        assert_eq!(new.clocks().len(), old.clocks().len());
        for (old, new) in old.clocks().iter().zip(new.clocks()) {
            assert_eq!(new.scope(), &id("processing"));
            assert_eq!(new.owner(), old.owner());
            assert_eq!(new.timing(), old.timing());
            assert_eq!(new.repeats(), old.repeats());
            assert_eq!(
                after.audio_bindings().timings()[new.timing()],
                before.audio_bindings().timings()[old.timing()]
            );
        }
    } else {
        assert_eq!(
            after.audio_bindings().sound_clocks(),
            before.audio_bindings().sound_clocks()
        );
    }''')
for a,b in [('3, false','3, false, false'),('1, false','1, false, false'),('3, true','3, true, false'),('1, true','1, true, false')]:
    replace('assert_fixed_output_resize('+a+');','assert_fixed_output_resize('+b+');')
replace('''#[test]
fn fixed_retime_descendant_owned_sound_keeps_changed_input_refusal()''',
'''#[test]
fn fixed_retime_output_owned_sound_under_sequence_group_grow_ignores_child_dependency() {
    assert_fixed_output_resize(3, false, true);
}

#[test]
fn fixed_retime_output_owned_sound_under_sequence_group_shrink_narrows_existing_journal() {
    assert_fixed_output_resize(1, true, true);
}

#[test]
fn fixed_retime_descendant_owned_sound_keeps_changed_input_refusal()''')
p.write_text(s)
