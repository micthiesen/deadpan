//! Real qualified Original, public exact pause, then a live affine speed update.
use super::*;
use deadpan_core::{BeatNode, PauseInsertionTarget, Subtree};
use deadpan_plan::{AudioBoundDomain, AudioSignalContent};

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn source_peak(project: &Project, range: std::ops::Range<i64>) -> Result<f64> {
    let mut peak = 0.0_f64;
    let mut cursor = range.start;
    while cursor < range.end {
        let end = (cursor + 256).min(range.end);
        let audio = success(&[
            "inspect-audio",
            project.path(),
            "--samples",
            &cursor.to_string(),
            &end.to_string(),
            "--authored-bus",
        ])?;
        let samples: Vec<[f64; 2]> = serde_json::from_value(audio["audio"]["samples"].clone())?;
        assert_eq!(samples.len(), (end - cursor) as usize);
        for sample in samples.into_iter().flatten() {
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }
        cursor = end;
    }
    Ok(peak)
}

fn outside_map(
    document: &ProjectDocument,
    start: i64,
) -> Result<(ExactRatio, ExactRatio, std::ops::Range<AudioSample>)> {
    let plan = deadpan_plan::RenderPlan::compile(document)?;
    let query = plan.audio_processing(
        AudioSample(start)..AudioSample(start + 1),
        Default::default(),
    )?;
    assert_eq!(query.spans.len(), 1);
    let AudioSignalContent::Bound(bound) = &query.spans[0].content else {
        return Err("the outside Original must retain its physical sampling clock".into());
    };
    let AudioBoundDomain::Root(domain) = bound.raw_domain()? else {
        return Err("the outside Original keeps a root physical domain".into());
    };
    Ok((
        bound.reference_at_offset(0)?,
        bound.reference_samples_per_output_sample(),
        domain.root_samples(),
    ))
}

fn pictures(output: i64) -> Vec<(u64, Expected)> {
    // Literal center-sampled pictures. Selected input13.5 contains Original
    //24..25.5, black1.5frames, then Original25.5..36. For15 output,
    // input centers are9*(2n+1)/20; for30 they are9*(2n+1)/40.
    let stage: &[Option<u64>] = if output == 15 {
        &[
            Some(24),
            Some(25),
            None,
            Some(25),
            Some(26),
            Some(27),
            Some(28),
            Some(29),
            Some(30),
            Some(31),
            Some(31),
            Some(32),
            Some(33),
            Some(34),
            Some(35),
        ]
    } else {
        assert_eq!(output, 30);
        &[
            Some(24),
            Some(24),
            Some(25),
            None,
            None,
            None,
            None,
            Some(25),
            Some(26),
            Some(26),
            Some(27),
            Some(27),
            Some(28),
            Some(28),
            Some(29),
            Some(29),
            Some(29),
            Some(30),
            Some(30),
            Some(31),
            Some(31),
            Some(32),
            Some(32),
            Some(33),
            Some(33),
            Some(33),
            Some(34),
            Some(34),
            Some(35),
            Some(35),
        ]
    };
    let mut expected = vec![original(12), original(13)];
    expected.extend(
        stage
            .iter()
            .map(|ordinal| ordinal.map_or(Expected::Background, original)),
    );
    expected.extend([original(36), original(37)]);
    assert_eq!(expected.len(), (output + 4) as usize);
    expected
        .into_iter()
        .enumerate()
        .map(|(frame, picture)| (frame as u64, picture))
        .collect()
}

fn build(dir: &Path, grow: bool) -> Result<Fixture> {
    let name = if grow {
        "clocked-speed-update-grow"
    } else {
        "clocked-speed-update-shrink"
    };
    let output = if grow { 30 } else { 15 };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(2)?;
    project.split_root(12)?;
    project.split_root(24)?;
    let prepared = project.split_root(26)?;
    let children = root_children(&prepared)?;
    assert_eq!(children.len(), 5);
    // Clone complete admitted Partition/Source fragments. Their retained
    // source snapshots and measured A/V mappings survive; no sampling binding
    // is copied to a fresh physical owner.
    let mut nodes = std::collections::BTreeMap::new();
    let lead = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[0],
        &mut nodes,
    )?;
    let spoken = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[2],
        &mut nodes,
    )?;
    let outside = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[3],
        &mut nodes,
    )?;
    let root = project.fresh(1)?.remove(0);
    nodes.insert(
        root.clone(),
        BeatNode::sequence(
            "Qualified affine edit",
            vec![lead, spoken.clone(), outside.clone()],
        ),
    );
    project.delete_range(0, prepared.duration()?.frames())?;
    let fresh = project.apply(|_, document, _| {
        Ok(Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                root,
                nodes,
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        })
    })?;
    assert_eq!(fresh.duration()?.frames(), 16);
    assert!(fresh.audio_bindings().is_empty());
    assert_eq!(
        fresh
            .nodes()
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Source { .. }))
            .count(),
        3
    );
    let rate = project.fresh(1)?.remove(0);
    project.apply(|_, _, _| {
        Ok(Command::WrapRetime {
            node: spoken.clone(),
            id: rate.clone(),
            duration: FrameDuration::new(24)?,
            pitch: PitchPolicy::FollowSpeed,
        })
    })?;
    let pause = project.fresh(1)?.remove(0);
    let before = project.apply(|project, document, revision| {
        let at = ProjectFrame(5);
        let PauseInsertionTarget::Retimed(target) = document.pause_insertion_target(at)? else {
            return Err("the public pause must enter the live FollowSpeed stage".into());
        };
        assert_eq!(target.retime, rate);
        Ok(Command::InsertTime {
            at,
            hold: silent(3, HoldVideo::Background)?,
            id: pause.clone(),
            identities: SplitIdentities {
                nodes: project.fresh(target.required_ids)?,
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    assert_eq!(before.duration()?.frames(), 31);
    assert_eq!(
        before.derived_clocks()[&rate].extent,
        ExactRatio::integer(27)
    );
    assert_eq!(before.derived_clocks()[&rate].child_start, ExactRatio::ZERO);
    assert_eq!(
        before.derived_clocks()[&rate].child_end,
        ExactRatio::new(27, 2)?
    );
    let mut inside = std::collections::BTreeSet::new();
    let mut pending = vec![rate.clone()];
    while let Some(owner) = pending.pop() {
        pending.extend(before.children(&owner).cloned());
        inside.insert(owner);
    }
    let mut outside_sources = Vec::new();
    let mut pending = vec![outside.clone()];
    while let Some(owner) = pending.pop() {
        pending.extend(before.children(&owner).cloned());
        if matches!(before.nodes()[&owner].kind, NodeKind::Source { .. }) {
            outside_sources.push(owner);
        }
    }
    assert_eq!(outside_sources.len(), 1);
    let after = project.apply(|_, _, _| {
        Ok(Command::SetRetime {
            node: rate.clone(),
            duration: FrameDuration::new(output)?,
            pitch: PitchPolicy::FollowSpeed,
        })
    })?;
    assert_eq!(after.duration()?.frames(), output + 4);
    assert_eq!(after.assets(), before.assets());
    assert_eq!(after.nodes()[&pause], before.nodes()[&pause]);
    assert_eq!(after.nodes()[&outside], before.nodes()[&outside]);
    let clock = &after.derived_clocks()[&rate];
    assert_eq!(clock.extent, ExactRatio::integer(output));
    assert_eq!(
        (clock.child_start, clock.child_end),
        (ExactRatio::ZERO, ExactRatio::new(27, 2)?)
    );
    for (owner, clock) in before.derived_clocks() {
        if owner != &rate {
            assert_eq!(after.derived_clocks()[owner], *clock);
        }
    }
    let mut inside_bound = 0;
    for owner in &inside {
        if let Some(binding) = before.audio_bindings().bindings().get(owner) {
            assert_eq!(after.audio_bindings().bindings().get(owner), Some(binding));
            inside_bound += 1;
        }
    }
    assert!(
        inside_bound >= 2,
        "both qualified interior fragments must retain sampling history"
    );
    let mut appended_outside = 0;
    for owner in outside_sources {
        let old = &before.audio_bindings().bindings()[&owner];
        let new = &after.audio_bindings().bindings()[&owner];
        assert_eq!(new.lattice, old.lattice);
        assert_eq!(new.resume, old.resume);
        assert!(new.reanchors.starts_with(&old.reanchors));
        if new.reanchors.len() > old.reanchors.len() {
            appended_outside += 1;
        }
    }
    // A prior resume exactly at the physical entry can already transport this
    // movement; its inert appended step may be elided. Inspect the actual map
    // as well as its chronological history rather than requiring a wire no-op.
    assert_eq!(
        outside_map(&after, boundary(output + 2)?)?,
        outside_map(&before, boundary(29)?)?
    );
    let active = if grow {
        boundary(10)?..boundary(30)?
    } else {
        boundary(6)?..boundary(16)?
    };
    let peak = source_peak(&project, active.clone())?;
    assert!(
        peak > 0.001,
        "{name} source-only interval {active:?} must contain audible Original PCM; peak={peak}"
    );
    let mut fixture = project.finish(vec!["Hold (insert time)", "Retime", "Slow delivery"], pictures(output), vec![format!(
        "Real qualified cfr-bframes.mp4 copied through complete fresh Partition/Source subtrees: Original12..14, Original24..36, Original36..38, initially unbound. Public WrapRetime24 then InsertTime(root5,length3) yields live extent27/input27/2; public SetRetime{output} preserves exact input, all descendant clocks and inside binding bytes. The outside Source's exact entry/reference map and chronological prefix survive its movement; {appended_outside} non-inert step was appended. Original24..25.5, black1.5, Original25.5..36 are sampled at literal output centers; every expected picture is supplied independently. Real source-only current window {active:?} has measured peak{peak}; outside sound cannot satisfy that guard. Canonical44.1k two-stage native/retained-transfer oracles are separate tests. This case keeps actual retained Original A/V, with no audio preprocessing, resampler flattening, event alignment, or equality of differently sped PCM. Emitted H264/AAC must verify every picture/provenance and full audio at offset0."
    )])?;
    fixture
        .audio
        .push((if grow { boundary(6)? + 256 } else { 6406 }, false));
    Ok(fixture)
}

pub fn shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false)
}
pub fn grow(dir: &Path) -> Result<Fixture> {
    build(dir, true)
}
