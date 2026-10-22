//! A long independent measured 44.1-kHz voice on a fixed Preserve output.
use super::*;
use deadpan_audio::{PreparedSource, ResampleRecipe};
use deadpan_core::{
    BeatNode, BeatSound, FrameRate, LinkRelation, SourceNode, SourceTimeBase, SourceVideo,
    SourceVideoMapping, Subtree,
};
use deadpan_media::{
    audio_index::AudioChannelLayout,
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor, sync::atomic::AtomicBool, time::Duration};

pub const FRAMES: u64 = 11;
pub const SAMPLES: i64 = 17_618;
const ONSET: i64 = 6443;
const END: i64 = 11782;

/// This fixture authors raw FitBeat picture mappings, whose endpoint policy is
/// Reject. Its center samples must stay strictly inside the selected Original
/// span; unlike the generic adjacent-hold fixtures, no ordinal is clamped.
pub fn plan_picture(picture: &Value) -> Result<Expected> {
    match picture["type"].as_str().ok_or("picture type")? {
        "background" => Ok(Expected::Background),
        "source" => {
            assert_eq!(picture["endpoints"], "reject");
            let point: SourcePoint = serde_json::from_value(picture["point"].clone())?;
            let selection: ExactSourceSpan = serde_json::from_value(picture["selection"].clone())?;
            assert_eq!(point.time_base, SourceTimeBase::new(1, 30000)?);
            assert_eq!(selection.start().time_base, point.time_base);
            assert_eq!(selection.end().time_base, point.time_base);
            assert!(
                !point.ticks.compare(selection.start().ticks).is_lt(),
                "{picture}"
            );
            assert!(
                point.ticks.compare(selection.end().ticks).is_lt(),
                "{picture}"
            );
            let ordinal = point.ticks.checked_div(ExactRatio::integer(1001))?.floor();
            Ok(original(u64::try_from(ordinal)?))
        }
        other => Err(format!("unexpected fixed Retime picture {other}").into()),
    }
}

fn span(start: i64, end: i64, rate: u32) -> Result<SourceSpan> {
    let time_base = SourceTimeBase::new(1, rate)?;
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )?)
}
fn wav() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav")
        .canonicalize()?)
}

fn register_mono(project: &mut Project) -> Result {
    let retained = success(&[
        "project",
        "retain-original",
        project.path(),
        wav()?.to_str().ok_or("UTF8")?,
    ])?;
    let document = project.document()?;
    let revision = project.next_revision()?;
    let request = project.directory.join("register-mono.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({"protocol":1,"registration":{
        "expected_revision":document.revision_id(),"new_revision":revision,
        "original":retained["retained_original"]["record"]["object"]["content"],
        "new_asset_id":"event","label":"Measured mono 44.1k event","insertion":null},
        "streams":{"type":"audio_only","stream":0,"interpretation":"mono"}}))?,
    )?;
    success(&[
        "project",
        "register-source",
        project.path(),
        "--request-json",
        request.to_str().ok_or("UTF8")?,
    ])?;
    Ok(())
}

fn read(project: &Project, start: i64, end: i64) -> Result<Vec<[f32; 2]>> {
    let mut output = Vec::new();
    for at in (start..end).step_by(256) {
        let until = (at + 256).min(end);
        let inspected = success(&[
            "inspect-audio",
            project.path(),
            "--samples",
            &at.to_string(),
            &until.to_string(),
            "--authored-bus",
        ])?;
        let samples: Vec<[f32; 2]> = serde_json::from_value(inspected["audio"]["samples"].clone())?;
        assert_eq!(samples.len(), (until - at) as usize);
        output.extend(samples);
    }
    Ok(output)
}

fn oracle(phase: ExactRatio) -> Result<Vec<[f32; 2]>> {
    let cancelled = AtomicBool::new(false);
    let bytes = fs::read(wav()?)?;
    let session = AudioSession::open_verified(
        &mut Cursor::new(&bytes),
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64)?,
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )?;
    assert_eq!(session.index().stream().sample_rate, 44100);
    let index = session.index().clone();
    let source = PreparedSource::with_layout(
        session,
        &index,
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
        &cancelled,
    )?;
    // Literal old owner origin3, +37 mix samples, first root sample4842:
    // phase=3100+(4842-4841.8)*44100/48000=3100+147/800.
    // Root ripple moves the allocated onset by1601 samples, retaining that
    // phase. Recomputing from current ideal6443.4 would be3100-147/400.
    let recipe = ResampleRecipe::new(
        3100..8005,
        phase,
        AudioSample(ONSET),
        ratio(147, 160)?,
        AudioSample(ONSET)..AudioSample(END),
    )?;
    let mut result = Vec::new();
    for at in (ONSET..END).step_by(256) {
        result.extend(
            source
                .prepare(
                    recipe.clone(),
                    AudioSample(at),
                    u32::try_from((END - at).min(256))?,
                    Duration::from_secs(10),
                    &cancelled,
                )?
                .samples,
        );
    }
    Ok(result)
}

fn assert_event(project: &Project, expected: &[[f32; 2]]) -> Result<f32> {
    // Include exact onset/end neighbors; those remain finite zero, not leaked
    // filter context or a recomputed/new event fade.
    let actual = read(project, ONSET - 3, END + 3)?;
    assert_eq!(actual.len(), expected.len() + 6);
    assert!(
        actual[..3]
            .iter()
            .chain(&actual[actual.len() - 3..])
            .flatten()
            .all(|v| v.abs() < 1e-7)
    );
    let mut maximum = 0.0_f32;
    for (index, (actual, expected)) in actual[3..3 + expected.len()]
        .iter()
        .zip(expected)
        .enumerate()
    {
        for channel in 0..2 {
            assert!(actual[channel].is_finite());
            let error = (actual[channel] - expected[channel]).abs();
            maximum = maximum.max(error);
            assert!(
                error < 0.000_002,
                "literal event phase sample{index}/channel{channel}: {} vs {}",
                actual[channel],
                expected[channel]
            );
        }
    }
    Ok(maximum)
}

pub fn build(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "fixed-retime-output-sound")?;
    register_mono(&mut project)?;
    project.delete_range(0, 120)?;
    let source = SourceAudio {
        asset: project.asset.clone(),
        span: span(12000, 22000, 48000)?,
    };
    let room = |length| -> Result<BeatNode> {
        Ok(BeatNode::hold(
            "Measured quiet RoomTone",
            HoldRecipe {
                duration: FrameDuration::new(length)?,
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::RoomTone {
                    source: source.clone(),
                },
            },
        ))
    };
    let picture = |length| -> Result<BeatNode> {
        let mut result = BeatNode::sequence("Qualified Original", vec![]);
        result.kind = NodeKind::Source {
            source: SourceNode {
                duration: FrameDuration::new(length)?,
                edit_window: None,
                video: SourceVideo::Stream {
                    asset: project.asset.clone(),
                    span: span(12012, 12012 + length * 1001, 30000)?,
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(source.clone()),
                audio_mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    FrameRate::new(30000, 1001)?,
                )?,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
            },
        };
        Ok(result)
    };
    let owner = node("processing")?;
    let mut stage = BeatNode::sequence("Fixed Preserve output", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("body")?,
        duration: FrameDuration::new(4)?,
        mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(8))?,
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    let mut nodes = BTreeMap::from([
        (node("lead")?, room(3)?),
        (owner.clone(), stage),
        (
            node("body")?,
            BeatNode::sequence(
                "Input",
                vec![node("before")?, node("hold")?, node("after")?],
            ),
        ),
        (node("before")?, picture(3)?),
        (node("hold")?, room(2)?),
        (node("after")?, picture(4)?),
        (node("tail")?, picture(3)?),
    ]);
    for node in nodes.values_mut() {
        node.audio_edges.node_start = AudioEdgePolicy::Hard;
        node.audio_edges.node_end = AudioEdgePolicy::Hard;
    }
    let lead = nodes.remove(&node("lead")?).ok_or("lead")?;
    let tail = nodes.remove(&node("tail")?).ok_or("tail")?;
    for (index, name, nodes) in [
        (0, "lead", BTreeMap::from([(node("lead")?, lead)])),
        (1, "processing", nodes),
        (2, "tail", BTreeMap::from([(node("tail")?, tail)])),
    ] {
        project.apply(|_, document, _| {
            Ok(Command::Insert {
                parent: document.root().clone(),
                index,
                subtree: Subtree {
                    root: node(name)?,
                    nodes,
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            })
        })?;
    }
    assert!(project.document()?.audio_bindings().is_empty());
    // Prove the real measured physical inputs are zero here. No muted owner,
    // Silence Hold, altered source bytes or allowance creates this isolation.
    assert!(
        read(&project, 0, 16016)?
            .iter()
            .flatten()
            .all(|v| v.abs() < 1e-7)
    );
    let event_source = SourceAudio {
        asset: AssetId::new("event")?,
        span: span(3100, 8005, 44100)?,
    };
    let event = BeatSound {
        label: "Independent 44.1k output event".into(),
        mapping: SourceAudioMapping::natural_rate(event_source.span, FrameRate::new(30000, 1001)?)?,
        source: event_source,
        offset: AudioSample(37),
        treatments: Default::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let sound = SoundId::new("event")?;
    project.apply(|_, _, _| {
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: event.clone(),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: node("lead")?,
            duration: FrameDuration::new(4)?,
        })
    })?;
    let before = project.document()?;
    let journal = before.audio_bindings().sound_clocks()[&owner][&sound].clone();
    assert_eq!(journal.scope(), &owner);
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(journal.clocks()[0].owner(), &owner);
    let expected = oracle(ratio(2480147, 800)?)?;
    let wrong = oracle(ratio(1239853, 400)?)?;
    let phase_difference = expected
        .iter()
        .flatten()
        .zip(wrong.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        phase_difference > 0.001,
        "fixture must distinguish the wrong newly rounded native phase"
    );
    let mut errors = vec![assert_event(&project, &expected)?];
    for length in [3, 1] {
        let prior = project.document()?;
        project.apply(|_, _, _| {
            Ok(Command::SetHoldDuration {
                node: node("hold")?,
                duration: FrameDuration::new(length)?,
            })
        })?;
        let after = project.document()?;
        assert_eq!(after.duration()?.frames(), FRAMES as i64);
        assert_eq!(after.nodes()[&owner], before.nodes()[&owner]);
        assert_eq!(after.beat_sounds()[&owner][&sound], event);
        assert_eq!(
            after.audio_bindings().sound_clocks()[&owner][&sound],
            journal
        );
        assert_eq!(after.node_duration(&node("body")?)?.frames(), 7 + length);
        errors.push(assert_event(&project, &expected)?);
        assert_ne!(prior.revision_id(), after.revision_id());
    }
    let final_document = project.document()?;
    for name in ["lead", "hold"] {
        assert!(matches!(&final_document.nodes()[&node(name)?].kind,
        NodeKind::Hold { recipe } if matches!(recipe.audio,HoldAudio::RoomTone { .. })));
    }
    assert!(final_document.sound_allowances().is_empty());
    let oracle_report = json!({"event_native_samples":[3100,8005],"mix_support":[ONSET,END],"event_samples":END-ONSET,
        "retained_native_phase":"2480147/800","source_step":"147/160","wrong_new_phase":"1239853/400",
        "phase_control_max_difference":phase_difference,"maximum_event_errors_before_grow_shrink":errors,
        "original_audio_policy":"measured quiet Original and RoomTone; no Silence or mute treatment"});
    fs::write(
        project.directory.join("output-owned-event-oracle.json"),
        serde_json::to_vec_pretty(&oracle_report)?,
    )?;
    // Final body Source3/Hold1/Source4; stage centers at input1.875,3.625,
    // 5.375,7.125 select Original13/background/Original13/Original15.
    let expected_pictures = [
        Expected::Background,
        Expected::Background,
        Expected::Background,
        Expected::Background,
        original(13),
        Expected::Background,
        original(13),
        original(15),
        original(12),
        original(13),
        original(14),
    ];
    project.finish(vec!["Hold (insert time)","Retime","Attached sounds"],
        expected_pictures.into_iter().enumerate().map(|(frame,picture)|(frame as u64,picture)).collect(),
        vec![format!("Ordinary room Hold2→3→1 under fixed Preserve output4/input[1,8); genuine root ripple lead3→4 retains independent output sound. Literal event oracle {oracle_report}")])
}
