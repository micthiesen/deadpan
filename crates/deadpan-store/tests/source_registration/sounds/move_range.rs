use super::*;
use deadpan_core::{ExactFrameRange, ExactRatio, FrameRange, MoveRangeDestination};

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn moves_preserve_qualified_root_sound_recipes_routes_and_hold_grants() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    store.register_source(
        &request(&store, &original, "import", "camera", Some("clip"))?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let imported = store.snapshot()?;
    let routed = SoundId::new("routed")?;
    store.commit(&edit(
        &imported,
        "sound",
        Command::SetSound {
            id: routed.clone(),
            event: sound(&imported)?,
        },
    ))?;
    let hold = NodeId::new("pause")?;
    store.commit(&edit(
        &store.snapshot()?,
        "pause",
        Command::InsertTime {
            at: ProjectFrame(5),
            hold: HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(2)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: hold.clone(),
            identities: SplitIdentities {
                nodes: (0..3)
                    .map(|i| NodeId::new(format!("pause-split-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: revision("pause"),
                ordinal: 0,
            },
        },
    ))?;
    let paused = store.snapshot()?;
    assert!(!paused.sound_routes()[&routed].edits.is_empty());

    // This entire unrouted sound lies inside the moved picture interval. Its
    // root owner keeps the same clock, so removal must not erase its support.
    let mut short = sound(&paused)?;
    short.offset = AudioSample(0);
    short.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: short.mapping.duration_frames(paused.duration()?)?,
        selection: ExactFrameRange {
            start: ExactRatio::integer(1),
            end: ExactRatio::integer(3),
        },
    };
    let short_id = SoundId::new("inside-moved-picture")?;
    store.commit(&edit(
        &paused,
        "short-sound",
        Command::SetSound {
            id: short_id.clone(),
            event: short,
        },
    ))?;
    let live = store.snapshot()?;
    let live_id = SoundId::new("live-hold-sound")?;
    store.commit(&edit(
        &live,
        "live-sound",
        Command::SetSound {
            id: live_id.clone(),
            event: sound(&live)?,
        },
    ))?;
    store.commit(&edit(
        &store.snapshot()?,
        "grant",
        Command::SetSoundAllowance {
            sound: live_id,
            issuer: SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: hold,
                    repeats: Vec::new(),
                },
            },
            allowed: true,
        },
    ))?;
    let before = store.snapshot()?;
    assert!(!before.sound_routes().contains_key(&short_id));
    assert!(!before.sound_allowances().is_empty());
    let sound_bytes = serde_json::to_vec(before.sounds())?;
    let route_bytes = serde_json::to_vec(before.sound_routes())?;
    let allowance_bytes = serde_json::to_vec(before.sound_allowances())?;
    let duration = before.duration()?.frames();
    assert!(duration > 10);
    let before_counts = counts(&path)?;
    for (name, selected, index) in [
        (
            "right",
            FrameRange::new(ProjectFrame(0), ProjectFrame(4))?,
            before.children(before.root()).count(),
        ),
        (
            "left",
            FrameRange::new(ProjectFrame(duration - 4), ProjectFrame(duration))?,
            0,
        ),
    ] {
        let current = store.snapshot()?;
        let destination = MoveRangeDestination::Seam {
            parent: current.root().clone(),
            index,
        };
        let plan = current.range_move(current.root(), selected, &destination)?;
        let command = edit(
            &current,
            name,
            Command::MoveRange {
                source_revision: current.revision_id().clone(),
                source_parent: current.root().clone(),
                range: selected,
                destination,
                identities: SplitIdentities {
                    nodes: (0..plan.required_ids)
                        .map(|i| NodeId::new(format!("{name}-split-{i}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                timing: AudioTimingId {
                    allocation: revision(name),
                    ordinal: 0,
                },
            },
        );
        let rows = counts(&path)?;
        let preview = store.preview(&command)?;
        assert_eq!(counts(&path)?, rows);
        assert_eq!(store.commit(&command)?.edit, preview);
        let moved = store.snapshot()?;
        assert_eq!(moved.duration()?, before.duration()?);
        assert_eq!(serde_json::to_vec(moved.sounds())?, sound_bytes);
        assert_eq!(serde_json::to_vec(moved.sound_routes())?, route_bytes);
        assert_eq!(
            serde_json::to_vec(moved.sound_allowances())?,
            allowance_bytes
        );
        assert_eq!(moved.assets(), before.assets());
        store.validate()?;
    }
    let after = store.snapshot()?;
    assert_eq!(
        counts(&path)?,
        (before_counts.0 + 2, before_counts.1 + 2, before_counts.2)
    );
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo-left"))?;
    store.undo(&revision("undo-left"), revision("undo-right"))?;
    assert_authored(&store.snapshot()?, &before)?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(&revision("undo-right"), revision("redo-right"))?;
    store.redo(&revision("redo-right"), revision("redo-left"))?;
    assert_authored(&store.snapshot()?, &after)?;
    assert_eq!(
        serde_json::to_vec(store.snapshot()?.sound_routes())?,
        route_bytes
    );
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
