use super::*;
use deadpan_core::{
    AudioEdgePolicy, AudioSample, AudioTimingId, AudioTreatments, BeatSound, Command,
    CommandRequest, EqualizerRecipe, FrameDuration, GainDb, HoldAudio, HoldRecipe, HoldVideo,
    RevisionId, SoundId, SoundOverflowPolicy, SourceAudio, SourceAudioMapping, SourceSpan,
    SourceTimestamp, SplitIdentities,
};
use deadpan_store::{AccessMode, ProjectStore};

pub(super) fn prefix_timing() -> Result<AudioTimingId, String> {
    Ok(AudioTimingId {
        allocation: RevisionId::new("ui-sound-group-prefix").map_err(|e| e.to_string())?,
        ordinal: 0,
    })
}

pub(super) fn treatment(gains: [i32; 3]) -> Result<AudioTreatments, String> {
    AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(gains.map(|gain| GainDb::new(gain).unwrap()), None, None)
                .map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())
}

fn commit(store: &mut ProjectStore, command: Command, name: &str) -> Result<(), String> {
    let before = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new(format!("ui-sound-group-{name}"))
                .map_err(|e| e.to_string())?,
            command,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn create(source: &Path, path: &Path, freeze: HoldVideo) -> Result<(), String> {
    deadpan_store::portable::copy_portable(
        source,
        path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    let before = store.snapshot().map_err(|e| e.to_string())?;
    let children = before.children(before.root()).cloned().collect::<Vec<_>>();
    let owner = children
        .first()
        .ok_or("No qualified Original Source")?
        .clone();
    let NodeKind::Source { source } = &before.nodes()[&owner].kind else {
        return Err("Sound endpoint replay requires one direct Original Source".into());
    };
    let original = source.audio.clone().ok_or("No qualified Original audio")?;
    let start = original.span.start();
    if children.len() != 1
        || before.duration().map_err(|e| e.to_string())?.frames() != 120
        || before.presentation_basis().frame_rate
            != deadpan_core::FrameRate::new(30000, 1001).map_err(|e| e.to_string())?
        || start.time_base
            != deadpan_core::SourceTimeBase::new(1, 48000).map_err(|e| e.to_string())?
        || original.span.end().ticks - start.ticks < 56016
        || before.assets()[&original.asset]
            .source_qualification
            .is_none()
        || !before.beat_sounds().is_empty()
        || !before.sounds().is_empty()
    {
        return Err("Sound endpoint replay requires the untouched qualified 120-frame cfr-bframes workspace at 30000/1001 with 48 kHz audio".into());
    }
    let audio = SourceAudio {
        asset: original.asset,
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start
                    .ticks
                    .checked_add(40000)
                    .ok_or("Event audio start overflow")?,
                time_base: start.time_base,
            },
            SourceTimestamp {
                ticks: start
                    .ticks
                    .checked_add(56016)
                    .ok_or("Event audio end overflow")?,
                time_base: start.time_base,
            },
        )
        .map_err(|e| e.to_string())?,
    };
    commit(
        &mut store,
        Command::SetAudioTreatments {
            node: owner.clone(),
            treatments: treatment([3000, -1500, 1000])?,
        },
        "owner-eq",
    )?;
    commit(
        &mut store,
        Command::SetBeatSound {
            owner,
            id: SoundId::new("ui-sound-group-event").map_err(|e| e.to_string())?,
            event: BeatSound {
                label: "Independent ten-frame Original event".into(),
                mapping: SourceAudioMapping::natural_rate(
                    audio.span,
                    before.presentation_basis().frame_rate,
                )
                .map_err(|e| e.to_string())?,
                source: audio,
                offset: AudioSample(37),
                treatments: treatment([6000, -6000, 3000])?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
        "event",
    )?;
    commit(
        &mut store,
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: HoldRecipe {
                duration: FrameDuration::new(1).map_err(|e| e.to_string())?,
                video: freeze,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
            id: NodeId::new("ui-sound-group-prefix").map_err(|e| e.to_string())?,
            identities: SplitIdentities::default(),
            timing: prefix_timing()?,
        },
        "prefix",
    )?;
    store.validate().map_err(|e| e.to_string())
}
