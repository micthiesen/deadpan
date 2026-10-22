use super::*;
use deadpan_core::{
    AudioEdgePolicy, AudioSample, AudioTreatments, BeatNode, BeatSound, Command, CommandRequest,
    FrameDuration, FrameRange, HoldAudio, HoldVideo, PitchPolicy, RetimePurpose, RevisionId,
    SoundId, SoundOverflowPolicy, SourceAudio, SourceAudioMapping, SourceSpan, SourceTimestamp,
    Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};

pub(super) fn id(name: &str) -> Result<NodeId, String> {
    NodeId::new(format!("ui-fixed-duration-{name}")).map_err(|e| e.to_string())
}
fn commit(store: &mut ProjectStore, command: Command, name: &str) -> Result<(), String> {
    let before = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new(format!("ui-fixed-duration-{name}"))
                .map_err(|e| e.to_string())?,
            command,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub(super) fn create(
    source: &Path,
    path: &Path,
    cropped: bool,
    freeze: HoldVideo,
) -> Result<(), String> {
    deadpan_store::portable::copy_portable(
        source,
        path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    let before = store.snapshot().map_err(|e| e.to_string())?;
    let original = before
        .nodes()
        .values()
        .find_map(|node| match &node.kind {
            NodeKind::Source { source } => source.audio.clone(),
            _ => None,
        })
        .ok_or("No qualified Original audio")?;
    let start = original.span.start();
    if start.time_base != deadpan_core::SourceTimeBase::new(1, 48000).map_err(|e| e.to_string())?
        || original.span.end().ticks - start.ticks < 4800
        || before.assets()[&original.asset]
            .source_qualification
            .is_none()
    {
        return Err(
            "The replay requires a qualified Original with at least4800 samples at48kHz".into(),
        );
    }
    let audio = SourceAudio {
        asset: original.asset,
        span: SourceSpan::new(
            start,
            SourceTimestamp {
                ticks: start
                    .ticks
                    .checked_add(4800)
                    .ok_or("Audio range overflow")?,
                time_base: start.time_base,
            },
        )
        .map_err(|e| e.to_string())?,
    };
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        return Err("Qualified fixture root is not a Sequence".into());
    };
    commit(
        &mut store,
        Command::DeleteChildren {
            parent: before.root().clone(),
            first: children.first().ok_or("No copied edit children")?.clone(),
            last: children.last().ok_or("No copied edit children")?.clone(),
            timing: deadpan_core::AudioTimingId {
                allocation: RevisionId::new("ui-fixed-duration-clear")
                    .map_err(|e| e.to_string())?,
                ordinal: 0,
            },
        },
        "clear",
    )?;
    let mut rate = BeatNode::sequence("Fixed input window", vec![]);
    rate.kind = NodeKind::Retime {
        child: id("body")?,
        duration: FrameDuration::new(if cropped { 10 } else { 20 }).map_err(|e| e.to_string())?,
        mapping: FrameRange::new(
            ProjectFrame(if cropped { 3 } else { 0 }),
            ProjectFrame(if cropped { 18 } else { 20 }),
        )
        .map_err(|e| e.to_string())?,
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let hold = |label: &str, length, video| -> Result<BeatNode, String> {
        Ok(BeatNode::hold(
            label,
            HoldRecipe {
                duration: FrameDuration::new(length).map_err(|e| e.to_string())?,
                video,
                picture_context: None,
                audio: HoldAudio::RoomTone {
                    source: audio.clone(),
                },
            },
        ))
    };
    let current = store.snapshot().map_err(|e| e.to_string())?;
    commit(
        &mut store,
        Command::Insert {
            parent: current.root().clone(),
            index: 0,
            subtree: Subtree {
                root: id("rate")?,
                nodes: BTreeMap::from([
                    (id("rate")?, rate),
                    (
                        id("body")?,
                        BeatNode::sequence(
                            "Ordinary input",
                            vec![id("lead")?, id("selected")?, id("tail")?],
                        ),
                    ),
                    (id("lead")?, hold("Lead", 2, HoldVideo::Background)?),
                    (id("selected")?, hold("Resize this pause", 8, freeze)?),
                    (id("tail")?, hold("Suffix", 10, HoldVideo::Background)?),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "insert",
    )?;
    let rate = store
        .snapshot()
        .map_err(|e| e.to_string())?
        .presentation_basis()
        .frame_rate;
    commit(
        &mut store,
        Command::SetBeatSound {
            owner: id("rate")?,
            id: SoundId::new("ui-fixed-duration-output-event").map_err(|e| e.to_string())?,
            event: BeatSound {
                label: "Independent qualified Original event".into(),
                mapping: SourceAudioMapping::natural_rate(audio.span, rate)
                    .map_err(|e| e.to_string())?,
                source: audio,
                offset: AudioSample(37),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
        "sound",
    )?;
    Ok(())
}
