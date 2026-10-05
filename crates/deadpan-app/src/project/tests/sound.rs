//! Real measured catalog sources through the native writer request path.

use deadpan_core::{
    AudioEdgePolicy, AudioSample, ExactFrameRange, ExactRatio, SoundId, SourceAudioMapping,
};

use super::*;

mod allowances;

pub(super) fn catalog(harness: &Harness) -> Arc<Workspace> {
    let update = command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    harness.finish(harness.job());
    harness.finish(harness.job());
    let ready = complete(&harness.service);
    let update = command(
        &harness.service,
        ProjectRequest::ImportSound {
            expected_session: ready.session,
            expected_revision: ready.document.revision_id().clone(),
            path: fixture("../audio-fixtures/pcm-stereo-48000.wav"),
            stream: None,
            interpretation: Some(AudioLayoutInterpretation::StereoLeftRight),
            ownership: OriginalOwnership::Managed,
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    harness.finish(harness.job());
    harness.finish(harness.job());
    complete(&harness.service)
}

pub(super) fn catalog_asset(workspace: &Workspace) -> AssetId {
    workspace
        .sources
        .values()
        .find(|source| source.sound_audition.is_some())
        .unwrap()
        .asset
        .clone()
}

pub(super) fn request(workspace: &Workspace, edit: ProjectSoundEdit) -> ProjectRequest {
    ProjectRequest::SoundEdit {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        edit,
    }
}

fn changed(
    service: &ProjectService,
    workspace: &Workspace,
    edit: ProjectSoundEdit,
) -> (Arc<Workspace>, Option<SoundId>) {
    let update = command(service, request(workspace, edit));
    assert!(update.error.is_none(), "{:?}", update.error);
    let committed = update.committed.unwrap();
    assert!(committed.preserve_cursor);
    assert!(committed.cursor.is_none());
    assert!(committed.selected_node.is_none());
    assert_eq!(committed.scope, SequenceScope::default());
    let after = update.workspace.unwrap();
    assert_eq!(&committed.revision, after.document.revision_id());
    (after, committed.sound.unwrap().selected)
}

fn same_picture(before: &Workspace, after: &Workspace) {
    assert_eq!(
        after.document.duration().unwrap(),
        before.document.duration().unwrap()
    );
    assert_eq!(after.document.nodes(), before.document.nodes());
    assert_eq!(
        after.document.audio_bindings(),
        before.document.audio_bindings()
    );
    assert_eq!(after.single_source, before.single_source);
    assert_eq!(after.document.assets(), before.document.assets());
    for frame in 0..before.plan.duration().frames() {
        assert_eq!(
            after.plan.picture(ProjectFrame(frame)).unwrap().picture,
            before.plan.picture(ProjectFrame(frame)).unwrap().picture
        );
    }
}

fn assert_failed(
    service: &ProjectService,
    workspace: &Workspace,
    edit: ProjectSoundEdit,
    contains: &str,
) {
    let update = command(service, request(workspace, edit));
    let error = update.error.unwrap();
    assert!(error.contains(contains), "{error}");
    assert!(update.committed.is_none());
    assert_eq!(*update.workspace.unwrap().document, *workspace.document);
}

#[test]
fn arbitrary_sample_sound_parameters_move_and_removal_survive_durable_history() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let asset = catalog_asset(&before);
    let (placed, selected) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: asset.clone(),
            at: AudioSample(137),
        },
    );
    let id = selected.unwrap();
    let original = placed.document.sounds()[&id].clone();
    assert_eq!(original.owner, *before.document.root());
    assert_eq!(
        original.source.span,
        before.document.assets()[&asset].audio.unwrap()
    );
    assert_eq!(original.offset, AudioSample(137));
    assert_eq!(original.gain_millidecibels, 0);
    assert_eq!(
        original.mapping,
        SourceAudioMapping::natural_rate(
            original.source.span,
            before.document.presentation_basis().frame_rate
        )
        .unwrap()
    );
    let audible = placed.plan.root_sound(&id).unwrap().audible_samples();
    assert_eq!(audible, AudioSample(137)..AudioSample(8334));
    assert_eq!(original.start_edge, AudioEdgePolicy::Automatic);
    same_picture(&before, &placed);

    let (updated, selected) = changed(
        &harness.service,
        &placed,
        ProjectSoundEdit::Update {
            id: id.clone(),
            gain_millidecibels: -3250,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Automatic,
        },
    );
    assert_eq!(selected.as_ref(), Some(&id));
    let adjusted = updated.document.sounds()[&id].clone();
    assert_eq!(adjusted.source, original.source);
    assert_eq!(adjusted.mapping, original.mapping);
    assert_eq!(adjusted.gain_millidecibels, -3250);
    let (moved, _) = changed(
        &harness.service,
        &updated,
        ProjectSoundEdit::Move {
            id: id.clone(),
            at: AudioSample(527),
        },
    );
    assert_eq!(moved.document.sounds()[&id].offset, AudioSample(527));
    assert_eq!(
        moved.plan.root_sound(&id).unwrap().audible_samples(),
        AudioSample(527)..AudioSample(8724)
    );
    let moved_event = moved.document.sounds()[&id].clone();
    let (deleted, selected) = changed(
        &harness.service,
        &moved,
        ProjectSoundEdit::Delete { id: id.clone() },
    );
    assert!(selected.is_none());
    assert!(deleted.document.sounds().is_empty());
    for workspace in [&updated, &moved, &deleted] {
        same_picture(&before, workspace);
    }

    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()));
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let mut current = reopened.workspace.unwrap();
    assert_eq!(*current.document, *deleted.document);
    for expected in [Some(&moved_event), Some(&adjusted), Some(&original), None] {
        let update = command(
            &harness.service,
            ProjectRequest::Undo {
                expected_revision: current.document.revision_id().clone(),
            },
        );
        assert!(update.error.is_none(), "{:?}", update.error);
        current = update.workspace.unwrap();
        assert_eq!(current.document.sounds().get(&id), expected);
        same_picture(&before, &current);
    }
    for expected in [Some(&original), Some(&adjusted), Some(&moved_event), None] {
        let update = command(
            &harness.service,
            ProjectRequest::Redo {
                expected_revision: current.document.revision_id().clone(),
            },
        );
        assert!(update.error.is_none(), "{:?}", update.error);
        current = update.workspace.unwrap();
        assert_eq!(current.document.sounds().get(&id), expected);
        same_picture(&before, &current);
    }
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn sound_requests_reject_overflow_stale_context_video_sources_and_invalid_parameters() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let asset = catalog_asset(&before);
    for at in [
        AudioSample(-1),
        before.plan.audio_duration().unwrap(),
        AudioSample(i64::MAX),
    ] {
        let update = command(
            &harness.service,
            request(
                &before,
                ProjectSoundEdit::Place {
                    asset: asset.clone(),
                    at,
                },
            ),
        );
        assert!(update.error.is_some());
        assert!(update.committed.is_none());
        assert_eq!(*update.workspace.unwrap().document, *before.document);
    }
    let picture = before
        .sources
        .values()
        .find(|source| source.original_audition.is_some())
        .unwrap()
        .asset
        .clone();
    assert_failed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: picture,
            at: AudioSample(0),
        },
        "audio-only",
    );
    assert_failed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: AssetId::new("missing").unwrap(),
            at: AudioSample(0),
        },
        "no longer registered",
    );
    let (placed, selected) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: asset.clone(),
            at: AudioSample(137),
        },
    );
    let id = selected.unwrap();
    let stale = command(
        &harness.service,
        request(&before, ProjectSoundEdit::Delete { id: id.clone() }),
    );
    assert!(stale.error.unwrap().contains("Project changed"));
    assert!(stale.committed.is_none());
    assert_eq!(*stale.workspace.unwrap().document, *placed.document);
    assert_failed(
        &harness.service,
        &placed,
        ProjectSoundEdit::Update {
            id: id.clone(),
            gain_millidecibels: 24_001,
            start_edge: AudioEdgePolicy::Automatic,
            end_edge: AudioEdgePolicy::Automatic,
        },
        "gain",
    );
    assert_failed(
        &harness.service,
        &placed,
        ProjectSoundEdit::Move {
            id: id.clone(),
            at: placed.plan.audio_duration().unwrap(),
        },
        "past the end",
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    let stale = command(
        &harness.service,
        request(&placed, ProjectSoundEdit::Delete { id }),
    );
    assert!(stale.error.unwrap().contains("session changed"));
    assert!(stale.committed.is_none());
    assert_eq!(*stale.workspace.unwrap().document, *reopened.document);
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn routed_sound_parameters_preserve_journal_and_native_move_is_explicitly_rejected() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let (placed, selected) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&before),
            at: AudioSample(137),
        },
    );
    let id = selected.unwrap();
    let paused = edited(
        &harness.service,
        &placed,
        ProjectEdit::InsertTime {
            at: ProjectFrame(2),
            duration: FrameDuration::new(1).unwrap(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(paused.document.sound_routes()[&id].edits.len(), 1);
    let recipe = paused.document.sounds()[&id].clone();
    let (updated, _) = changed(
        &harness.service,
        &paused,
        ProjectSoundEdit::Update {
            id: id.clone(),
            gain_millidecibels: -1250,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
        },
    );
    assert_eq!(
        updated.document.sound_routes(),
        paused.document.sound_routes()
    );
    assert_eq!(updated.document.sounds()[&id].source, recipe.source);
    assert_eq!(updated.document.sounds()[&id].mapping, recipe.mapping);
    assert_eq!(updated.document.sounds()[&id].offset, recipe.offset);
    same_picture(&paused, &updated);
    assert_failed(
        &harness.service,
        &updated,
        ProjectSoundEdit::Move {
            id: id.clone(),
            at: AudioSample(500),
        },
        "retained edit cuts",
    );
    assert_failed(
        &harness.service,
        &updated,
        ProjectSoundEdit::Nudge {
            id: id.clone(),
            frames: 1,
        },
        "retained edit cuts",
    );
    let (removed, _) = changed(
        &harness.service,
        &updated,
        ProjectSoundEdit::Delete { id: id.clone() },
    );
    assert!(removed.document.sound_routes().is_empty());
    let restored = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: removed.document.revision_id().clone(),
        },
    );
    assert!(restored.error.is_none(), "{:?}", restored.error);
    let restored = restored.workspace.unwrap();
    assert_eq!(
        restored.document.sound_routes(),
        updated.document.sound_routes()
    );
    assert_eq!(restored.document.sounds(), updated.document.sounds());
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn placement_rechecks_revoked_stored_qualification_in_the_commit_transaction() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let asset = catalog_asset(&before);
    let id = before.sources[&asset].receipt.id().as_str();
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    let retained: (String, String, Vec<u8>) = database.query_row(
        "SELECT original_content_id, original_ref, snapshot FROM source_qualifications WHERE id=?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    // Fault injection leaves the service's immutable workspace receipt cached.
    // It must not serve as authority after the stored evidence is revoked.
    database
        .execute("DELETE FROM source_qualifications WHERE id=?1", [id])
        .unwrap();
    assert_failed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: asset.clone(),
            at: AudioSample(137),
        },
        "qualification is missing",
    );
    database.execute("INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
        rusqlite::params![id, retained.0, retained.1, retained.2]).unwrap();
    let (placed, _) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset,
            at: AudioSample(137),
        },
    );
    same_picture(&before, &placed);
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn moving_an_unrouted_selected_recipe_targets_its_exact_onset_without_losing_source_phase() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let (placed, selected) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&before),
            at: AudioSample(137),
        },
    );
    let id = selected.unwrap();
    let mut event = placed.document.sounds()[&id].clone();
    let frames = event.mapping.duration_frames(FrameDuration::ZERO).unwrap();
    let trim = ExactRatio::new(1, 7).unwrap();
    event.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames,
        selection: ExactFrameRange {
            start: trim,
            end: frames.checked_sub(trim).unwrap(),
        },
    };
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&before.path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::SetSound {
            id: id.clone(),
            event: event.clone(),
        },
        "external-selected-sound",
    );
    drop(store);
    let selected = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    let (moved, _) = changed(
        &harness.service,
        &selected,
        ProjectSoundEdit::Move {
            id: id.clone(),
            at: AudioSample(527),
        },
    );
    let result = &moved.document.sounds()[&id];
    assert_eq!(result.source, event.source);
    assert_eq!(result.offset, AudioSample(527));
    let SourceAudioMapping::SelectedPlacement {
        start,
        frames: retained,
        selection,
    } = result.mapping
    else {
        panic!("selected mapping")
    };
    assert_eq!(retained, frames);
    assert_eq!(start, ExactRatio::ZERO.checked_sub(trim).unwrap());
    assert_eq!(selection.start, ExactRatio::ZERO);
    assert_eq!(
        selection.end,
        frames.checked_sub(trim).unwrap().checked_sub(trim).unwrap()
    );
    assert_eq!(
        moved.plan.root_sound(&id).unwrap().audible_samples().start,
        AudioSample(527)
    );
    same_picture(&selected, &moved);
    let restored = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: moved.document.revision_id().clone(),
        },
    );
    assert!(restored.error.is_none(), "{:?}", restored.error);
    let restored = restored.workspace.unwrap();
    assert_eq!(restored.document.sounds()[&id], event);
    let (nudged, _) = changed(
        &harness.service,
        &restored,
        ProjectSoundEdit::Nudge {
            id: id.clone(),
            frames: 3,
        },
    );
    let nudge = &nudged.document.sounds()[&id];
    assert_eq!(nudge.offset, event.offset);
    assert_eq!(nudge.source, event.source);
    let SourceAudioMapping::SelectedPlacement {
        start,
        frames: retained,
        selection,
    } = nudge.mapping
    else {
        panic!("selected mapping")
    };
    assert_eq!(start, ExactRatio::integer(3));
    assert_eq!(retained, frames);
    assert_eq!(
        selection.start,
        trim.checked_add(ExactRatio::integer(3)).unwrap()
    );
    assert_eq!(
        selection.end,
        frames
            .checked_sub(trim)
            .unwrap()
            .checked_add(ExactRatio::integer(3))
            .unwrap()
    );
    // Selection relative to the complete mapping is the original source phase.
    assert_eq!(selection.start.checked_sub(start).unwrap(), trim);
    let (inverse, _) = changed(
        &harness.service,
        &nudged,
        ProjectSoundEdit::Nudge {
            id: id.clone(),
            frames: -3,
        },
    );
    assert_eq!(inverse.document.sounds()[&id], event);
    same_picture(&restored, &inverse);
    command(&harness.service, ProjectRequest::Close);
}

fn catalog_at_rate(
    harness: &Harness,
    path: &Path,
    rate: deadpan_core::FrameRate,
) -> Arc<Workspace> {
    let document = ProjectDocument::new(
        ProjectId::new("nudge").unwrap(),
        RevisionId::new("initial").unwrap(),
        deadpan_core::PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: rate,
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut store = ProjectStore::create(path, &document).unwrap();
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(node("hold"), BeatNode::hold("Hold", hold(100_000)))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "hold",
    );
    drop(store);
    let before = command(&harness.service, ProjectRequest::Open(path.into()))
        .workspace
        .unwrap();
    let update = command(
        &harness.service,
        ProjectRequest::ImportSound {
            expected_session: before.session,
            expected_revision: before.document.revision_id().clone(),
            path: fixture("../audio-fixtures/pcm-stereo-48000.wav"),
            stream: None,
            interpretation: Some(AudioLayoutInterpretation::StereoLeftRight),
            ownership: OriginalOwnership::Managed,
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    harness.finish(harness.job());
    harness.finish(harness.job());
    complete(&harness.service)
}

#[test]
fn frame_nudges_are_count_equivalent_and_semantically_reversible_on_fractional_sample_grids() {
    for rate in [
        deadpan_core::FrameRate::new(30_000, 1001).unwrap(),
        deadpan_core::FrameRate::new(32_000, 1).unwrap(),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let harness = Harness::new();
        let before = catalog_at_rate(&harness, &scratch.path().join("nudge.deadpan"), rate);
        let (placed, selected) = changed(
            &harness.service,
            &before,
            ProjectSoundEdit::Place {
                asset: catalog_asset(&before),
                at: AudioSample(137),
            },
        );
        let id = selected.unwrap();
        let original = placed.document.sounds()[&id].clone();
        let (once, _) = changed(
            &harness.service,
            &placed,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: 1,
            },
        );
        let (twice, _) = changed(
            &harness.service,
            &once,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: 1,
            },
        );
        assert_eq!(once.document.sounds()[&id].offset, AudioSample(137));
        assert_eq!(twice.document.sounds()[&id].offset, AudioSample(137));
        assert_eq!(twice.document.sounds()[&id].source, original.source);
        assert_eq!(
            twice.document.sounds()[&id].mapping.start_frames(),
            ExactRatio::integer(2)
        );
        let mut reset = twice.clone();
        for _ in 0..2 {
            let undone = command(
                &harness.service,
                ProjectRequest::Undo {
                    expected_revision: reset.document.revision_id().clone(),
                },
            );
            assert!(undone.error.is_none(), "{:?}", undone.error);
            reset = undone.workspace.unwrap();
        }
        assert_eq!(reset.document.sounds()[&id], original);
        let (counted, _) = changed(
            &harness.service,
            &reset,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: 2,
            },
        );
        assert_eq!(counted.document.sounds(), twice.document.sounds());
        assert_eq!(counted.document.nodes(), placed.document.nodes());
        if rate.denominator() == 1001 {
            assert_eq!(
                counted
                    .plan
                    .root_sound(&id)
                    .unwrap()
                    .audible_samples()
                    .start,
                AudioSample(3340)
            );
        } else {
            assert_eq!(
                once.plan.root_sound(&id).unwrap().audible_samples().start,
                AudioSample(138)
            );
        }
        let (returned, _) = changed(
            &harness.service,
            &counted,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: -2,
            },
        );
        let (one, _) = changed(
            &harness.service,
            &returned,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: 1,
            },
        );
        let (inverse, _) = changed(
            &harness.service,
            &one,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: -1,
            },
        );
        assert_eq!(inverse.document.sounds(), returned.document.sounds());
        let restored = &inverse.document.sounds()[&id];
        assert_eq!(restored.offset, original.offset);
        assert_eq!(restored.source, original.source);
        assert_eq!(
            restored.mapping.start_frames(),
            original.mapping.start_frames()
        );
        assert_eq!(
            restored
                .mapping
                .duration_frames(FrameDuration::ZERO)
                .unwrap(),
            original
                .mapping
                .duration_frames(FrameDuration::ZERO)
                .unwrap()
        );
        assert_eq!(
            restored
                .mapping
                .selection_frames_with_offset(FrameDuration::ZERO, restored.offset, rate)
                .unwrap(),
            original
                .mapping
                .selection_frames_with_offset(FrameDuration::ZERO, original.offset, rate)
                .unwrap()
        );
        assert_eq!(
            inverse.plan.root_sound(&id).unwrap().audible_samples(),
            placed.plan.root_sound(&id).unwrap().audible_samples()
        );
        assert_failed(
            &harness.service,
            &inverse,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: -100_000,
            },
            "onset",
        );
        assert_failed(
            &harness.service,
            &inverse,
            ProjectSoundEdit::Nudge {
                id: id.clone(),
                frames: 100_000,
            },
            "past the end",
        );
        for frames in [i64::MIN, i64::MAX] {
            let rejected = command(
                &harness.service,
                request(
                    &inverse,
                    ProjectSoundEdit::Nudge {
                        id: id.clone(),
                        frames,
                    },
                ),
            );
            assert!(rejected.error.is_some());
            assert!(rejected.committed.is_none());
            assert_eq!(*rejected.workspace.unwrap().document, *inverse.document);
        }
        command(&harness.service, ProjectRequest::Close);
    }
}
