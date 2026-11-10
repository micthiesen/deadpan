//! Place slice inside one explicit Repeat play over the real counter A/V
//! Original: an Original moment placed strictly inside play 2 only.
use super::*;
use deadpan_core::{
    InstancePath, MarkId, OccurrenceEdit, OccurrenceIdentities, OccurrencePlacement, RepeatInstance,
};

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn pcm(project: &Project, start: i64) -> Result<Vec<[f64; 2]>> {
    let result = success(&[
        "inspect-audio",
        project.path(),
        "--samples",
        &start.to_string(),
        &(start + 256).to_string(),
        "--authored-bus",
    ])?;
    Ok(serde_json::from_value(result["audio"]["samples"].clone())?)
}

/// The Original click at sample 48000 lies 3155 samples after B(28) of the
/// Original's frame clock (Original frame 28 is where the tail begins).
const CLICK_IN_TAIL: i64 = 3155 - 64;

/// Base edit Original 12..42 at 30000/1001: root = [12..22, body, 28..42]
/// where body = [22..28) plays three times. Play 2 (project 16..22) receives
/// the six-frame moment 22..28 three frames in, at project 19. Plays 1 and 3
/// keep the shared definition; play 3 and the tail move six frames later on
/// their own retained clocks (six frames is 9609.6 samples, not whole).
pub fn build(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "scoped-place-slice")?;
    project.shorten()?;
    project.split_root(10)?;
    project.split_root(16)?;
    project.apply(|_, document, _| {
        Ok(Command::Group {
            parent: document.root().clone(),
            start: 1,
            end: 2,
            id: node("body")?,
            label: "Shared body".into(),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::WrapRepeat {
            node: node("body")?,
            id: node("plays")?,
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let before = project.document()?;
    assert_eq!(before.duration()?.frames(), 42);
    let tail_before = pcm(&project, boundary(28)? + CLICK_IN_TAIL)?;
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&node("plays")?].kind else {
        return Err("Repeat".into());
    };
    let beat = before
        .children(&node("body")?)
        .next()
        .cloned()
        .ok_or("body child")?;
    let instance = InstancePath {
        node: beat,
        repeats: vec![RepeatInstance {
            node: node("plays")?,
            iteration: iterations.at(1).ok_or("play 2")?,
        }],
    };
    let at = OccurrencePlacement::Interior {
        index: 0,
        at: FrameDuration::new(3)?,
    };
    project.apply(|project, document, revision| {
        let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
        let asset = document
            .assets()
            .keys()
            .next()
            .cloned()
            .ok_or("Original asset")?;
        let receipt = store.registered_source(document.revision_id(), &asset)?;
        let video = receipt.snapshot().video().ok_or("qualified pictures")?;
        let moment = deadpan_media::source_import_timing::derive_source_moment(
            video.index(),
            receipt.snapshot().audio(),
            22..28,
            document.presentation_basis().frame_rate,
        )?
        .source_node(asset);
        let plan = document.occurrence_placement(&instance, at, None)?;
        assert_eq!(plan.boundary, ProjectFrame(19));
        let timing = AudioTimingId {
            allocation: revision.clone(),
            ordinal: 0,
        };
        let edit = OccurrenceEdit::PasteSource {
            before: false,
            source: Box::new(moment),
            id: node("placed-moment")?,
            label: "Original [22..28)".into(),
            timing,
            at: Some(at),
            splits: SplitIdentities {
                nodes: project.fresh(plan.required_ids)?,
            },
        };
        let needs = document.occurrence_structure_requirements(&instance, &edit)?;
        Ok(Command::EditOccurrence {
            instance: instance.clone(),
            edit,
            identities: OccurrenceIdentities {
                nodes: project.fresh(needs.nodes)?,
                marks: (0..needs.marks)
                    .map(|n| MarkId::new(format!("{revision}-mark-{n}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
        })
    })?;
    let after = project.document()?;
    assert_eq!(after.duration()?.frames(), 48);
    // The tail starts at 34 instead of 28 and keeps its old sample entry.
    let tail_start = boundary(34)? + CLICK_IN_TAIL;
    let tail_after = pcm(&project, tail_start)?;
    assert!(
        tail_before
            .iter()
            .flatten()
            .any(|value| value.abs() > 0.001)
    );
    for (sample, (old, new)) in tail_before.iter().zip(&tail_after).enumerate() {
        for channel in 0..2 {
            assert!(
                (old[channel] - new[channel]).abs() < 1e-6,
                "tail sample {sample} channel {channel}: {} != {}",
                new[channel],
                old[channel]
            );
        }
    }
    let mut pictures: Vec<_> = (12..22).map(original).collect();
    pictures.extend((22..28).map(original));
    pictures.extend((22..25).map(original));
    pictures.extend((22..28).map(original));
    pictures.extend((25..28).map(original));
    pictures.extend((22..28).map(original));
    pictures.extend((28..42).map(original));
    assert_eq!(pictures.len(), 48);
    let mut fixture = project.finish(
        vec!["Repeat operator", "Visual slice placement"],
        pictures
            .into_iter()
            .enumerate()
            .map(|(frame, expected)| (frame as u64, expected))
            .collect(),
        vec![format!(
            "Real cfr-bframes.mp4 A/V at 30000/1001. A three-play Repeat of Original 22..28; Place slice puts Original 22..28 three frames inside play 2 only (project 19). Plays 1 and 3 keep the shared definition; the tail moves 6 frames (9609.6 samples) and its decoded click window at {tail_start} equals the pre-edit window at B(28)+{CLICK_IN_TAIL} within 1e-6."
        )],
    )?;
    fixture.audio = vec![(tail_start, true)];
    Ok(fixture)
}
