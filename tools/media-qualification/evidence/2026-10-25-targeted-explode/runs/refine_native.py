from pathlib import Path
p=Path('/tmp/deadpan-targeted-explode-2026-10-25/targeted_explode_native.rs')
s=p.read_text().replace('Keeps the view inside the canvas while following.','Keeps target view inside canvas above 100%')
s=s.replace('observations(&baseline.plan, &repeat, &source)', 'observations(&baseline.plan, &baseline.document, &repeat)')
s=s.replace('observations(&exploded.plan, &repeat, &source)', 'observations(&exploded.plan, &exploded.document, &repeat)')
s=s.replace('observations(&exploded.plan,&repeat,&source)', 'observations(&exploded.plan,&exploded.document,&repeat)')
s=s.replace('observations(&d.app().workspace.as_ref().unwrap().plan,&repeat,&source)', 'observations(&d.app().workspace.as_ref().unwrap().plan,document(d)?,&repeat)')
s=s.replace('observations(\n                &d.app().workspace.as_ref().unwrap().plan,\n                &repeat,\n                &source,\n            )?', 'observations(\n                &d.app().workspace.as_ref().unwrap().plan,\n                document(d)?,\n                &repeat,\n            )?')
s=s.replace('    repeat: &NodeId,\n    source: &NodeId,\n) -> Result<Vec<Value>, String>', '    document: &ProjectDocument,\n    repeat: &NodeId,\n) -> Result<Vec<Value>, String>')
s=s.replace('try_fold(0i64,|sum,owner|{','try_fold(ExactRatio::ZERO,|sum,owner|{')
s=s.replace('.map_or(0,|value|value.millidecibels);','.map_or(ExactRatio::ZERO,|value|value.millidecibels);')
s=s.replace('Ok::<_,String>(sum+owner.escalation_millidecibels()+clip)', 'sum.checked_add(ExactRatio::integer(owner.escalation_millidecibels())).and_then(|sum|sum.checked_add(clip)).map_err(string)')
s=s.replace('''            !layer.escalation && layer.pose.is_some() && layer.instance.node!=*repeat
                && layer.instance.node!=root && layer.effective_duration==ExactRatio::integer(120)
                && (layer.instance.node==*source||layer.local_position.compare(ExactRatio::integer(120)).is_lt())''','''            !layer.escalation && matches!(document.nodes().get(&layer.instance.node).map(|node|&node.kind),
                Some(NodeKind::Source{..}))''')
s=s.replace('timing: AudioTimingId::new(revision, 0),','timing: AudioTimingId { allocation:revision,ordinal:0 },')
s=s.replace('''    d.key(Key::Plus)?;
    d.settled()?;
    let state = camera(d);''','''    d.key(Key::Tab)?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events("Set the exploded Follow scale to exactly 200%", vec![egui::Event::Text("200".into())])?;
    d.key(Key::Tab)?;
    d.settled()?;
    let state = camera(d);''')
s=s.replace('&& shown.center_x.compare(ratio(1,4)).is_gt()', '&& shown.scale==ExactRatio::integer(2) && shown.center_x==ratio(1,4)')
p.write_text(s)
