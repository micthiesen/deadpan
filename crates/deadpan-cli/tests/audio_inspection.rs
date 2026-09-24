#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_cli::audio::ProjectAudioSession;
use deadpan_core::{
    AssetId, AudioSample, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration,
    FrameRange, FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind, PitchPolicy,
    PresentationBasis, ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceAudio,
    SourceSpan, SourceTimeBase, SourceTimestamp, SplitIdentities, Subtree,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits, SourceAudioSample};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn asset() -> AssetId {
    AssetId::new("camera").unwrap()
}
fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests")
        .join(if name.ends_with(".wav") {
            "audio-fixtures"
        } else {
            "fixtures"
        })
        .join(name)
        .canonicalize()
        .unwrap()
}
fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("audio.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("audio-inspection")?,
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}

fn register(
    store: &mut ProjectStore,
    path: &Path,
    stream: u32,
    ownership: OriginalOwnership,
    next: &str,
) -> Result<Vec<[f32; 2]>> {
    let original = store
        .retain_original(path, ownership, limits(), &active())?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let audio = AudioSession::open_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        stream,
        AudioSessionLimits::default(),
        &active(),
    )?;
    let first = audio
        .index()
        .frames()
        .iter()
        .find(|frame| frame.valid_start < frame.valid_end)
        .unwrap()
        .valid_start;
    let original_block = audio.read_samples(
        SourceAudioSample(first),
        256,
        Duration::from_secs(2),
        &active(),
    )?;
    let expected = original_block
        .samples
        .chunks_exact(2)
        .map(|frame| [frame[0], frame[1]])
        .collect();
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio))?;
    store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: revision(next),
            original: original.object().content().clone(),
            new_asset_id: asset(),
            label: "Measured audio".into(),
            insertion: Some(SourceInsertionRequest {
                parent: node("root"),
                index: 0,
                node: node("clip"),
                label: "Full source audio".into(),
                purpose: Default::default(),
            }),
        },
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    Ok(expected)
}

fn commit(store: &mut ProjectStore, next: &str, command: Command) -> Result {
    let before = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision(next),
        command,
    })?;
    Ok(())
}

fn inspect(path: &Path, start: &str, end: &str, succeeds: bool) -> Result<Value> {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "inspect-audio",
            path.to_str().unwrap(),
            "--samples",
            start,
            end,
        ])
        .output()?;
    assert_eq!(
        output.status.success(),
        succeeds,
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if succeeds {
        assert!(output.stderr.is_empty());
        Ok(serde_json::from_slice(&output.stdout)?)
    } else {
        assert!(output.stdout.is_empty());
        Ok(serde_json::from_slice(&output.stderr)?)
    }
}

fn counts(path: &Path) -> Result<(i64, i64, i64)> {
    let database = rusqlite::Connection::open(path.join("project.sqlite"))?;
    Ok(database.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM source_qualifications)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

#[test]
fn room_tone_inspection_loops_an_explicit_aac_range_and_retains_silent_time() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "import",
    )?;
    let clock = SourceTimeBase::new(1, 48_000)?;
    let selected = SourceAudio {
        asset: asset(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: 256,
                time_base: clock,
            },
        )?,
    };
    for (name, index, frames, audio) in [
        ("ambience", 0, 2, HoldAudio::RoomTone { source: selected }),
        ("silence", 1, 1, HoldAudio::Silence),
    ] {
        commit(
            &mut store,
            name,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(
                        node(name),
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: FrameDuration::new(frames)?,
                                video: HoldVideo::Background,
                                audio,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                },
            },
        )?;
    }
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let mut session = ProjectAudioSession::open(&path)?;
    let block = session.read_time_mapped(AudioSample(0), 256, &active())?;
    assert!(block.suppressed.is_empty());
    for (at, actual) in block.samples.iter().enumerate() {
        let phase = at % 160;
        let expected = if at < 160 || phase >= 96 {
            original[phase]
        } else {
            let weight = phase as f64 / 96.0;
            std::array::from_fn(|channel| {
                (f64::from(original[160 + phase][channel]) * (1.0 - weight)
                    + f64::from(original[phase][channel]) * weight) as f32
            })
        };
        assert_eq!(*actual, expected, "crossfade sample {at}");
    }
    let silent = session.read_time_mapped(AudioSample(3200), 256, &active())?;
    assert_eq!(silent.samples, vec![[0.0; 2]; 256]);
    assert_eq!(
        silent.suppressed,
        vec![AudioSample(3200)..AudioSample(3456)]
    );
    assert_eq!(
        session
            .read_time_mapped(AudioSample(4800), 256, &active())?
            .samples,
        original
    );
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "inspect-audio",
            path.to_str().unwrap(),
            "--samples",
            "0",
            "256",
            "--time-mapped",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let wire: Value = serde_json::from_slice(&output.stdout)?;
    let pcm: Vec<[f32; 2]> = serde_json::from_value(wire["audio"]["samples"].clone())?;
    assert_eq!(pcm, block.samples);
    assert_eq!(wire["audio"]["revision_id"], "silence");
    assert_eq!(wire["audio"]["stage"], "time_mapped_pcm_before_effects");
    let faded = session.read_edge_faded(AudioSample(0), 256, &active())?;
    let expected_faded: Vec<_> = block
        .samples
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            let gain = 1.0_f32.min((at as f32 + 0.5) / 96.0);
            sample.map(|value| value * gain)
        })
        .collect();
    assert_eq!(faded.samples, expected_faded);
    assert!(faded.suppressed.is_empty());
    let faded_silence = session.read_edge_faded(AudioSample(3200), 256, &active())?;
    assert_eq!(faded_silence.samples, silent.samples);
    assert_eq!(faded_silence.suppressed, silent.suppressed);
    let faded_output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "inspect-audio",
            path.to_str().unwrap(),
            "--samples",
            "0",
            "256",
            "--edge-faded",
        ])
        .output()?;
    assert!(
        faded_output.status.success(),
        "{}",
        String::from_utf8_lossy(&faded_output.stderr)
    );
    assert!(faded_output.stderr.is_empty());
    let faded_wire: Value = serde_json::from_slice(&faded_output.stdout)?;
    assert_eq!(faded_wire["protocol"], 1);
    assert_eq!(
        faded_wire["audio"]["stage"],
        "edge_faded_pcm_before_voice_effects"
    );
    assert_eq!(faded_wire["audio"]["engine"], deadpan_audio::EDGE_FADE_ID);
    assert_eq!(
        faded_wire["audio"]["processing_order"],
        json!(["time_pitch_mapping", "edge_fades"])
    );
    assert_eq!(faded_wire["audio"]["revision_id"], "silence");
    let faded_pcm: Vec<[f32; 2]> = serde_json::from_value(faded_wire["audio"]["samples"].clone())?;
    assert_eq!(faded_pcm, faded.samples);
    assert_eq!(
        inspect(&path, "0", "256", false)?["error"]["code"],
        "AudioOperationUnsupported"
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn mapped_inspection_prepares_preserve_from_historical_aac_without_writing() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "import",
    )?;
    let document = store.snapshot()?;
    let clip = document.nodes()[&node("clip")].clone();
    let length = document.node_duration(&node("clip"))?.frames();
    commit(&mut store, "remove", Command::Delete { node: node("clip") })?;
    commit(
        &mut store,
        "slow",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("slow"),
                nodes: BTreeMap::from([
                    (node("copy"), clip),
                    (
                        node("slow"),
                        BeatNode {
                            audio_edges: Default::default(),
                            label: "Preserve speech pitch".into(),
                            kind: NodeKind::Retime {
                                purpose: deadpan_core::RetimePurpose::Edit,
                                child: node("copy"),
                                duration: FrameDuration::new(length * 2)?,
                                mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(length))?,
                                pitch: PitchPolicy::Preserve,
                            },
                        },
                    ),
                ]),
                overrides: BTreeMap::new(),
            },
        },
    )?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let mut session = ProjectAudioSession::open(&path)?;
    assert!(session.read(AudioSample(0), 256, &active()).is_err());
    let mapped = session.read_time_mapped(AudioSample(0), 256, &active())?;
    assert_eq!(mapped.stage, "time_mapped_pcm_before_effects");
    assert_eq!(mapped.revision_id, revision("slow"));
    assert!(
        mapped
            .samples
            .iter()
            .flatten()
            .any(|sample| sample.abs() > 0.01)
    );
    assert!(mapped.suppressed.is_empty());
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "inspect-audio",
            path.to_str().unwrap(),
            "--samples",
            "0",
            "256",
            "--time-mapped",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let wire: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(wire["protocol"], 1);
    let mut actual_metadata = wire["audio"].as_object().unwrap().clone();
    let actual_samples: Vec<[f32; 2]> =
        serde_json::from_value(actual_metadata.remove("samples").unwrap())?;
    let mut expected_metadata = serde_json::to_value(&mapped)?.as_object().unwrap().clone();
    expected_metadata.remove("samples");
    assert_eq!(actual_metadata, expected_metadata);
    // A JSON f64 parse can differ from direct f32-to-Value promotion. Compare
    // the PCM in its actual f32 format, including every bit and signed zero.
    assert_eq!(actual_samples.len(), mapped.samples.len());
    for (frame, (actual, expected)) in actual_samples.iter().zip(&mapped.samples).enumerate() {
        for channel in 0..2 {
            assert_eq!(
                actual[channel].to_bits(),
                expected[channel].to_bits(),
                "PCM differs at frame {frame}, channel {channel}"
            );
        }
    }
    for (start, end, mode) in [
        ("0", "257", "--time-mapped"),
        ("-1", "1", "--time-mapped"),
        ("0", "257", "--edge-faded"),
        ("-1", "1", "--edge-faded"),
    ] {
        let invalid = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
            .args([
                "inspect-audio",
                path.to_str().unwrap(),
                "--samples",
                start,
                end,
                mode,
            ])
            .output()?;
        assert!(!invalid.status.success());
        assert!(invalid.stdout.is_empty());
        let error: Value = serde_json::from_slice(&invalid.stderr)?;
        assert_eq!(error["error"]["code"], "AudioRangeOutOfRange");
    }
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn actual_aac_pcm_inspection_is_read_only_beside_writer_and_matches_original_impulse() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let expected = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "registered",
    )?;
    // This fixture's encoded source event is at original sample 100. Compare
    // the actual independent decoder oracle, without assuming lossy amplitude.
    let peak = expected
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left[0].abs().total_cmp(&right[0].abs()))
        .unwrap()
        .0;
    assert_eq!(peak, 100);
    assert!(expected[peak][0] > 0.1);
    assert!(expected[peak][1] < -0.1);
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let report = inspect(&path, "0", "256", true)?;
    assert_eq!(report["protocol"], 1);
    assert_eq!(report["audio"]["schema_version"], 1);
    assert_eq!(report["audio"]["stage"], "source_pcm_before_effects");
    assert_eq!(report["audio"]["revision_id"], "registered");
    assert_eq!(report["audio"]["start"], 0);
    assert_eq!(
        serde_json::from_value::<Vec<[f32; 2]>>(report["audio"]["samples"].clone())?,
        expected
    );
    let mut session = ProjectAudioSession::open(&path)?;
    assert_eq!(session.revision(), before.revision_id());
    let block = session.read(AudioSample(83), 31, &active())?;
    assert_eq!(block.samples, expected[83..114]);
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn source_repeat_gaps_and_silent_hold_boundaries_follow_exact_sequence_allocation() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let expected = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "registered",
    )?;
    let child_frames = store.snapshot()?.duration()?.frames();
    commit(
        &mut store,
        "repeated",
        Command::WrapRepeat {
            node: node("clip"),
            id: node("repeat"),
            plays: 2,
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
            anchor_policy: Default::default(),
        },
    )?;
    let mut session = ProjectAudioSession::open(&path)?;
    let rate = session.plan().metadata().presentation_basis.frame_rate;
    let gap = rate.audio_boundary(ProjectFrame(child_frames))?;
    let second = rate.audio_boundary(ProjectFrame(child_frames + 1))?;
    assert_eq!(
        session.read(gap, 256, &active())?.samples,
        vec![[0.0; 2]; 256]
    );
    assert_eq!(session.read(second, 256, &active())?.samples, expected);
    let across = session.read(AudioSample(second.0 - 17), 128, &active())?;
    assert_eq!(across.samples[..17], [[0.0; 2]; 17]);
    assert_eq!(across.samples[17..], expected[..111]);
    let report = inspect(&path, &gap.0.to_string(), &(gap.0 + 32).to_string(), true)?;
    assert_eq!(report["audio"]["samples"], json!(vec![[0.0; 2]; 32]));
    Ok(())
}

#[test]
fn frozen_session_resolves_historical_receipt_after_undo_and_asset_alias_reuse() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let expected = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "first-import",
    )?;
    // Keep the reader cold until after the current alias means another source.
    let mut frozen = ProjectAudioSession::open(&path)?;
    store.undo(&revision("first-import"), revision("undone"))?;
    let replacement = register(
        &mut store,
        &fixture("offset-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "second-import",
    )?;
    assert_ne!(replacement, expected);
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    assert_eq!(frozen.revision(), &revision("first-import"));
    assert_eq!(
        frozen.read(AudioSample(0), 256, &active())?.samples,
        expected
    );
    let mut current = ProjectAudioSession::open(&path)?;
    assert_eq!(current.revision(), &revision("second-import"));
    assert_eq!(
        current.read(AudioSample(0), 256, &active())?.samples,
        replacement
    );
    assert_eq!(
        frozen.read(AudioSample(90), 21, &active())?.samples,
        expected[90..111]
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn invalid_ranges_and_cancellation_fail_without_returning_partial_pcm() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "registered",
    )?;
    let before = store.snapshot()?;
    for (start, end, code) in [
        ("-1", "1", "AudioRangeOutOfRange"),
        ("0", "257", "AudioRangeOutOfRange"),
        ("2", "1", "AudioRangeOutOfRange"),
        ("0", "0", "AudioRangeOutOfRange"),
        ("0", "bad", "InvalidInput"),
    ] {
        assert_eq!(inspect(&path, start, end, false)?["error"]["code"], code);
    }
    let mut session = ProjectAudioSession::open(&path)?;
    assert!(session.read(AudioSample(0), 0, &active()).is_err());
    assert!(session.read(AudioSample(0), 257, &active()).is_err());
    assert!(session.read(AudioSample(-1), 1, &active()).is_err());
    assert!(
        session
            .read(AudioSample(0), 1, &AtomicBool::new(true))
            .is_err()
    );
    assert_eq!(session.read(AudioSample(0), 1, &active())?.samples.len(), 1);
    assert_eq!(store.snapshot()?, before);
    Ok(())
}

#[test]
fn missing_and_corrupt_linked_originals_fail_before_pcm_is_exposed() -> Result {
    for corrupt in [false, true] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = project(scratch.path())?;
        let local = scratch.path().join("linked.mp4");
        fs::copy(fixture("cfr-bframes.mp4"), &local)?;
        register(
            &mut store,
            &local,
            1,
            OriginalOwnership::Linked { bookmark: None },
            "registered",
        )?;
        let before = store.snapshot()?;
        let mut session = ProjectAudioSession::open(&path)?;
        if corrupt {
            let mut bytes = fs::read(&local)?;
            bytes[100] ^= 1;
            fs::write(&local, bytes)?;
        } else {
            fs::remove_file(&local)?;
        }
        assert!(session.read(AudioSample(0), 256, &active()).is_err());
        assert!(
            session
                .read_domain(AudioSample(0), AudioSample(0), 256, &active())
                .is_err()
        );
        let error = inspect(&path, "0", "256", false)?;
        assert_eq!(error["error"]["code"], "SourceAudioUnavailable");
        assert!(
            error["error"]["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
        assert_eq!(store.snapshot()?, before);
    }
    Ok(())
}

#[test]
fn unspecified_layout_and_legacy_unqualified_assets_are_not_guessed() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("pcm-stereo-48000.wav"),
        0,
        OriginalOwnership::Managed,
        "registered",
    )?;
    let error = inspect(&path, "0", "16", false)?;
    assert_eq!(error["error"]["code"], "AudioLayoutUnsupported");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("layout")
    );
    let mut legacy = serde_json::to_value(store.snapshot()?)?;
    legacy["assets"]["camera"]["source_qualification"] = Value::Null;
    let legacy = ProjectDocument::from_json(&serde_json::to_string(&legacy)?)?;
    let legacy_path = scratch.path().join("legacy.deadpan");
    let legacy_store = ProjectStore::create(&legacy_path, &legacy)?;
    let mut session = ProjectAudioSession::open(&legacy_path)?;
    assert!(session.read(AudioSample(0), 16, &active()).is_err());
    let error = inspect(&legacy_path, "0", "16", false)?;
    assert_eq!(error["error"]["code"], "SourceAudioUnavailable");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("qualification")
    );
    assert_eq!(legacy_store.snapshot()?, legacy);
    Ok(())
}

#[test]
fn physical_domain_cli_reads_hidden_negative_source_and_retained_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let expected = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "registered",
    )?;
    commit(
        &mut store,
        "split",
        Command::Split {
            node: node("clip"),
            at: FrameDuration::new(1)?,
            identities: SplitIdentities {
                nodes: vec![node("left"), node("right"), node("right-context")],
            },
        },
    )?;
    commit(
        &mut store,
        "only-right",
        Command::Delete { node: node("left") },
    )?;
    let before = store.snapshot()?;
    let context = deadpan_core::FrozenAudioContext::capture(&before)?;
    let mut session = ProjectAudioSession::open(&path)?;
    let block = session.read_domain(AudioSample(0), AudioSample(-1600), 256, &active())?;
    assert_eq!(block.samples, expected);
    assert_eq!(block.root_samples.start, AudioSample(-1600));
    assert_eq!(block.visible_samples.start, AudioSample(0));
    assert_eq!(block.instance.node, node("right-context"));

    let invoke = |probe: &str, start: &str, end: &str| {
        ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
            .args([
                "inspect-audio-domain",
                path.to_str().unwrap(),
                "--at",
                probe,
                "--samples",
                start,
                end,
            ])
            .output()
    };
    let output = invoke("0", "-1600", "-1344")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(actual["protocol"], 1);
    assert_eq!(
        actual["audio"]["stage"],
        "physical_domain_pcm_before_effects"
    );
    assert_eq!(actual["audio"]["start"], -1600);
    let decoded: Vec<[f32; 2]> = serde_json::from_value(actual["audio"]["samples"].clone())?;
    assert_eq!(decoded, expected);
    for (probe, start, end) in [
        ("-1", "-1600", "-1344"),
        ("0", "-1601", "-1600"),
        ("0", "0", "257"),
        ("0", "0", "0"),
        ("0", "9223372036854775807", "-9223372036854775808"),
    ] {
        let output = invoke(probe, start, end)?;
        assert!(!output.status.success());
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["code"], "AudioRangeOutOfRange");
    }
    assert_eq!(store.snapshot()?, before);
    commit(
        &mut store,
        "delete-right",
        Command::Delete {
            node: node("right"),
        },
    )?;
    let after = store.snapshot()?;
    let mut historical = ProjectAudioSession::open_context(&path, &context)?;
    assert_eq!(
        historical
            .read_domain(AudioSample(0), AudioSample(-1600), 256, &active())?
            .samples,
        expected
    );
    assert_eq!(store.snapshot()?, after);
    Ok(())
}
