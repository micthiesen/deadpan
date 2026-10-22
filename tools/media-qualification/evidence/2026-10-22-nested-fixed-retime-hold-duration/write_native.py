from pathlib import Path
s=Path('crates/deadpan-app/src/preview/harness/scoped_plays/this_play_fixed_duration.rs').read_text()
s=s.replace('//! Native This-play duration editing beneath one fixed Edit Retime.','//! Native explicit middle/middle duration editing beneath nested Repeat bodies.')
s=s.replace('this-play-fixed-duration','nested-fixed-duration')
s=s.replace('This-play','Nested This-play').replace('This play fixed Retime','Nested This play fixed Retime')
s=s.replace('== 24','== 42').replace('"output_frames":24','"output_frames":42').replace('24-frame','42-frame').replace('!= 24','!= 42')
s=s.replace('sequence_cursor == 10','sequence_cursor == 22').replace('"Edit":10','"Edit":22')
s=s.replace('prior.target.repeats.len() == 1','prior.target.repeats.len() == 2')
s=s.replace('target.target.repeats.len() != 1','target.target.repeats.len() != 2')
s=s.replace('prior.target.repeats[0].branch,\n                RepeatEditBranch::Play { iteration } if iteration.ordinal == 1)', 'prior.target.repeats[0].branch,\n                RepeatEditBranch::Play { iteration } if iteration.ordinal == 1)\n            && matches!(&prior.target.repeats[1].branch,\n                RepeatEditBranch::Play { iteration } if iteration.ordinal == 1)')
s=s.replace('json!({"scope":"This play 2/3"','json!({"scope":"This play outer 2/3, inner 2/2"')
s=s.replace('after.target.repeats == prior.target.repeats','after.target.repeats[0] == prior.target.repeats[0]\n            && after.target.repeats[1].branch == prior.target.repeats[1].branch\n            && after.target.repeats[1].repeat != prior.target.repeats[1].repeat')
s=s.replace('"overrides":1','"overrides":2')
s=s.replace('.sum::<usize>()\n                == 1','.sum::<usize>()\n                == 2')
s=s.replace('d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L, Key::Enter])?;', 'd.chord(&[Key::G, Key::G, Key::Num2, Key::Num2, Key::L, Key::Enter])?;')
s=s.replace('    d.command("scope play 2")?;\n    d.key(Key::Enter)?;\n    d.key(Key::Enter)?;\n    d.key(Key::J)?;', '''    d.command("scope play 2")?;
    d.key(Key::Enter)?; // Outer selected play -> Inner Repeat.
    d.command("scope play 2")?;
    d.key(Key::Enter)?; // Inner selected play -> fixed Retime.
    d.key(Key::Enter)?; // Fixed Retime -> ordinary Body Sequence.
    d.key(Key::J)?;''')
s=s.replace('second stable play through one fixed Edit Retime','second stable play at both Repeat levels through one fixed Edit Retime')
s=s.replace('child: id("stage")?,\n            iterations: IterationOrder::new(rev("outer-order")?, 3)', 'child: id("inner")?,\n            iterations: IterationOrder::new(rev("outer-order")?, 3)')
s=s.replace('        let mut stage = BeatNode::sequence("Fixed stage", vec![]);','''        let mut inner = BeatNode::sequence("Inner repeat", vec![]);
        inner.kind = NodeKind::Repeat {
            child: id("stage")?,
            iterations: IterationOrder::new(rev("inner-order")?, 2).map_err(|e| e.to_string())?,
            gap: None,
            escalation: None,
        };
        let mut stage = BeatNode::sequence("Fixed stage", vec![]);''')
s=s.replace('(id("outer")?, repeat),','(id("outer")?, repeat),\n            (id("inner")?, inner),')
# Resolve the fixture's inner child before the Retime match.
s=s.replace('        let NodeKind::Retime {\n            child: body,','''        let NodeKind::Repeat { child, iterations: inner_iterations, gap: None, .. } =
            &doc.nodes()[child].kind else {
                return Err("Outer child is not an ungapped Inner Repeat".into());
            };
        let NodeKind::Retime {
            child: body,''')
s=s.replace('|| iterations.len() != 3','|| iterations.len() != 3\n            || inner_iterations.len() != 2')
s=s.replace('|| doc.nodes().len() != 9','|| doc.nodes().len() != 10')
s=s.replace('matches!(&node.kind, NodeKind::Repeat { .. }))\n                .count()\n                != 1','matches!(&node.kind, NodeKind::Repeat { .. }))\n                .count()\n                != 2')
s=s.replace('exactly one outer Repeat and fixed Edit Retime','exactly two nested Repeats and one fixed Edit Retime')
Path('crates/deadpan-app/src/preview/harness/scoped_plays/nested_fixed_duration.rs').write_text(s)
