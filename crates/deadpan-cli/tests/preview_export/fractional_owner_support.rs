//! Complete nonzero Original/RoomTone master through fractional owner seams.
use super::*;
use deadpan_audio::{PreparedSource, ResampleRecipe};
use deadpan_cli::audio::ProjectAudioSession;
use deadpan_core::{
    BeatNode, BeatSound, FrameRate, LinkRelation, Saturation, SourceNode, SourceTimeBase,
    SourceVideo, SourceVideoMapping, Subtree,
};
use deadpan_media::{
    audio_index::AudioChannelLayout,
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    sync::atomic::AtomicBool,
    time::Duration,
};

pub const FRAMES: u64 = 11;
pub const SAMPLES: i64 = 17618;
const EVENT_START: i64 = 6443;
const EVENT_END: i64 = 11782;

fn hash(path: &Path) -> Result<String> {
    let mut input = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut block = [0_u8; 65536];
    loop {
        let count = input.read(&mut block)?;
        if count == 0 {
            break;
        }
        digest.update(&block[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

// The same nonperiodic low-frequency knot construction used by
// tools/media-qualification/generate_pcm_export_signals.py. Distinct channels,
// finite DC offsets and conservative amplitudes make the Original nonzero at
// meaningful fractional boundaries while leaving the shared nonlinear bus headroom.
fn knot(index: u32, channel: u32) -> f64 {
    let mut value = index
        .wrapping_add(1)
        .wrapping_mul(0x9e3779b9)
        .wrapping_add(channel.wrapping_mul(0x85ebca6b));
    value = (value ^ (value >> 16)).wrapping_mul(0x7feb352d);
    value = (value ^ (value >> 15)).wrapping_mul(0x846ca68b);
    f64::from(value ^ (value >> 16)) / f64::from(u32::MAX) - 0.5
}

fn nonzero_original(directory: &Path) -> Result<(PathBuf, Value)> {
    fs::create_dir_all(directory)?;
    let directory = directory.canonicalize()?;
    let wav = directory.join("nonzero-original.wav");
    let output = directory.join("nonzero-original.mp4");
    let data_bytes = 192192_u32 * 4;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((60 + data_bytes).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(40_u32.to_le_bytes());
    bytes.extend(0xfffe_u16.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(48000_u32.to_le_bytes());
    bytes.extend(192000_u32.to_le_bytes());
    bytes.extend(4_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(22_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(3_u32.to_le_bytes());
    bytes.extend([
        1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ]);
    bytes.extend(b"data");
    bytes.extend(data_bytes.to_le_bytes());
    for sample in 0..192192_u32 {
        let position = u64::from(sample) * 431;
        let cell = u32::try_from(position / 48000)?;
        let fraction = (position % 48000) as f64 / 48000.0;
        for channel in 0..2_u32 {
            let a = knot(cell, channel);
            let b = knot(cell + 1, channel);
            let (scale, dc) = if channel == 0 {
                (0.18, 0.013)
            } else {
                (0.13, -0.009)
            };
            let value = dc + scale * (a + fraction * (b - a));
            let integer = (value * 32768.0).round();
            assert!((-32768.0..=32767.0).contains(&integer));
            bytes.extend((integer as i16).to_le_bytes());
        }
    }
    assert_eq!(bytes.len(), usize::try_from(68 + data_bytes)?);
    fs::write(&wav, &bytes)?;
    let ffmpeg = development_ffmpeg().map_err(std::io::Error::other)?;
    let source = source_media().canonicalize()?;
    assert_eq!(
        hash(&source)?,
        "5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918"
    );
    let arguments = vec![
        "-nostdin".into(),
        "-v".into(),
        "error".into(),
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-i".into(),
        wav.to_string_lossy().into_owned(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-flags:a".into(),
        "+bitexact".into(),
        "-video_track_timescale".into(),
        "30000".into(),
        "-movie_timescale".into(),
        "48000".into(),
        "-use_editlist".into(),
        "1".into(),
        "-map_metadata".into(),
        "-1".into(),
        "-fflags".into(),
        "+bitexact".into(),
        "-movflags".into(),
        "+faststart+write_colr".into(),
        "-f".into(),
        "mp4".into(),
        output.to_string_lossy().into_owned(),
    ];
    let mut command = Process::new(&ffmpeg);
    command
        .args(&arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let result = deadpan_native_process::spawn(&mut command)?.wait_with_output()?;
    assert!(
        result.status.success(),
        "development stream-copy producer: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // ISO MP4 emits admitted nclx color metadata and mp4a AAC. QuickTime MOV
    // instead emits nclc for H.264 and sowt PCM, outside the strict grammar.
    // Explicit edit metadata retains192192 presented samples after1024 delay
    // and320 padding samples; the fixture asserts that qualified endpoint.
    // Only the source audio is encoded. All video packets retain their exact
    // bytes, PTS, DTS and duration. The master oracle reads actual qualified
    // decoded Original PCM rather than treating the input WAV as its output.
    let provenance = json!({"recipe":"stereo s16 48000Hz192192samples; 431Hz interpolated unsigned32 hashed knots; L=.013+.18*knot,R=-.009+.13*knot; round(value*32768); explicit FL/FR mask3; source audio AAC192k without resampling, ISO MP4 explicit delay/padding edits and48000 movie clock; cfr video stream copied without reencoding",
        "producer":ffmpeg,"producer_sha256":hash(&ffmpeg)?,"arguments":arguments,"wav_sha256":hash(&wav)?,
        "original":output,"original_sha256":hash(&output)?,"original_bytes":fs::metadata(&output)?.len(),
        "video_source_sha256":hash(&source)?});
    fs::write(
        directory.join("producer.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    Ok((output, provenance))
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
fn event_wav() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav")
        .canonicalize()?)
}

fn register_event(project: &mut Project) -> Result {
    let retained = success(&[
        "project",
        "retain-original",
        project.path(),
        event_wav()?.to_str().ok_or("UTF8")?,
    ])?;
    let current = project.document()?;
    let revision = project.next_revision()?;
    let path = project.directory.join("register-event.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"protocol":1,"registration":{
        "expected_revision":current.revision_id(),"new_revision":revision,"original":retained["retained_original"]["record"]["object"]["content"],
        "new_asset_id":"event","label":"Independent measured44100 event","insertion":null},
        "streams":{"type":"audio_only","stream":0,"interpretation":"mono"}}))?,
    )?;
    success(&[
        "project",
        "register-source",
        project.path(),
        "--request-json",
        path.to_str().ok_or("UTF8")?,
    ])?;
    Ok(())
}

fn event_reference() -> Result<Vec<[f32; 2]>> {
    let cancelled = AtomicBool::new(false);
    let bytes = fs::read(event_wav()?)?;
    let session = AudioSession::open_verified(
        &mut Cursor::new(&bytes),
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64)?,
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )?;
    let index = session.index().clone();
    assert_eq!(index.stream().sample_rate, 44100);
    let source = PreparedSource::with_layout(
        session,
        &index,
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
        &cancelled,
    )?;
    // The old frame3 origin plus37 samples starts at ideal4841.8. Its first
    // allocated point4842 reads3100+(4842-4841.8)*147/160=2480147/800.
    // Lead growth moves that allocated point by1601, preserving its phase:
    // current onset6443 differs from a fresh ideal6443.4 recomputation. The
    // selected4905 native samples retain5339 allocated mix points.
    let recipe = ResampleRecipe::new(
        3100..8005,
        ratio(2480147, 800)?,
        AudioSample(EVENT_START),
        ratio(147, 160)?,
        AudioSample(EVENT_START)..AudioSample(EVENT_END),
    )?;
    let mut output = Vec::new();
    for at in (EVENT_START..EVENT_END).step_by(256) {
        let block = source.prepare(
            recipe.clone(),
            AudioSample(at),
            u32::try_from((EVENT_END - at).min(256))?,
            Duration::from_secs(10),
            &cancelled,
        )?;
        output.extend(block.samples.into_iter().map(|pair| {
            pair.map(|sample| (f64::from(sample) * 10.0_f64.powf(-18.0 / 20.0)) as f32)
        }));
    }
    Ok(output)
}

struct Bus {
    samples: Vec<[f32; 2]>,
    mask: Vec<bool>,
}
fn bus(project: &Project) -> Result<Bus> {
    let document = project.document()?;
    let mut session = ProjectAudioSession::open_revision(&project.package, document.revision_id())?;
    assert_eq!(session.plan().audio_duration()?.0, SAMPLES);
    let mut samples = Vec::new();
    let mut mask = Vec::new();
    for start in (0..SAMPLES).step_by(256) {
        let count = u32::try_from((SAMPLES - start).min(256))?;
        let block =
            session.read_authored_bus(AudioSample(start), count, &AtomicBool::new(false))?;
        assert_eq!(block.start, AudioSample(start));
        assert_eq!(block.samples.len(), count as usize);
        mask.extend((start..start + i64::from(count)).map(|at| {
            block
                .suppressed
                .iter()
                .any(|range| range.contains(&AudioSample(at)))
        }));
        samples.extend(block.samples);
    }
    Ok(Bus { samples, mask })
}

fn physical_control(project: &Project, name: &'static str) -> Result<Project> {
    let package = project
        .directory
        .parent()
        .ok_or("project parent")?
        .join(format!("{name}.deadpan"));
    let result = success(&[
        "project",
        "copy-portable",
        project.path(),
        package.to_str().ok_or("UTF8")?,
    ])?;
    assert_eq!(
        result["portable_copy"]["revision_id"],
        project.document()?.revision_id().as_str()
    );
    let mut control = Project {
        name,
        directory: project.directory.parent().ok_or("parent")?.to_owned(),
        package,
        step: 0,
        ids: 0,
        asset: project.asset.clone(),
    };
    control.apply(|_, _, _| {
        Ok(Command::DeleteBeatSound {
            owner: node("processing")?,
            id: SoundId::new("event")?,
        })
    })?;
    for owner in [node("processing")?, control.document()?.root().clone()] {
        control.apply(|_, _, _| {
            Ok(Command::SetAudioTreatments {
                node: owner,
                treatments: Default::default(),
            })
        })?;
    }
    Ok(control)
}

fn compare_master(project: &Project, physical: &Bus, event: &[[f32; 2]]) -> Result<Value> {
    let actual = bus(project)?;
    // B(f)=round(f*8008/5): the old processor occupies[4805,11211),
    // length6406. The journal moves it to current[6406,12813), length6407.
    // Its final point12812 exhausts retained physical support. All other
    // points, including fractional leaf seam8236, carry current non-silent
    // policy. This mask and equation do not query the owner's planned ranges.
    let mut expected_mask = vec![false; SAMPLES as usize];
    expected_mask[12812] = true;
    assert_eq!(physical.mask, expected_mask);
    assert_eq!(actual.mask, expected_mask);
    assert!(
        physical.samples[8236]
            .iter()
            .any(|sample| sample.abs() > 0.0001),
        "fractional seam must carry actual nonzero Original/RoomTone"
    );
    assert!(
        actual.samples[8236]
            .iter()
            .any(|sample| sample.abs() > 0.0001)
    );
    assert_eq!(physical.samples[12812], [0.0; 2]);
    assert_eq!(actual.samples[12812], [0.0; 2]);
    let processor_drive = 10.0_f64.powf(6.0 / 20.0);
    let root_drive = 10.0_f64.powf(3.0 / 20.0);
    let mut maximum = 0.0_f64;
    let mut separate_difference = 0.0_f64;
    let mut omitted_difference = 0.0_f64;
    let mut omitted_root_difference = 0.0_f64;
    let mut peak = 0.0_f64;
    for (at, (physical, actual)) in physical.samples.iter().zip(&actual.samples).enumerate() {
        for channel in 0..2 {
            let sound = if (EVENT_START as usize..EVENT_END as usize).contains(&at) {
                f64::from(event[at - EVENT_START as usize][channel])
            } else {
                0.0
            };
            let value = f64::from(physical[channel]);
            let joined = value + sound;
            let processed = if (6406..12813).contains(&at) {
                (joined * processor_drive).tanh()
            } else {
                joined
            };
            let expected = (processed * root_drive).tanh();
            let error = (f64::from(actual[channel]) - expected).abs();
            maximum = maximum.max(error);
            peak = peak.max(f64::from(actual[channel]).abs());
            assert!(
                actual[channel].is_finite() && error < 0.000_003,
                "whole master sample{at}/channel{channel}: {} expected{expected},error{error}",
                actual[channel]
            );
            if (EVENT_START as usize..EVENT_END as usize).contains(&at) {
                let separate = ((value * processor_drive).tanh()
                    + (sound * processor_drive).tanh())
                    * root_drive;
                separate_difference = separate_difference.max((expected - separate.tanh()).abs());
                omitted_difference =
                    omitted_difference.max((expected - (joined * root_drive).tanh()).abs());
                omitted_root_difference = omitted_root_difference.max((expected - processed).abs());
            }
        }
    }
    assert!(peak < 0.9, "leave limiter headroom, peak{peak}");
    assert!(
        separate_difference > 0.0001
            && omitted_difference > 0.001
            && omitted_root_difference > 0.001
    );
    Ok(
        json!({"samples":SAMPLES,"max_master_error":maximum,"master_peak":peak,
        "wrong_separate_voice_max_difference":separate_difference,"omitted_processor_max_difference":omitted_difference,
        "omitted_root_max_difference":omitted_root_difference,
        "raw_seam8236":physical.samples[8236],"master_seam8236":actual.samples[8236],"suppressed_samples":[12812]}),
    )
}

pub fn build(directory: &Path) -> Result<Fixture> {
    let (original_path, producer) = nonzero_original(&directory.join("producer"))?;
    let mut project = Project::create_from(
        &directory.join("project"),
        "fractional-owner-support",
        &original_path,
    )?;
    register_event(&mut project)?;
    let document = project.document()?;
    let original_asset = &document.assets()[&project.asset];
    assert_eq!(
        original_asset.content_hash,
        format!(
            "blake3:{}",
            blake3::hash(&fs::read(&original_path)?).to_hex()
        )
    );
    assert_eq!(
        original_asset.video.ok_or("Original video")?.end().ticks,
        120120
    );
    assert_eq!(
        original_asset.audio.ok_or("Original audio")?,
        span(0, 192192, 48000)?
    );
    assert_eq!(
        project.document()?.presentation_basis().frame_rate,
        FrameRate::new(30000, 1001)?
    );
    project.delete_range(0, 120)?;
    let source = SourceAudio {
        asset: project.asset.clone(),
        span: span(3100, 23100, 48000)?,
    };
    let room = |length| -> Result<BeatNode> {
        Ok(BeatNode::hold(
            "Real nonzero Original RoomTone",
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
        let mut result = BeatNode::sequence("Nonzero Original", vec![]);
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
    let mut stage = BeatNode::sequence("Shared nonlinear Preserve output", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("body")?,
        duration: FrameDuration::new(4)?,
        mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(8))?,
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    let mut nodes = BTreeMap::from([
        (node("lead")?, room(3)?),
        (node("processing")?, stage),
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
    for recipe in nodes.values_mut() {
        recipe.audio_edges.node_start = AudioEdgePolicy::Hard;
        recipe.audio_edges.node_end = AudioEdgePolicy::Hard;
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
    // Match the hard node edges on the inserted tree at its root, too. The
    // independent equation can then shape the complete physical bus without
    // a coincident ancestor envelope modifying the hard-edged event.
    for edge in [
        deadpan_core::AudioBoundaryKind::NodeStart,
        deadpan_core::AudioBoundaryKind::NodeEnd,
    ] {
        project.apply(|_, document, _| {
            Ok(Command::SetAudioEdge {
                node: document.root().clone(),
                edge,
                policy: AudioEdgePolicy::Hard,
            })
        })?;
    }
    for (owner, drive) in [
        (node("processing")?, 6000),
        (project.document()?.root().clone(), 3000),
    ] {
        project.apply(|_, _, _| {
            Ok(Command::SetAudioTreatments {
                node: owner,
                treatments: AudioTreatments::default()
                    .with_saturation(Some(Saturation::new(GainDb::new(drive)?)?))?,
            })
        })?;
    }
    let event_source = SourceAudio {
        asset: AssetId::new("event")?,
        span: span(3100, 8005, 44100)?,
    };
    project.apply(|_, _, _| {
        Ok(Command::SetBeatSound {
            owner: node("processing")?,
            id: SoundId::new("event")?,
            event: BeatSound {
                label: "Independent44.1k voice".into(),
                mapping: SourceAudioMapping::natural_rate(
                    event_source.span,
                    FrameRate::new(30000, 1001)?,
                )?,
                source: event_source,
                offset: AudioSample(37),
                treatments: AudioTreatments::default().with_trim(GainDb::new(-18000)?)?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: node("lead")?,
            duration: FrameDuration::new(4)?,
        })
    })?;
    let before = project.document()?;
    let event = event_reference()?;
    assert_eq!(event.len(), 5339);
    let physical_before = bus(&physical_control(&project, "physical-before")?)?;
    let before_report = compare_master(&project, &physical_before, &event)?;
    project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: node("hold")?,
            duration: FrameDuration::new(3)?,
        })
    })?;
    let after = project.document()?;
    assert_eq!(
        after.nodes()[&node("processing")?],
        before.nodes()[&node("processing")?]
    );
    assert_eq!(
        after.audio_bindings().sound_clocks(),
        before.audio_bindings().sound_clocks()
    );
    assert_eq!(after.beat_sounds(), before.beat_sounds());
    let physical_after = bus(&physical_control(&project, "physical-after")?)?;
    let after_report = compare_master(&project, &physical_after, &event)?;
    let physical_change = physical_before
        .samples
        .iter()
        .flatten()
        .zip(physical_after.samples.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        physical_change > 0.0001,
        "Hold change must rebuild a nonzero physical operand"
    );
    let report = json!({"producer":producer,"before":before_report,"after":after_report,"physical_change_maximum":physical_change,
        "event_source_sha256":hash(&event_wav()?)?,"event_native":[3100,8005],"event_support":[EVENT_START,EVENT_END],"event_trim_db":-18,
        "retained_event_phase":"2480147/800","event_step":"147/160","nonlinear_equation":"root=tanh(10^(3/20)*processor); processor=tanh(10^(6/20)*(current_physical+independent_event)) in[6406,12813)",
        "control":"portable copy with identical committed history, then public DeleteBeatSound and SetAudioTreatments(default) only"});
    fs::write(
        directory.join("whole-master-oracle.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let pictures = [
        Expected::Background,
        Expected::Background,
        Expected::Background,
        Expected::Background,
        original(13),
        Expected::Background,
        Expected::Background,
        original(13),
        original(12),
        original(13),
        original(14),
    ];
    project.finish(vec!["Retime","Hold (insert time)","Group mixing","Saturation","Attached sounds"],
        pictures.into_iter().enumerate().map(|(frame,picture)|(frame as u64,picture)).collect(),
        vec![format!("Complete nonzero Original/RoomTone and independent44.1k event; fractional Preserve owner seam8236 and exhausted sample12812; full master oracle {report}")])
}
