//! Complete real-media PCM and picture oracle for neutral endpoint grouping.
//! All authored writes go through actual CLI command/macro entrypoints.
use super::*;
use deadpan_cli::audio::ProjectAudioSession;
use deadpan_core::{BeatSound, EqualizerRecipe};
use deadpan_store::single_source::SingleSourceState;
use std::sync::atomic::AtomicBool;

#[path = "aligned_group_recipe.rs"]
pub mod aligned;

pub const TOTAL_SAMPLES: i64 = 193_794;

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn equalizer(gains: [i32; 3]) -> Result<AudioTreatments> {
    Ok(
        AudioTreatments::default().with_equalizer(Some(EqualizerRecipe::new(
            gains.map(|gain| GainDb::new(gain).unwrap()),
            None,
            None,
        )?))?,
    )
}

#[derive(Debug, PartialEq, serde::Serialize)]
struct CanonicalPcm {
    authored_bits: Vec<[u32; 2]>,
    limited_bits: Vec<[u32; 2]>,
    limiter_gain_bits: Vec<u64>,
}

fn read_canonical(path: &Path, partitions: &[u32]) -> Result<CanonicalPcm> {
    assert!(!partitions.is_empty());
    assert!(partitions.iter().all(|count| (1..=256).contains(count)));
    let mut session = ProjectAudioSession::open(path)?;
    assert_eq!(session.plan().audio_duration()?.0, TOTAL_SAMPLES);
    let cancelled = AtomicBool::new(false);
    let mut result = CanonicalPcm {
        authored_bits: Vec::with_capacity(usize::try_from(TOTAL_SAMPLES)?),
        limited_bits: Vec::with_capacity(usize::try_from(TOTAL_SAMPLES)?),
        limiter_gain_bits: Vec::with_capacity(usize::try_from(TOTAL_SAMPLES)?),
    };
    let mut at = 0;
    let mut partition = 0;
    while at < TOTAL_SAMPLES {
        let count = u32::try_from((TOTAL_SAMPLES - at).min(i64::from(partitions[partition])))?;
        let authored = session.read_authored_bus(AudioSample(at), count, &cancelled)?;
        let limited = session.read_limited(AudioSample(at), count, &cancelled)?;
        assert_eq!(authored.stage, "authored_bus_pcm_before_mastering");
        assert_eq!(authored.samples.len(), usize::try_from(count)?);
        assert_eq!(limited.samples.len(), usize::try_from(count)?);
        assert_eq!(limited.gain.len(), usize::try_from(count)?);
        for sample in authored.samples.iter().chain(&limited.samples) {
            assert!(sample.iter().all(|value| value.is_finite()));
        }
        assert!(
            limited
                .gain
                .iter()
                .all(|gain| gain.is_finite() && (0.0..=1.0).contains(gain))
        );
        result.authored_bits.extend(
            authored
                .samples
                .into_iter()
                .map(|sample| sample.map(f32::to_bits)),
        );
        result.limited_bits.extend(
            limited
                .samples
                .into_iter()
                .map(|sample| sample.map(f32::to_bits)),
        );
        result
            .limiter_gain_bits
            .extend(limited.gain.into_iter().map(f64::to_bits));
        at += i64::from(count);
        partition = (partition + 1) % partitions.len();
    }
    Ok(result)
}

fn same_pcm(expected: &CanonicalPcm, actual: &CanonicalPcm, context: &str) {
    for (kind, expected, actual) in [
        ("authored", &expected.authored_bits, &actual.authored_bits),
        ("limited", &expected.limited_bits, &actual.limited_bits),
    ] {
        assert_eq!(expected.len(), usize::try_from(TOTAL_SAMPLES).unwrap());
        assert_eq!(actual.len(), expected.len());
        for (at, (expected, actual)) in expected.iter().zip(actual).enumerate() {
            assert_eq!(actual, expected, "{context}: {kind} sample {at}");
        }
    }
    assert_eq!(
        actual.limiter_gain_bits.len(),
        expected.limiter_gain_bits.len()
    );
    for (at, (expected, actual)) in expected
        .limiter_gain_bits
        .iter()
        .zip(&actual.limiter_gain_bits)
        .enumerate()
    {
        assert_eq!(actual, expected, "{context}: limiter gain {at}");
    }
}

fn cold_seams(path: &Path, expected: &CanonicalPcm, context: &str) -> Result {
    // B(5), B(11), finite event end, independent root event and project terminal.
    for (first, count) in [(8007, 3), (17617, 3), (17653, 5), (64063, 3), (193791, 3)] {
        let mut session = ProjectAudioSession::open(path)?;
        let cancelled = AtomicBool::new(false);
        let authored = session.read_authored_bus(AudioSample(first), count, &cancelled)?;
        let limited = session.read_limited(AudioSample(first), count, &cancelled)?;
        let start = usize::try_from(first)?;
        let end = start + usize::try_from(count)?;
        assert_eq!(
            authored
                .samples
                .into_iter()
                .map(|sample| sample.map(f32::to_bits))
                .collect::<Vec<_>>(),
            expected.authored_bits[start..end],
            "{context}: authored cold seam at {first}",
        );
        assert_eq!(
            limited
                .samples
                .into_iter()
                .map(|sample| sample.map(f32::to_bits))
                .collect::<Vec<_>>(),
            expected.limited_bits[start..end],
            "{context}: limited cold seam at {first}",
        );
        assert_eq!(
            limited
                .gain
                .into_iter()
                .map(f64::to_bits)
                .collect::<Vec<_>>(),
            expected.limiter_gain_bits[start..end],
            "{context}: cold gain at {first}",
        );
    }
    Ok(())
}

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

fn check_pictures(project: &Project, document: &ProjectDocument) -> Result {
    assert_eq!(document.duration()?.frames(), 121);
    for frame in 0..121 {
        let sampled = success(&[
            "inspect-plan",
            project.path(),
            "--frame",
            &frame.to_string(),
        ])?;
        assert_eq!(
            sampled["sample"]["revision_id"],
            json!(document.revision_id())
        );
        let picture = &sampled["sample"]["picture"];
        assert_eq!(
            picture["type"],
            if frame == 0 { "freeze" } else { "source" }
        );
        let point: SourcePoint = serde_json::from_value(picture["point"].clone())?;
        assert_eq!(
            point.time_base,
            deadpan_core::SourceTimeBase::new(1, 30000)?
        );
        if frame != 0 {
            assert_eq!(picture["endpoints"], "hold_adjacent");
            let span: ExactSourceSpan = serde_json::from_value(picture["selection"].clone())?;
            assert_eq!(span.start().time_base, point.time_base);
            assert_eq!(span.end().time_base, point.time_base);
            assert!(!point.ticks.compare(span.start().ticks).is_lt());
            assert!(point.ticks.compare(span.end().ticks).is_lt());
        }
        assert_eq!(
            u64::try_from(point.ticks.checked_div(ExactRatio::integer(1001))?.floor())?,
            if frame == 0 { 0 } else { frame - 1 },
            "literal source identity at Edit {frame}",
        );
    }
    Ok(())
}

pub fn build(directory: &Path) -> Result<Fixture> {
    // Parent helper uses full create-original, with measured source retention.
    // Do not call shorten and do not synthesize AssetRecords or receipts.
    let mut project = Project::create(directory, "sound-group-endpoints")?;
    let initial = project.document()?;
    assert_eq!(initial.duration()?.frames(), 120);
    assert_eq!(
        serde_json::to_value(initial.presentation_basis().frame_rate)?,
        json!({"numerator":30000,"denominator":1001})
    );
    assert_eq!(boundary(121)?, TOTAL_SAMPLES);
    // Independent literal round-to-nearest argument: 968968/5 = 193793 + 3/5.
    let numerator = (i128::from(initial.duration()?.frames()) + 1) * 8008;
    assert_eq!(numerator, 968_968);
    assert_eq!(numerator / 5, 193_793);
    assert_eq!(numerator % 5, 3);
    assert_eq!(numerator / 5 + 1, i128::from(TOTAL_SAMPLES));

    let owner = root_children(&initial)?[0].clone();
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let Some(SingleSourceState::Ready {
        asset,
        baseline_revision,
        qualification,
        ..
    }) = store.single_source_state()?
    else {
        return Err("qualified full Original required".into());
    };
    assert_eq!(initial.revision_id(), &baseline_revision);
    let receipt = store.registered_source(&baseline_revision, &asset)?;
    assert_eq!(receipt.id(), &qualification);
    assert_eq!(
        receipt
            .snapshot()
            .video()
            .ok_or("qualified video")?
            .index()
            .index()
            .frames()
            .len(),
        120
    );
    assert_eq!(
        receipt
            .snapshot()
            .audio()
            .ok_or("qualified audio")?
            .stream()
            .sample_rate,
        48_000
    );
    let provenance = json!({
        "asset":asset, "baseline_revision":baseline_revision, "qualification":qualification,
        "full_original_frames":120, "sample_rate":48000,
        "event_source_samples":[40000,56016], "event_offset_samples":37,
        "event_eq_millidecibels":[6000,-6000,3000],
        "owner_eq_millidecibels":[3000,-1500,1000],
    });
    drop(receipt);
    drop(store);

    let NodeKind::Source { source } = &initial.nodes()[&owner].kind else {
        return Err("full Original owner must be Source".into());
    };
    let original_audio = source.audio.as_ref().ok_or("Original audio")?;
    assert_eq!(original_audio.asset, asset);
    let tb = deadpan_core::SourceTimeBase::new(1, 48_000)?;
    assert_eq!(
        original_audio.span.start(),
        SourceTimestamp {
            ticks: 0,
            time_base: tb
        }
    );
    assert!(original_audio.span.end().ticks >= 56_016);
    // Includes the known click at Original sample 48000. The selected finite
    // event is exactly 16016 samples (10 NTSC frames) and is independent.
    let finite_source = SourceAudio {
        asset: asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 40_000,
                time_base: tb,
            },
            SourceTimestamp {
                ticks: 56_016,
                time_base: tb,
            },
        )?,
    };
    project.apply(|_, document, _| {
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: SoundId::new("attached")?,
            event: BeatSound {
                label: "Finite Original audio event".into(),
                source: finite_source.clone(),
                mapping: SourceAudioMapping::natural_rate(
                    finite_source.span,
                    document.presentation_basis().frame_rate,
                )?,
                offset: AudioSample(37),
                treatments: equalizer([6000, -6000, 3000])?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: owner.clone(),
            treatments: equalizer([3000, -1500, 1000])?,
        })
    })?;
    project.apply(|project, document, revision| {
        let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
        let plan = deadpan_plan::RenderPlan::compile(document)?;
        let provider =
            deadpan_cli::pause::pause_provider(document, &plan, ProjectFrame(0), &mut |asset| {
                store
                    .source_video_index(document.revision_id(), asset)
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            })?;
        assert!(matches!(provider.video, HoldVideo::Freeze { .. }));
        assert!(
            document
                .insert_time_target(ProjectFrame(0))?
                .split
                .is_none()
        );
        Ok(Command::InsertTime {
            at: ProjectFrame(0),
            hold: HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: provider.video,
                picture_context: provider.picture_context,
                audio: HoldAudio::Silence,
            },
            id: node("freeze-prefix")?,
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    // Separate root sound coexists with the owned event. Add it after prefix,
    // then require byte-identical recipe preservation through neutral grouping.
    let root_source = SourceAudio {
        asset: asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 48_000,
                time_base: tb,
            },
            SourceTimestamp {
                ticks: 52_800,
                time_base: tb,
            },
        )?,
    };
    project.apply(|_, document, _| {
        Ok(Command::SetSound {
            id: SoundId::new("root-effect")?,
            event: SoundEvent {
                owner: document.root().clone(),
                label: "Independent root event".into(),
                source: root_source.clone(),
                mapping: SourceAudioMapping::natural_rate(
                    root_source.span,
                    document.presentation_basis().frame_rate,
                )?,
                offset: AudioSample(64_064),
                treatments: AudioTreatments::default().with_trim(GainDb::new(-18000)?)?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    let before = project.document()?;
    let old_root = serde_json::to_vec(before.sounds())?;
    let journal = &before.audio_bindings().sound_clocks()[&owner][&SoundId::new("attached")?];
    assert_eq!(journal.scope(), &owner);
    assert_ne!(journal.scope(), before.root());
    assert_eq!(journal.clocks().len(), 1);
    let selection = deadpan_core::SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(5), ProjectFrame(11))?,
    };
    assert_eq!(
        before
            .group_selection(before.root(), &selection)?
            .required_split_ids,
        5
    );
    check_pictures(&project, &before)?;
    let oracle = read_canonical(&project.package, &[256])?;
    assert!(
        oracle.authored_bits[8008..17618]
            .iter()
            .flatten()
            .any(|sample| f32::from_bits(*sample).abs() > 0.001)
    );
    fs::write(
        directory.join("sound-group-endpoints-before-pcm.json"),
        serde_json::to_vec(&json!({
            "sample_count":TOTAL_SAMPLES,"channels":2,"encoding":"IEEE float bits",
            "independent_DSP_claim":false,"canonical":oracle,
        }))?,
    )?;
    let output = project.macro_operation(json!({
        "type":"apply", "parent":before.root(), "cursor":5,
        "program":{"instructions":[
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":6},
            {"type":"group","selector":{"type":"visual_selection"},"label":"Bounded sound group"}
        ]}
    }), false)?;
    fs::write(
        directory.join("sound-group-endpoints-macro.stdout.json"),
        &output.stdout,
    )?;
    fs::write(
        directory.join("sound-group-endpoints-macro.stderr.txt"),
        &output.stderr,
    )?;
    assert!(
        output.status.success(),
        "macro: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace: Value = serde_json::from_slice(&output.stdout)?;
    let grouped_trace = trace["trace"]
        .as_array()
        .ok_or("macro trace")?
        .iter()
        .find(|row| row["instruction"]["type"] == "group")
        .ok_or("group trace")?;
    assert_eq!(grouped_trace["resolved_range"], json!({"start":5,"end":11}));

    let after = project.document()?;
    assert_ne!(after.revision_id(), before.revision_id());
    assert_eq!(after.duration()?.frames(), 121);
    assert_eq!(after.beat_sounds().len(), 3);
    assert_eq!(serde_json::to_vec(after.sounds())?, old_root);
    check_pictures(&project, &after)?;
    same_pcm(
        &oracle,
        &read_canonical(&project.package, &[251, 37, 173])?,
        "group",
    );
    cold_seams(&project.package, &oracle, "group")?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = project.document()?;
    same_authored(&undone, &before)?;
    same_pcm(
        &oracle,
        &read_canonical(&project.package, &[256])?,
        "durable undo",
    );
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = project.document()?;
    same_authored(&restored, &after)?;
    same_pcm(
        &oracle,
        &read_canonical(&project.package, &[173, 251, 37])?,
        "durable redo",
    );
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &restored)?;
    let history = store.validate_report(true)?;
    drop(store);

    fs::write(
        directory.join("sound-group-endpoints-authored-evidence.json"),
        serde_json::to_vec_pretty(&json!({
            "provenance":provenance, "before":before, "grouped":after, "restored":restored,
            "history":history, "sample_count":TOTAL_SAMPLES,
            "complete_authored_bit_equality":true, "complete_limited_bit_equality":true,
            "complete_limiter_gain_bit_equality":true, "root_sound_recipe_byte_equality":true,
            "listening_claim":false, "independent_DSP_claim":false,
        }))?,
    )?;
    let mut fixture = project.finish(
        vec!["Editable group slices with owned attachments"],
        (0..121).map(|frame| (frame, original(if frame == 0 { 0 } else { frame - 1 }))).collect(),
        vec!["Full qualified cfr-bframes Original; one silent frozen frame at zero; finite Original audio [40000,56016) at +37 samples with distinct event and owner EQ; independent root sound. Shared CLI semantic group [5,11), durable Undo/Redo, reopened history and portable owned media preserve all canonical authored/limited PCM and limiter gain bits. Every one of 121 pictures has a literal Original identity. This is not listening or independent DSP qualification.".into()],
    )?;
    let portable = directory.join("sound-group-endpoints-portable.deadpan");
    let copied = success(&[
        "project",
        "copy-portable",
        fixture.package.to_str().ok_or("UTF-8 package")?,
        portable.to_str().ok_or("UTF-8 portable")?,
    ])?;
    assert_eq!(
        copied["portable_copy"]["revision_id"],
        fixture.revision.as_str()
    );
    assert_eq!(copied["portable_copy"]["originals"][0]["from"], "managed");
    let store = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &restored)?;
    assert_eq!(serde_json::to_vec(store.snapshot()?.sounds())?, old_root);
    store.validate_report(true)?;
    drop(store);
    same_pcm(
        &oracle,
        &read_canonical(&portable, &[37, 251, 173])?,
        "portable reopen",
    );
    cold_seams(&portable, &oracle, "portable reopen")?;
    fs::write(
        directory.join("sound-group-endpoints-portable-copy.json"),
        serde_json::to_vec_pretty(&copied)?,
    )?;
    fixture.package = portable;
    Ok(fixture)
}
