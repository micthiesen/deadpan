from pathlib import Path
audio = Path('crates/deadpan-audio/tests/stages/processing_hold_duration_repeat')
s=(audio/'this_play_fixed_retime.rs').read_text()
s=s.replace('//! One explicit Play can resize a Hold under its fixed Edit Retime without\n//! changing the other Repeat plays or the exterior audio owner.', '//! Two explicit Repeat choices isolate one fixed Preserve input without\n//! changing any of the other eight processing plays or the exterior owner.')
s=s.replace('this-play', 'nested-fixed')
s=s.replace('    wire["nodes"]["exterior"] =', '''    let mut top = BeatNode::sequence("Three nested families", vec![]);
    top.kind = NodeKind::Repeat {
        child: id("outer"),
        iterations: IterationOrder::new(RevisionId::new("nested-fixed-top").unwrap(), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["top"] = serde_json::to_value(top).unwrap();
    wire["nodes"]["exterior"] =''')
s=s.replace('json!(["outer", "exterior"])', 'json!(["top", "exterior"])')
s=s.replace('    InstancePath {\n        node: id(target),\n        repeats: vec![RepeatInstance {', '''    let NodeKind::Repeat { iterations: top_order, .. } = &document.nodes()[&id("top")].kind else {
        unreachable!()
    };
    InstancePath {
        node: id(target),
        repeats: vec![RepeatInstance {
            node: id("top"),
            iteration: top_order.at(1).unwrap(),
        }, RepeatInstance {''')
s=s.replace('fn this_play_fixed_retime_duration_preserves_fractional_input_phase_and_other_owners()', 'fn nested_fixed_retime_duration_preserves_fractional_input_phase_and_all_other_owners()')
s=s.replace("// Frame 4's exact root grid point is 6406.4 samples; the stage's\n        // physical entry is the input's PointCeil sample, 6407.", "// Frame 16 is exactly 25625.6 root mix samples. PointCeil 25626\n        // selects the middle/middle physical processor, independently of its input grid.")
s=s.replace('ratio(4 * 8008, 5)', 'ratio(16 * 8008, 5)')
s=s.replace('        let old_input =', '        assert_eq!(root_entry, 25626);\n        let old_input =', 1)
s=s.replace('let play1 = ntsc_boundary(4);', 'let play1 = ntsc_boundary(16);')
s=s.replace('let play2 = ntsc_boundary(8);', 'let play2 = ntsc_boundary(20);')
s=s.replace('let exterior = ntsc_boundary(12);', 'let exterior = ntsc_boundary(36);')
s=s.replace('ntsc_boundary(16));', 'ntsc_boundary(40));')
s=s.replace('// The isolated branch is Stage + operand Sequence + lead Hold +\n        // pause Hold + speech Source. The reusable shared nodes remain unchanged.\n        assert_eq!((needs.nodes, needs.marks), (5, 0));', '''// Outside-in: clone inner Repeat + its five-node Stage subtree (6),
        // then clone that Stage subtree for the selected inner play (5).
        assert_eq!((needs.nodes, needs.marks), (11, 0));''')
s=s.replace('ProjectFrame(5), ProjectFrame(7)', 'ProjectFrame(17), ProjectFrame(19)')
s=s.replace('ProjectFrame(5), ProjectFrame(6)', 'ProjectFrame(17), ProjectFrame(18)')
s=s.replace('        assert_ne!(grown.presentation.node, id("pause"));', '''        assert_eq!(grown.isolation().steps().len(), 2);
        assert_eq!(grown.isolation().steps()[0].nodes().len(), 6);
        assert_eq!(grown.isolation().steps()[1].nodes().len(), 5);
        assert_ne!(grown.presentation.node, id("pause"));''')
# Add explicit full intervals, in addition to contiguous prefix/suffix checks.
needle='        let grown_output = record(&grown.document, &mut provider, false, &[127, 3, 251]);'
s=s.replace(needle, needle+'''
        assert_other_outputs(&old_output, &grown_output);
        for (node, original) in before.nodes() {
            assert_eq!(&grown.document.nodes()[node], original, "retained template {node}");
        }
        assert_eq!(grown.transaction.inverse.apply(&grown.document).unwrap(), before);
        assert_eq!(
            ProjectDocument::from_json(&grown.document.to_json().unwrap()).unwrap(),
            grown.document
        );''')
needle='        let shrunk_output = record(&shrunk.document, &mut provider, false, &[127, 3, 251]);'
s=s.replace(needle,needle+'''
        assert_other_outputs(&old_output, &shrunk_output);
        assert_eq!(shrunk_output.samples.len(), old_total);
        same_recorded(
            &stage_input(&shrunk_plan, &mut provider, root_entry, &shrunk_stage, &[1, 239]),
            &shrunk_input,
            0,
            "shrunk fixed-Retime chunk parity",
        );
        assert_eq!(
            record(&shrunk.document, &mut provider, false, &[1, 239]),
            shrunk_output
        );''')
s += '''
fn assert_other_outputs(before: &Recorded, after: &Recorded) {
    // This literal equal-layout fixture has nine four-frame processor plays.
    // RoundEven belongs to root output; PointCeil above belongs to stage input.
    for (start, end) in [
        (0, 4), (4, 8), (8, 12), (12, 16),
        (20, 24), (24, 28), (28, 32), (32, 36), (36, 40),
    ] {
        let range = ntsc_boundary(start)..ntsc_boundary(end);
        assert_eq!(after.samples[range.clone()], before.samples[range.clone()],
            "complete unselected output interval {start}..{end}");
        assert_eq!(after.suppressed[range.clone()], before.suppressed[range],
            "complete unselected suppression interval {start}..{end}");
    }
}
'''
(audio/'nested_fixed_retime.rs').write_text(s)

store=Path('crates/deadpan-store/tests/generation_bundles/preparations/scoped_mixed_duration')
s=(store/'this_play_fixed_retime.rs').read_text()
s=s.replace('//! One selected Repeat Play under a fixed Edit Retime keeps admitted media,\n//! scoped intent, and the complete Compound history durable.', '//! Nested explicit Repeat choices keep admitted synthetic media, mapped\n//! preparation intent and a saved nested macro durable in one Compound.')
s=s.replace('this-play', 'nested-fixed')
s=s.replace('fn this_play_fixed_retime_macro_compound_keeps_admitted_media_scope_and_history_atomic()', 'fn nested_fixed_retime_macro_compound_keeps_admitted_media_scope_and_history_atomic()')
s=s.replace('    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["outer"]);', '''    let mut top = BeatNode::sequence("Three nested families", Vec::new());
    top.kind = NodeKind::Repeat {
        child: NodeId::new("outer")?,
        iterations: IterationOrder::new(RevisionId::new("nested-fixed-top")?, 3)?,
        gap: None,
        escalation: None,
    };
    wire["nodes"]["top"] = serde_json::to_value(top)?;
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["top"]);''')
s=s.replace('iteration: order.at(1).unwrap(),', 'iteration: order.at(0).unwrap(),')
s=s.replace('        repeats: vec![RepeatEditStep {\n            repeat,\n            branch:', '''        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("top")?,
            branch: RepeatEditBranch::Play {
                iteration: match &document.nodes()[&NodeId::new("top")?].kind {
                    NodeKind::Repeat { iterations, .. } => iterations.at(1).unwrap(),
                    _ => return Err("top Repeat required".into()),
                },
            },
        }, RepeatEditStep {
            repeat,
            branch:''', 1)
s=s.replace('        repeats: vec![RepeatEditStep {\n            repeat: NodeId::new("outer")?,', '''        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("top")?,
            branch: RepeatEditBranch::Default,
        }, RepeatEditStep {
            repeat: NodeId::new("outer")?,''', 1)
start=s.index('    let RepeatEditBranch::Play { iteration } = &target.repeats[0].branch')
end=s.index('\n}\n\nfn set', start)
s=s[:start]+'''    Ok(InstancePath {
        node: target.node.clone(),
        repeats: target.repeats.iter().map(|step| {
            let RepeatEditBranch::Play { iteration } = &step.branch else {
                return Err("explicit Play required".into());
            };
            Ok(RepeatInstance { node: step.repeat.clone(), iteration: iteration.clone() })
        }).collect::<Result<Vec<_>>>()?,
    })'''+s[end:]
s=s.replace('target(&store.snapshot()?, 2)?', 'target(&store.snapshot()?, 1)?')
s=s.replace('&NodeId::new("outer")?,\n            &instance,', '&NodeId::new("top")?,\n            &instance,')
s=s.replace('ProjectFrame(24), ProjectFrame(30)', 'ProjectFrame(48), ProjectFrame(54)')
s=s.replace('root: NodeId::new("outer")?,', 'root: NodeId::new("top")?,')
s=s.replace('selected_child: Some(NodeId::new("outer")?)', 'selected_child: Some(NodeId::new("top")?)')
s=s.replace('                    nodes: 4,', '                    nodes: 10,')
s=s.replace('        assert_eq!(after.overrides().len(), before.overrides().len());', '        assert_eq!(after.overrides().len(), before.overrides().len() + 2);')
start=s.index('        let outer = NodeId::new("outer")?;')
end=s.index('        for (node, original) in before.nodes() {', start)
s=s[:start]+'''        assert_eq!(after.duration()?.frames(), 108);
        assert_eq!(prepared.isolation().steps().len(), 2);
        assert_eq!(prepared.isolation().steps()[0].nodes().len(), 6);
        assert_eq!(prepared.isolation().steps()[1].nodes().len(), 4);
        let outer = NodeId::new("outer")?;
        let top = NodeId::new("top")?;
        let NodeKind::Repeat { iterations, .. } = &after.nodes()[&outer].kind else {
            panic!("inner Repeat retained")
        };
        assert_eq!(
            after.overrides()[&outer].get(&iterations.at(0).unwrap()),
            Some(&NodeId::new("owned")?)
        );
        let cloned_inner = &prepared.isolation().steps()[0].nodes()[&outer];
        let cloned_owned = &prepared.isolation().steps()[0].nodes()[&NodeId::new("owned")?];
        assert_eq!(after.nodes()[cloned_owned], before.nodes()[&NodeId::new("owned")?]);
        let NodeKind::Repeat { iterations: top_order, .. } = &after.nodes()[&top].kind else {
            panic!("top Repeat retained")
        };
        assert_eq!(after.overrides()[&top].get(&top_order.at(1).unwrap()), Some(cloned_inner));
        let cloned_stage = &prepared.isolation().steps()[1].nodes()[
            &prepared.isolation().steps()[0].nodes()[&NodeId::new("stage")?]
        ];
        assert_eq!(
            after.overrides()[cloned_inner].get(&iterations.at(1).unwrap()),
            Some(cloned_stage),
            "only the middle/middle Play receives the isolated Stage"
        );
'''+s[end:]
(store/'nested_fixed_retime.rs').write_text(s)
