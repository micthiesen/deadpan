//! Real qualified Original, new processing group and retained ancestor clocks.
use super::*;
use deadpan_core::{BeatNode, MoveRangeDestination, Saturation, SliceCaptureSelection, Subtree};

pub const FINAL_SAMPLES: i64 = 30_430;
pub const FINAL_FRAMES: u64 = 19;

fn retain(directory: &Path, name: &str, value: &impl serde::Serialize) -> Result {
    fs::write(directory.join(name), serde_json::to_vec(value)?)?;
    Ok(())
}

fn read_exact(path: &Path, partitions: &[u32]) -> Result<CanonicalPcm> {
    assert!(!partitions.is_empty());
    assert!(partitions.iter().all(|count| (1..=256).contains(count)));
    let mut session = ProjectAudioSession::open(path)?;
    let total = session.plan().audio_duration()?.0;
    let cancelled = AtomicBool::new(false);
    let mut result = CanonicalPcm {
        authored_bits: Vec::with_capacity(usize::try_from(total)?),
        limited_bits: Vec::with_capacity(usize::try_from(total)?),
        limiter_gain_bits: Vec::with_capacity(usize::try_from(total)?),
    };
    let mut at = 0;
    let mut partition = 0;
    while at < total {
        let count = u32::try_from((total - at).min(i64::from(partitions[partition])))?;
        let authored = session.read_authored_bus(AudioSample(at), count, &cancelled)?;
        let limited = session.read_limited(AudioSample(at), count, &cancelled)?;
        assert_eq!(authored.samples.len(), usize::try_from(count)?);
        assert_eq!(limited.samples.len(), usize::try_from(count)?);
        assert_eq!(limited.gain.len(), usize::try_from(count)?);
        assert_eq!(authored.stage, "authored_bus_pcm_before_mastering");
        assert!(
            authored
                .samples
                .iter()
                .chain(&limited.samples)
                .flatten()
                .all(|sample| sample.is_finite())
        );
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

fn same_exact(expected: &CanonicalPcm, actual: &CanonicalPcm, context: &str) {
    for (stage, expected, actual) in [
        ("authored", &expected.authored_bits, &actual.authored_bits),
        ("limited", &expected.limited_bits, &actual.limited_bits),
    ] {
        assert_eq!(actual.len(), expected.len(), "{context}: {stage} length");
        for (sample, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "{context}: {stage} sample {sample}");
        }
    }
    assert_eq!(
        actual.limiter_gain_bits.len(),
        expected.limiter_gain_bits.len()
    );
    for (sample, (actual, expected)) in actual
        .limiter_gain_bits
        .iter()
        .zip(&expected.limiter_gain_bits)
        .enumerate()
    {
        assert_eq!(actual, expected, "{context}: limiter gain {sample}");
    }
}

fn cold_exact(path: &Path, expected: &CanonicalPcm, boundaries: &[i64], context: &str) -> Result {
    let total = i64::try_from(expected.authored_bits.len())?;
    for boundary in boundaries {
        let first = (*boundary - 2).max(0);
        let count = u32::try_from((total - first).min(5))?;
        let mut session = ProjectAudioSession::open(path)?;
        let cancelled = AtomicBool::new(false);
        let authored = session.read_authored_bus(AudioSample(first), count, &cancelled)?;
        let limited = session.read_limited(AudioSample(first), count, &cancelled)?;
        let begin = usize::try_from(first)?;
        let end = begin + usize::try_from(count)?;
        assert_eq!(
            authored
                .samples
                .into_iter()
                .map(|sample| sample.map(f32::to_bits))
                .collect::<Vec<_>>(),
            expected.authored_bits[begin..end],
            "{context}: authored cold read {first}"
        );
        assert_eq!(
            limited
                .samples
                .into_iter()
                .map(|sample| sample.map(f32::to_bits))
                .collect::<Vec<_>>(),
            expected.limited_bits[begin..end],
            "{context}: limited cold read {first}"
        );
        assert_eq!(
            limited
                .gain
                .into_iter()
                .map(f64::to_bits)
                .collect::<Vec<_>>(),
            expected.limiter_gain_bits[begin..end],
            "{context}: gain cold read {first}"
        );
    }
    Ok(())
}

fn whole_scope(
    source: &ProjectDocument,
    owner: &NodeId,
    scope: &NodeId,
    sound: &SoundId,
) -> Result {
    let journal = &source.audio_bindings().sound_clocks()[owner][sound];
    assert_eq!(journal.scope(), scope);
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(journal.clocks()[0].scope(), scope);
    Ok(())
}

fn durable_roundtrip(
    project: &Project,
    before: &ProjectDocument,
    after: &ProjectDocument,
    before_pcm: &CanonicalPcm,
    after_pcm: &CanonicalPcm,
    label: &str,
) -> Result<ProjectDocument> {
    let bank = ProjectStore::open(&project.package, AccessMode::ReadOnly)?.registers()?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = project.document()?;
    same_authored(&undone, before)?;
    same_exact(
        before_pcm,
        &read_exact(&project.package, &[251, 173, 37])?,
        &format!("durable undo {label}"),
    );
    assert_eq!(
        ProjectStore::open(&project.package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = project.document()?;
    same_authored(&restored, after)?;
    same_exact(
        after_pcm,
        &read_exact(&project.package, &[173, 37, 251])?,
        &format!("durable redo {label}"),
    );
    assert_eq!(
        ProjectStore::open(&project.package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    Ok(restored)
}

pub fn build(directory: &Path) -> Result<Fixture> {
    let mut project = Project::create(directory, "aligned-sound-scope")?;
    // Obtain this four-frame Source through the exact public Original moment
    // path. The measured source receipt and retained bytes remain authoritative.
    project.yank_original('o', 27, 31)?;
    project.delete_range(0, 120)?;
    project.apply_semantic(
        json!([{"type":"paste","register":"o","before":false}]),
        0,
        None,
    )?;
    let moment = project.document()?;
    assert_eq!(moment.duration()?.frames(), 4);
    let owner = root_children(&moment)?[0].clone();
    assert!(matches!(
        moment.nodes()[&owner].kind,
        NodeKind::Source { .. }
    ));
    for (index, id, count, label) in [
        (0, "aligned-lead", 4, "Four-frame lead"),
        (2, "aligned-tail", 2, "Two-frame tail"),
    ] {
        project.apply(|_, document, _| {
            Ok(Command::Insert {
                parent: document.root().clone(),
                index,
                subtree: Subtree {
                    root: node(id)?,
                    nodes: std::collections::BTreeMap::from([(
                        node(id)?,
                        BeatNode::hold(label, silent(count, HoldVideo::Background)?),
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            })
        })?;
    }
    let top = node("aligned-top")?;
    project.apply(|_, document, _| {
        Ok(Command::Group {
            parent: document.root().clone(),
            start: 1,
            end: 3,
            id: top.clone(),
            label: "Retained ancestor Top".into(),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: top.clone(),
            treatments: equalizer([3000, -1500, 1000])?,
        })
    })?;
    let sound = SoundId::new("aligned-attached")?;
    let tb = deadpan_core::SourceTimeBase::new(1, 48_000)?;
    let event_source = SourceAudio {
        asset: project.asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 48_000,
                time_base: tb,
            },
            SourceTimestamp {
                ticks: 52_096,
                time_base: tb,
            },
        )?,
    };
    project.apply(|_, document, _| {
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: BeatSound {
                label: "Independent finite qualified Original event".into(),
                source: event_source.clone(),
                mapping: SourceAudioMapping::natural_rate(
                    event_source.span,
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
    project.apply(|_, _, revision| {
        Ok(Command::InsertTime {
            at: ProjectFrame(0),
            hold: silent(5, HoldVideo::Background)?,
            id: node("aligned-prefix")?,
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let before = project.document()?;
    assert_eq!(before.duration()?.frames(), 15);
    assert_eq!(boundary(15)?, 24_024);
    whole_scope(&before, &owner, &top, &sound)?;
    let prior = before.audio_bindings().sound_clocks()[&owner][&sound].clone();
    let selection = SliceCaptureSelection::Child {
        node: owner.clone(),
    };
    assert_eq!(
        before.group_selection(&top, &selection)?.required_split_ids,
        0
    );
    let baseline = read_exact(&project.package, &[256])?;
    assert_eq!(baseline.authored_bits.len(), 24_024);
    let owner_start = usize::try_from(boundary(9)?)?;
    let owner_end = usize::try_from(boundary(13)?)?;
    assert!(
        baseline.authored_bits[owner_start..owner_end]
            .iter()
            .flatten()
            .any(|sample| f32::from_bits(*sample).abs() > 0.001)
    );
    // The Source itself is audible, so a mixed-bus nonzero check cannot prove
    // that the independent BeatSound participates. Remove that exact event
    // through the public CLI, compare its admitted interval, then undo it.
    project.apply(|_, _, _| {
        Ok(Command::DeleteBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
        })
    })?;
    let event_off_document = project.document()?;
    assert!(event_off_document.beat_sounds().is_empty());
    let event_off = read_exact(&project.package, &[173, 251, 37])?;
    assert_eq!(event_off.authored_bits.len(), baseline.authored_bits.len());
    let event_start = owner_start.checked_add(37).ok_or("event start overflow")?;
    let event_end = event_start.checked_add(4096).ok_or("event end overflow")?;
    assert!(event_end <= owner_end);
    let changed_event_samples = baseline.authored_bits[event_start..event_end]
        .iter()
        .zip(&event_off.authored_bits[event_start..event_end])
        .filter(|(with, without)| with != without)
        .count();
    assert!(
        changed_event_samples > 0,
        "the independent event must affect its admitted finite interval"
    );
    let changed_bus_samples = baseline
        .authored_bits
        .iter()
        .zip(&event_off.authored_bits)
        .filter(|(with, without)| with != without)
        .count();
    retain(directory, "aligned-event-off-pcm.json", &event_off)?;
    retain(
        directory,
        "aligned-event-off-document.json",
        &event_off_document,
    )?;
    retain(
        directory,
        "aligned-event-control.json",
        &json!({
            "owner":owner,"event":sound,"event_source_samples":[48000,52096],
            "checked_project_samples":[event_start,event_end],
            "changed_event_samples":changed_event_samples,"changed_bus_samples":changed_bus_samples,
            "offset_samples":37,"finite_event_samples":4096,
            "outside_interval_silence_claim":false,
        }),
    )?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        event_off_document.revision_id().as_str(),
    ])?;
    let restored_event = project.document()?;
    same_authored(&restored_event, &before)?;
    whole_scope(&restored_event, &owner, &top, &sound)?;
    same_exact(
        &baseline,
        &read_exact(&project.package, &[37, 173, 251])?,
        "durable event-off control undo",
    );
    let before = restored_event;
    retain(directory, "aligned-before-pcm.json", &baseline)?;
    retain(directory, "aligned-before-document.json", &before)?;
    let output = project.macro_operation(
        json!({"type":"apply","parent":top,"cursor":14,
        "selected_child":owner,"program":{"instructions":[
            {"type":"group","selector":{"type":"selected_beat"},"label":"New processing G"}
        ]}}),
        false,
    )?;
    fs::write(
        directory.join("aligned-group-macro.stdout.json"),
        &output.stdout,
    )?;
    fs::write(
        directory.join("aligned-group-macro.stderr.txt"),
        &output.stderr,
    )?;
    assert!(
        output.status.success(),
        "aligned native-equivalent group: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        trace["trace"][0]["resolved_range"],
        json!({"start":9,"end":13})
    );
    let group: NodeId = serde_json::from_value(trace["context"]["selected_child"].clone())?;
    let grouped = project.document()?;
    whole_scope(&grouped, &owner, &top, &sound)?;
    let journal = &grouped.audio_bindings().sound_clocks()[&owner][&sound];
    assert_eq!(journal.clocks()[0].timing(), prior.clocks()[0].timing());
    assert!(journal.clocks()[0].introduced_groups().contains(&group));
    assert_eq!(
        grouped.audio_bindings().timings(),
        before.audio_bindings().timings()
    );
    same_exact(
        &baseline,
        &read_exact(&project.package, &[251, 37, 173])?,
        "neutral aligned GroupSelection",
    );
    cold_exact(
        &project.package,
        &baseline,
        &[boundary(9)?, boundary(11)?, boundary(13)?, boundary(15)?],
        "neutral aligned group",
    )?;
    durable_roundtrip(
        &project,
        &before,
        &grouped,
        &baseline,
        &baseline,
        "aligned group",
    )?;
    let eq = equalizer([4500, -2500, 1500])?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: group.clone(),
            treatments: eq.clone(),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: group.clone(),
            treatments: eq.with_saturation(Some(Saturation::new(GainDb::new(6000)?)?))?,
        })
    })?;
    let treated_document = project.document()?;
    let treated = read_exact(&project.package, &[256])?;
    assert!(
        treated.authored_bits[owner_start..owner_end]
            .iter()
            .zip(&baseline.authored_bits[owner_start..owner_end])
            .any(|(actual, before)| actual != before),
        "new G EQ/saturation must actually affect real PCM"
    );
    same_exact(
        &treated,
        &read_exact(&project.package, &[173, 37, 251])?,
        "treated G partition invariance",
    );
    retain(directory, "aligned-treated-pcm.json", &treated)?;
    retain(
        directory,
        "aligned-treated-document.json",
        &treated_document,
    )?;
    project.apply(|project, _, _| {
        Ok(Command::Split {
            node: owner.clone(),
            at: FrameDuration::new(2)?,
            identities: SplitIdentities {
                nodes: project.fresh(16)?,
            },
        })
    })?;
    let split = project.document()?;
    assert_eq!(
        split.beat_sounds().len(),
        2,
        "both complementary Source fragments retain the independent sound"
    );
    assert_eq!(split.duration()?.frames(), 15);
    for events in split.audio_bindings().sound_clocks().values() {
        let journal = events.get(&sound).ok_or("split event clock")?;
        assert!(
            journal
                .clocks()
                .iter()
                .any(|reference| reference.introduced_groups().contains(&group)),
            "the new G birth certificate survives the Source Split"
        );
        assert!(
            journal
                .clocks()
                .iter()
                .any(|reference| !reference.introduced_splits().is_empty()),
            "explicit Source Split retains its certified broad forest"
        );
    }
    same_exact(
        &treated,
        &read_exact(&project.package, &[37, 251, 173])?,
        "neutral Source Split after new G treatments",
    );
    cold_exact(
        &project.package,
        &treated,
        &[boundary(9)?, boundary(11)?, boundary(13)?],
        "treated G Split",
    )?;
    retain(directory, "aligned-split-document.json", &split)?;
    let split = durable_roundtrip(
        &project,
        &treated_document,
        &split,
        &treated,
        &treated,
        "treated neutral Split",
    )?;
    project.apply(|project, document, revision| {
        // Move the complete retained scope. Splitting Top's own tail while
        // moving only G changes the broad scope's arity without a plain-Split
        // certificate, so that is outside this qualified command path.
        let range = FrameRange::new(ProjectFrame(9), ProjectFrame(15))?;
        let destination = MoveRangeDestination::Interior {
            parent: document.root().clone(),
            target: node("aligned-lead")?,
            at: FrameDuration::new(3)?,
        };
        let planned = document.range_move(document.root(), range, &destination)?;
        assert_eq!(
            planned.inserted,
            FrameRange::new(ProjectFrame(8), ProjectFrame(14))?
        );
        Ok(Command::MoveRange {
            source_revision: document.revision_id().clone(),
            source_parent: document.root().clone(),
            range,
            destination,
            identities: SplitIdentities {
                nodes: project.fresh(planned.required_ids)?,
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let moved = project.document()?;
    assert_eq!(
        moved.sequence_children(&top, &group, &group)?.range,
        FrameRange::new(ProjectFrame(8), ProjectFrame(12))?
    );
    assert_eq!(
        moved.nodes()[&group].audio_treatments,
        treated_document.nodes()[&group].audio_treatments
    );
    let moved_pcm = read_exact(&project.package, &[256])?;
    same_exact(
        &moved_pcm,
        &read_exact(&project.package, &[251, 37, 173])?,
        "moved G partition invariance",
    );
    cold_exact(
        &project.package,
        &moved_pcm,
        &[
            boundary(8)?,
            boundary(10)?,
            boundary(12)?,
            boundary(14)?,
            boundary(15)?,
        ],
        "moved complete Top",
    )?;
    retain(directory, "aligned-moved-pcm.json", &moved_pcm)?;
    retain(directory, "aligned-moved-document.json", &moved)?;
    let moved = durable_roundtrip(
        &project,
        &split,
        &moved,
        &treated,
        &moved_pcm,
        "whole Top move",
    )?;
    let trailing_lead = root_children(&moved)?
        .last()
        .ok_or("trailing lead after Top move")?
        .clone();
    assert_eq!(
        moved.sequence_children(moved.root(), &top, &top)?.range,
        FrameRange::new(ProjectFrame(8), ProjectFrame(14))?
    );
    assert_eq!(
        moved
            .sequence_children(moved.root(), &trailing_lead, &trailing_lead)?
            .range,
        FrameRange::new(ProjectFrame(14), ProjectFrame(15))?
    );
    let copied = project.macro_operation(
        json!({"type":"apply","parent":moved.root(),"cursor":15,
        "selected_child":trailing_lead,"program":{"instructions":[
            {"type":"yank","selector":{"type":"node","node":group},"register":"c"},
            {"type":"paste","register":"c","before":false}
        ]}}),
        false,
    )?;
    fs::write(
        directory.join("aligned-copy-macro.stdout.json"),
        &copied.stdout,
    )?;
    fs::write(
        directory.join("aligned-copy-macro.stderr.txt"),
        &copied.stderr,
    )?;
    assert!(
        copied.status.success(),
        "copy complete new G: {}",
        String::from_utf8_lossy(&copied.stderr)
    );
    let final_document = project.document()?;
    assert_eq!(final_document.duration()?.frames(), FINAL_FRAMES as i64);
    assert_eq!(
        final_document.beat_sounds().len(),
        4,
        "original and copied G each retain the two physical event owners"
    );
    assert_eq!(boundary(FINAL_FRAMES as i64)?, FINAL_SAMPLES);
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new('c')?].as_ref() else {
        return Err("complete G copy was not retained as an editable capture".into());
    };
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Child {
            node: group.clone()
        }
    );
    assert_eq!(slice.parent(), &top);
    assert_eq!(
        slice.range(),
        FrameRange::new(ProjectFrame(8), ProjectFrame(12))?
    );
    slice.validate_capture(&store.capture_snapshot_at(slice.revision_id())?)?;
    assert_eq!(slice.revision_id(), moved.revision_id());
    assert!(
        slice.identity_requirements()?.timings > 0,
        "the copy retains historical sound clocks"
    );
    let history = store.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    drop(store);
    let final_pcm = read_exact(&project.package, &[256])?;
    assert_eq!(
        final_pcm.authored_bits.len(),
        usize::try_from(FINAL_SAMPLES)?
    );
    same_exact(
        &final_pcm,
        &read_exact(&project.package, &[37, 173, 251])?,
        "copied G partition invariance",
    );
    cold_exact(
        &project.package,
        &final_pcm,
        &[
            boundary(8)?,
            boundary(10)?,
            boundary(12)?,
            boundary(14)?,
            boundary(15)?,
            boundary(17)?,
            FINAL_SAMPLES,
        ],
        "complete G copy",
    )?;
    retain(directory, "aligned-final-pcm.json", &final_pcm)?;
    retain(directory, "aligned-final-document.json", &final_document)?;
    let restored = durable_roundtrip(
        &project,
        &moved,
        &final_document,
        &moved_pcm,
        &final_pcm,
        "whole G copy",
    )?;
    let portable = directory.join("aligned-sound-scope-portable.deadpan");
    let copy = success(&[
        "project",
        "copy-portable",
        project.path(),
        portable.to_str().ok_or("UTF-8 package")?,
    ])?;
    let store = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &restored)?;
    assert_eq!(store.registers()?, bank);
    let restored_history = store.validate_report(true)?;
    assert_eq!(restored_history.verified_by_receipt, 0);
    assert_eq!(restored_history.replayed + 1, restored_history.revisions);
    drop(store);
    same_exact(
        &final_pcm,
        &read_exact(&portable, &[37, 251, 173])?,
        "portable reopened complete PCM",
    );
    cold_exact(
        &portable,
        &final_pcm,
        &[
            boundary(8)?,
            boundary(10)?,
            boundary(12)?,
            boundary(14)?,
            boundary(15)?,
            boundary(17)?,
            FINAL_SAMPLES,
        ],
        "portable reopened",
    )?;
    retain(directory, "aligned-portable-copy.json", &copy)?;
    retain(
        directory,
        "aligned-history-evidence.json",
        &json!({
            "owner":owner,"ancestor":top,"new_group":group,"history_before_navigation":history,
            "history_after_portable_reopen":restored_history,"final_revision":restored.revision_id(),
            "neutral_group_complete_bit_equality":true,"treated_source_split_complete_bit_equality":true,
            "moved_and_copied_partition_and_cold_read_bit_equality":true,
            "source_ordinals":[27,28,29,30],"event_source_samples":[48000,52096],"event_offset_samples":37,
            "before_frames":15,"final_frames":FINAL_FRAMES,"final_samples":FINAL_SAMPLES,
            "independent_DSP_claim":false,"listening_claim":false,
            "move_PCM_translation_claim":false,
            "move_before":[9,15],"move_after":[8,14],"new_group_after_move":[8,12],
            "event_off_control_project_samples":[event_start,event_end],
            "event_off_control_changed_samples":changed_event_samples,
            "source_review_correction":"Move the complete Top scope; the earlier unexecuted draft moved G into Top's tail and would change the retained scope arity without a plain-Split certificate",
        }),
    )?;
    let mut pictures = vec![Expected::Background; 8];
    pictures.extend((27..31).map(original));
    pictures.extend([Expected::Background; 3]);
    pictures.extend((27..31).map(original));
    assert_eq!(pictures.len(), usize::try_from(FINAL_FRAMES)?);
    let mut fixture = project.finish(vec!["Aligned sound-owner grouping in retained ancestor scope"],
        pictures.into_iter().enumerate().map(|(frame,picture)|(frame as u64,picture)).collect(),
        vec!["Actual qualified cfr-bframes Original moment [27,31), finite independent Original audio [48000,52096) at +37 samples, event EQ and ancestor Top EQ. A public event-off control proves that the finite BeatSound affects actual PCM, and durable Undo restores every authored field and journal. A five-frame root prefix creates a broad retained Top journal. Native-equivalent semantic GroupSelectedBeat wraps the complete Source as new G without Split identities, preserving every authored/limited PCM and limiter-gain bit. New G EQ and saturation affect actual PCM; a subsequent neutral Source Split preserves every bit. The complete Top moves from Root [9,15) into the lead at global frame 8, retaining G at [8,12); G's editable copy appends at Root frame 15. Cold/partition reads, durable Undo/Redo, reopened history, portable owned media, literal picture provenance and complete emitted-file comparison are checked. The unselected Top treatment is not part of the copied G. This is not an independent DSP or naive PCM-translation oracle and makes no listening claim.".into()])?;
    fixture.package = portable;
    Ok(fixture)
}
