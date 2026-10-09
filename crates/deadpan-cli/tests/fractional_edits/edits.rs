use std::path::Path;

use deadpan_core::{
    AudioBoundaryKind, AudioEdgePolicy, AudioEditorialEdges, AudioSample, AudioTimingId, Command,
    CommandRequest, ExactRatio, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, RetimePurpose, RevisionId, SourceTimeBase, SplitIdentities,
};
use deadpan_plan::{
    AudioContent, AudioQueryLimits, AudioSignalContent, Picture, RenderPlan, SilenceReason,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::{
    CUT_FRAME, CUT_SAMPLE, EDITS, SOURCE_FRAMES, boundary,
    fixture::Fixture,
    flattened_suffix_source_fifths,
    support::{Result, Run, WIRE_LIMIT},
};

pub struct Edited {
    pub revision: RevisionId,
    pub checkpoints: Vec<(i64, RevisionId)>,
    pub setup_commands: i64,
    pub history_before: [i64; 2],
    pub history_after: [i64; 2],
    pub maximum_document_bytes: usize,
    pub maximum_edit_bytes: usize,
}

pub fn apply(run: &Run, source: &Fixture) -> Result<Edited> {
    let mut store = ProjectStore::open(&source.package, AccessMode::ReadWrite)?;
    let initial = store.snapshot()?;
    assert_eq!(initial.revision_id(), &source.initial_revision);
    assert_eq!(
        RenderPlan::compile(&initial)?.duration().frames(),
        SOURCE_FRAMES
    );
    let mut result = Edited {
        revision: initial.revision_id().clone(),
        checkpoints: Vec::new(),
        setup_commands: 0,
        history_before: history_counts(&source.package)?,
        history_after: [0; 2],
        maximum_document_bytes: 0,
        maximum_edit_bytes: 0,
    };
    let pause = NodeId::new("fractional-pause")?;
    let first = revision(1)?;
    commit(
        &mut store,
        first.clone(),
        Command::InsertTime {
            at: ProjectFrame(CUT_FRAME),
            id: pause.clone(),
            hold: HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            identities: SplitIdentities {
                nodes: (0..6)
                    .map(|i| NodeId::new(format!("fractional-split-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: first,
                ordinal: 0,
            },
        },
        1,
        &mut result,
    )?;
    check_plan(&store.snapshot()?, source, 1)?;

    // Explicit Hard edges keep sample-by-sample timing checks independent of
    // automatic creative fades. Partition policies first need authored edges.
    let nodes = store.snapshot()?.nodes().clone();
    for (id, node) in &nodes {
        if matches!(
            node.kind,
            NodeKind::Retime {
                purpose: RetimePurpose::Partition,
                ..
            }
        ) {
            setup(
                &mut store,
                Command::SetEditorialEdges {
                    node: id.clone(),
                    edges: AudioEditorialEdges {
                        start: true,
                        end: true,
                    },
                },
                &mut result,
            )?;
        }
        for edge in [AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd] {
            setup(
                &mut store,
                Command::SetAudioEdge {
                    node: id.clone(),
                    edge,
                    policy: AudioEdgePolicy::Hard,
                },
                &mut result,
            )?;
        }
        if matches!(node.kind, NodeKind::Source { .. }) {
            for edge in [
                AudioBoundaryKind::SourcePlacementStart,
                AudioBoundaryKind::SourcePlacementEnd,
            ] {
                setup(
                    &mut store,
                    Command::SetAudioEdge {
                        node: id.clone(),
                        edge,
                        policy: AudioEdgePolicy::Hard,
                    },
                    &mut result,
                )?;
            }
        }
    }
    let baseline = store.snapshot()?;
    assert_eq!(
        baseline.nodes().len(),
        6,
        "fixed transparent Source partitions plus Hold"
    );
    let bindings = baseline.audio_bindings().clone();
    result.checkpoints.push((1, baseline.revision_id().clone()));
    super::audio::check_early_checkpoint(run, source, 1, baseline.revision_id())?;
    for count in 2..=EDITS {
        run.check("one-frame edit loop")?;
        let before = store.snapshot_shared()?;
        let NodeKind::Hold { recipe } = &before.nodes()[&pause].kind else {
            panic!("pause changed kind")
        };
        assert_eq!(
            recipe.duration.frames(),
            count - 1,
            "edit {count} must increase duration by one"
        );
        drop(before);
        commit(
            &mut store,
            revision(count)?,
            Command::SetHoldDuration {
                node: pause.clone(),
                duration: FrameDuration::new(count)?,
            },
            1,
            &mut result,
        )?;
        let document = store.snapshot_shared()?;
        assert_eq!(
            document.nodes().len(),
            6,
            "edit {count} grew the active structure"
        );
        assert_eq!(
            document.audio_bindings(),
            &bindings,
            "edit {count} changed retained sampling contexts/journals"
        );
        for (id, original) in baseline.nodes() {
            if matches!(original.kind, NodeKind::Source { .. }) {
                assert_eq!(
                    &document.nodes()[id],
                    original,
                    "edit {count} changed Original context {id}"
                );
            }
        }
        check_plan(&document, source, count)?;
        if count <= 5 {
            super::audio::check_early_checkpoint(run, source, count, document.revision_id())?;
        }
        if count <= 5 || count % 1000 == 0 || count >= 9996 {
            result
                .checkpoints
                .push((count, document.revision_id().clone()));
        }
        if count % 100 == 0 {
            run.check_storage()?;
        }
        if count % 1000 == 0 {
            eprintln!("fractional edits: {count}/10000 committed and independently checked");
        }
    }
    result.revision = store.head_revision()?;
    drop(store);
    result.history_after = history_counts(&source.package)?;
    for column in 0..2 {
        assert_eq!(
            result.history_after[column] - result.history_before[column],
            EDITS + result.setup_commands,
            "every timing edit and separately counted edge setup must be durable"
        );
    }
    let reopened = ProjectStore::open(&source.package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.head_revision()?, result.revision);
    check_plan(&reopened.snapshot()?, source, EDITS)?;
    run.check_storage()?;
    Ok(result)
}

fn revision(count: i64) -> Result<RevisionId> {
    Ok(RevisionId::new(format!("fractional-edit-{count:05}"))?)
}

fn setup(store: &mut ProjectStore, command: Command, stats: &mut Edited) -> Result {
    stats.setup_commands += 1;
    commit(
        store,
        RevisionId::new(format!("fractional-edge-{:02}", stats.setup_commands))?,
        command,
        0,
        stats,
    )
}

fn commit(
    store: &mut ProjectStore,
    next: RevisionId,
    command: Command,
    delta: i64,
    stats: &mut Edited,
) -> Result {
    let before = store.snapshot_shared()?;
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: next.clone(),
        command,
    };
    let request_bytes = serde_json::to_vec(&request)?.len();
    assert!(
        request_bytes < WIRE_LIMIT,
        "oversized fractional edit request"
    );
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.revision_id, next);
    assert_eq!(outcome.edit.duration_delta, delta);
    let edit_bytes = request_bytes + serde_json::to_vec(&outcome.edit)?.len();
    assert!(
        edit_bytes <= WIRE_LIMIT,
        "fractional request+edit grew to {edit_bytes} bytes"
    );
    stats.maximum_edit_bytes = stats.maximum_edit_bytes.max(edit_bytes);
    let document_bytes = store.snapshot_shared()?.to_compact_json()?.len();
    assert!(
        document_bytes <= WIRE_LIMIT,
        "fractional document grew to {document_bytes} bytes"
    );
    stats.maximum_document_bytes = stats.maximum_document_bytes.max(document_bytes);
    Ok(())
}

fn history_counts(package: &Path) -> Result<[i64; 2]> {
    let connection = rusqlite::Connection::open_with_flags(
        package.join("project.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    Ok(connection.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
        [],
        |row| Ok([row.get(0)?, row.get(1)?]),
    )?)
}

fn check_plan(document: &ProjectDocument, source: &Fixture, inserted: i64) -> Result {
    let plan = RenderPlan::compile(document)?;
    let resume = boundary(CUT_FRAME + inserted);
    let total = boundary(SOURCE_FRAMES + inserted);
    assert_eq!(
        plan.duration().frames(),
        SOURCE_FRAMES + inserted,
        "edit {inserted}"
    );
    assert_eq!(
        plan.audio_duration()?.0,
        total,
        "edit {inserted} total samples"
    );
    let audio = plan.audio(
        AudioSample(0)..AudioSample(total),
        AudioQueryLimits {
            maximum_spans: 8,
            maximum_work: 256,
        },
    )?;
    assert_eq!(
        audio.spans.len(),
        3,
        "edit {inserted}: prefix, silence, suffix: {:?}",
        audio.spans
    );
    for (span, (start, end)) in
        audio
            .spans
            .iter()
            .zip([(0, CUT_SAMPLE), (CUT_SAMPLE, resume), (resume, total)])
    {
        assert_eq!(
            span.samples,
            AudioSample(start)..AudioSample(end),
            "edit {inserted}"
        );
        assert_eq!(
            span.allocated_samples,
            AudioSample(start)..AudioSample(end),
            "edit {inserted} allocation"
        );
    }
    assert!(matches!(
        audio.spans[0].content,
        AudioContent::Source { .. }
    ));
    assert!(matches!(
        audio.spans[1].content,
        AudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    ));
    assert!(matches!(
        audio.spans[2].content,
        AudioContent::Source { .. }
    ));
    for sample in [resume, resume + 17, total - 1] {
        let point = audio.spans[2].source_point(AudioSample(sample))?;
        assert_eq!(point.time_base, SourceTimeBase::new(1, 48_000)?);
        assert_eq!(
            point.ticks,
            ExactRatio::new(flattened_suffix_source_fifths(inserted, sample), 5)?,
            "edit {inserted}, flattened structural source phase at output {sample}"
        );
    }
    // Canonical StageAudio uses this retained binding, not the flattened source
    // span above. InsertTime resumes old B(50)=80080 at new B(50+i); changing
    // the Hold duration retains that phase and the complete original lattice.
    let processing = plan.audio_processing(
        AudioSample(resume)..AudioSample(total),
        AudioQueryLimits {
            maximum_spans: 8,
            maximum_work: 256,
        },
    )?;
    assert_eq!(processing.spans.len(), 1, "edit {inserted}: bound suffix");
    let span = &processing.spans[0];
    let AudioSignalContent::Bound(bound) = &span.content else {
        panic!("edit {inserted}: suffix lost its retained audio binding");
    };
    assert_eq!(bound.reference_samples_per_output_sample(), ExactRatio::ONE);
    for sample in [resume, resume + 17, total - 1] {
        assert_eq!(
            bound.reference_at_offset(sample - span.allocated_samples.start.0)?,
            ExactRatio::integer(CUT_SAMPLE + sample - resume),
            "edit {inserted}, retained PCM reference at output {sample}"
        );
    }
    for frame in [0, 49, 50, 49 + inserted, 50 + inserted, 99 + inserted] {
        check_picture(&plan, source, inserted, frame)?;
    }
    Ok(())
}

pub fn check_picture(plan: &RenderPlan, source: &Fixture, inserted: i64, frame: i64) -> Result {
    let picture = plan.picture(ProjectFrame(frame))?.picture;
    if (CUT_FRAME..CUT_FRAME + inserted).contains(&frame) {
        assert!(
            matches!(picture, Picture::Background),
            "edit {inserted} frame {frame}: {picture:?}"
        );
    } else {
        let expected = if frame < CUT_FRAME {
            frame
        } else {
            frame - inserted
        };
        let Picture::Source { asset, point, .. } = picture else {
            panic!("expected Original at edit {inserted} frame {frame}: {picture:?}")
        };
        assert_eq!(asset, source.asset);
        assert_eq!(point.time_base, SourceTimeBase::new(1, 30_000)?);
        assert_eq!(
            point
                .ticks
                .numerator()
                .div_euclid(point.ticks.denominator() * 1001),
            i128::from(expected),
            "edit {inserted} frame {frame}"
        );
    }
    Ok(())
}
