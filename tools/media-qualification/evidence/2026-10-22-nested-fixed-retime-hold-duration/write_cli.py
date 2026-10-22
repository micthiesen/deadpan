from pathlib import Path
p=Path('crates/deadpan-cli/tests/preview_export/this_play_fixed_retime_duration.rs')
s=p.read_text()
s=s.replace('//! One selected Repeat play, isolated beneath a fixed unclocked Edit Retime.', '//! One explicit middle/middle occurrence beneath nested Repeat bodies and a fixed Retime.')
s=s.replace('    document: ProjectDocument,\n}', '    document: ProjectDocument,\n    isolation_steps: usize,\n    allocated_nodes: usize,\n}')
s=s.replace('        let command = Command::EditOccurrence {','        assert_eq!(required.marks, 0);\n        let command = Command::EditOccurrence {')
s=s.replace('            document: resolved.document.clone(),','            document: resolved.document.clone(),\n            isolation_steps: resolved.isolation().steps().len(),\n            allocated_nodes: required.nodes,')
s=s.replace('"this-play-fixed-', '"nested-fixed-')
s=s.replace('let ids = project.fresh(6)?;\n    let [root, repeat_id, stage, body, held, pad] =\n        <[NodeId; 6]>::try_from(ids).map_err(|_| "six fresh identities")?;', 'let ids = project.fresh(7)?;\n    let [root, outer, repeat_id, stage, body, held, pad] =\n        <[NodeId; 7]>::try_from(ids).map_err(|_| "seven fresh identities")?;')
s=s.replace('    nodes.insert(repeat_id.clone(), repeat);','''    nodes.insert(repeat_id.clone(), repeat);
    let mut outer_repeat = BeatNode::sequence("Three outer plays", vec![]);
    outer_repeat.kind = NodeKind::Repeat {
        child: repeat_id.clone(),
        iterations: IterationOrder::new(RevisionId::new(format!("{name}-outer-plays"))?, 3)?,
        gap: None,
        escalation: None,
    };
    nodes.insert(outer.clone(), outer_repeat);''')
s=s.replace('vec![lead, repeat_id.clone(), tail.clone()]','vec![lead, outer.clone(), tail.clone()]')
s=s.replace('let total = 19 + 3 * output;', 'let total = 19 + 9 * output;')
a=s.index('    let iteration = |document:')
b=s.index('\n    let before_final',a)
s=s[:a]+'''    let stable = |document: &ProjectDocument, node: &NodeId| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
            return Err("Repeat".into());
        };
        Ok(RepeatInstance {
            node: node.clone(),
            iteration: iterations.at(selected_ordinal).ok_or("selected play")?,
        })
    };
    // Read stable identities after Insert. Insert may re-key authored iteration seeds.
    let outer_play = stable(&before, &outer)?;
    let inner_play = stable(&before, &repeat_id)?;
    let mut target = PreparedTarget {
        instance: InstancePath {
            node: held.clone(),
            repeats: vec![outer_play.clone(), inner_play.clone()],
        },
        document: before.clone(),
        isolation_steps: 0,
        allocated_nodes: 0,
    };
    if already_owned {
        target = edit_play(&mut project, target.instance, 4)?;
        assert_eq!(target.document.duration()?.frames(), total);
        assert_eq!(target.isolation_steps, 2);
        assert!(target.allocated_nodes > 0);
    }
''' + s[b:]
s=s.replace('let selected_play_start = 16 + i64::from(selected_ordinal) * output;', 'let selected_play_start = 16 + 4 * output;')
s=s.replace('[20_i64, 36_i64, 40_i64]', '[20_i64, 44_i64, 60_i64, 76_i64, 88_i64]')
a=s.index('    assert_eq!(after_target.instance.repeats, target.instance.repeats);')
b=s.index('    let NodeKind::Hold { recipe } = &after.nodes()[&after_target.instance.node].kind',a)
s=s[:a]+'''    assert_eq!(after_target.instance.repeats[0], outer_play);
    assert_eq!(after_target.instance.repeats[1].iteration, inner_play.iteration);
    if already_owned {
        assert_eq!(after_target.instance, target.instance, "owned mapped identities stay stable");
        assert_eq!(after_target.allocated_nodes, 0);
        assert_eq!(after_target.isolation_steps, 0);
        assert_eq!(after.nodes().len(), before_final.nodes().len());
    } else {
        assert_ne!(after_target.instance.node, held);
        assert_ne!(after_target.instance.repeats[1].node, repeat_id);
        assert!(after_target.allocated_nodes > 0);
        assert_eq!(after_target.isolation_steps, 2);
    }
    assert_eq!(after.nodes()[&repeat_id], before.nodes()[&repeat_id], "default inner Repeat");
    assert_eq!(after.nodes()[&stage], before.nodes()[&stage], "default fixed Retime");
    assert_eq!(after.overrides()[&outer].len(), 1);
    let mapped_inner = &after_target.instance.repeats[1].node;
    assert_eq!(after.overrides()[mapped_inner].len(), 1);
    assert_eq!(after.overrides().values().map(|entries| entries.len()).sum::<usize>(), 2);
    assert_eq!(
        after.nodes()[mapped_inner], before_final.nodes().get(mapped_inner)
            .cloned().unwrap_or_else(|| before.nodes()[&repeat_id].clone()),
        "mapped inner Repeat retains its committed stable play order"
    );
''' + s[b:]
# Node equality fresh clone repeat iterations can differ? isolation clone preserves iteration IDs; correct.
s=s.replace('    let mut lengths = [3; 3];\n    lengths[usize::try_from(selected_ordinal)?] = final_length;\n    let pictures = shared_fixed_retime_duration::layout(output, lengths);','''    // Reuse only the independently sampled Repeat section. The lead and exterior
    // tail occur once, outside both Repeat operators.
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    for outer_ordinal in 0..3 {
        let mut lengths = [3; 3];
        if outer_ordinal == 1 {
            lengths[1] = final_length;
        }
        let inner = shared_fixed_retime_duration::layout(output, lengths);
        pictures.extend(inner.into_iter().skip(16).take(usize::try_from(3 * output)?));
    }
    pictures.extend((29..32).map(original));''')
s=s.replace('Three direct Repeat plays each show a fixed unclocked Edit Retime output{output} selecting child [0,8). Explicit selected play {selected_ordinal}', 'Three outer plays each contain three inner plays of a fixed unclocked Edit Retime output{output} selecting child [0,8). Explicit middle/middle play')
s=s.replace('the other two remain 3f', 'the other eight remain 3f')
s=s.replace('fixed Repeat length', 'fixed nested Repeat length')
s=s.replace('fixture.audio = (0..3)', 'fixture.audio = (0..9)')
Path('crates/deadpan-cli/tests/preview_export/nested_fixed_retime_duration.rs').write_text(s)
