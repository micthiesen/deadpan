//! Real qualified sound permissions through the native revision-bound service.

use super::*;
use crate::project::sound::pause_target;

fn edit(
    id: &SoundId,
    target: &crate::project::sound::PauseTarget,
    allowed: bool,
) -> ProjectSoundEdit {
    ProjectSoundEdit::Allowance {
        id: id.clone(),
        issuer: target.issuer.clone(),
        at: target.at,
        allowed,
    }
}

#[test]
fn selected_sound_between_frame_boundaries_can_be_allowed_and_revoked_atomically() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(None);
    let before = catalog_at_rate(
        &harness,
        &scratch.path().join("subframe-allowance.deadpan"),
        deadpan_core::FrameRate::new(24, 1).unwrap(),
    );
    let split = edited(
        &harness.service,
        &before,
        ProjectEdit::Split {
            node: node("hold"),
            at: FrameDuration::new(1).unwrap(),
        },
    )
    .workspace
    .unwrap();
    let (placed, id) = changed(
        &harness.service,
        &split,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&split),
            at: AudioSample(0),
        },
    );
    let id = id.unwrap();
    let mut event = placed.document.sounds()[&id].clone();
    event.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: event.mapping.duration_frames(FrameDuration::ZERO).unwrap(),
        selection: ExactFrameRange {
            start: ExactRatio::new(1, 4).unwrap(),
            end: ExactRatio::new(1, 2).unwrap(),
        },
    };
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&before.path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::SetSound {
            id: id.clone(),
            event,
        },
        "subframe-selection",
    );
    drop(store);
    let selected = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    let sound = selected.plan.root_sound(&id).unwrap();
    assert_eq!(sound.audible_samples(), AudioSample(500)..AudioSample(1000));
    assert!(!sound.selects_sample(AudioSample(0)).unwrap());
    let target = pause_target(&selected, &id, ProjectFrame(0)).unwrap();
    let other = pause_target(&selected, &id, ProjectFrame(1)).unwrap();
    assert!(target.selected_support);
    assert!(!other.selected_support);
    assert_ne!(target.issuer, other.issuer);
    assert_failed(
        &harness.service,
        &selected,
        edit(&id, &other, true),
        "no retained selection",
    );
    let (allowed, _) = changed(&harness.service, &selected, edit(&id, &target, true));
    assert!(pause_target(&allowed, &id, target.at).unwrap().allowed);
    assert!(!pause_target(&allowed, &id, other.at).unwrap().allowed);
    let (revoked, _) = changed(&harness.service, &allowed, edit(&id, &target, false));
    assert!(revoked.document.sound_allowances().is_empty());
    let mut current = revoked;
    for (redo, expected) in [(false, true), (false, false), (true, true), (true, false)] {
        let revision = current.document.revision_id().clone();
        let update = command(
            &harness.service,
            if redo {
                ProjectRequest::Redo {
                    expected_revision: revision,
                }
            } else {
                ProjectRequest::Undo {
                    expected_revision: revision,
                }
            },
        );
        assert!(update.error.is_none(), "{:?}", update.error);
        current = update.workspace.unwrap();
        assert_eq!(
            pause_target(&current, &id, target.at).unwrap().allowed,
            expected
        );
        assert!(!pause_target(&current, &id, other.at).unwrap().allowed);
        assert_eq!(current.document.nodes(), selected.document.nodes());
        assert_eq!(current.document.sounds(), selected.document.sounds());
        assert_eq!(
            current.document.sound_routes(),
            selected.document.sound_routes()
        );
    }
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn exact_pause_allowance_is_scoped_reversible_and_durable() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(None);
    let before = catalog_at_rate(
        &harness,
        &scratch.path().join("allowance.deadpan"),
        deadpan_core::FrameRate::new(24, 1).unwrap(),
    );
    let (first, id) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&before),
            at: AudioSample(0),
        },
    );
    let id = id.unwrap();
    let (second, other) = changed(
        &harness.service,
        &first,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&first),
            at: AudioSample(0),
        },
    );
    let other = other.unwrap();
    let target = pause_target(&second, &id, ProjectFrame(1)).unwrap();
    assert!(target.selected_support && !target.allowed);
    assert_eq!(target.label, second.document.nodes()[&node("hold")].label);
    let (allowed, selected) = changed(&harness.service, &second, edit(&id, &target, true));
    assert_eq!(selected.as_ref(), Some(&id));
    assert!(pause_target(&allowed, &id, target.at).unwrap().allowed);
    assert!(!pause_target(&allowed, &other, target.at).unwrap().allowed);
    assert_eq!(allowed.document.nodes(), second.document.nodes());
    assert_eq!(allowed.document.sounds(), second.document.sounds());
    assert_eq!(
        allowed.document.sound_routes(),
        second.document.sound_routes()
    );
    assert_eq!(allowed.document.assets(), second.document.assets());
    assert_eq!(allowed.plan.duration(), second.plan.duration());

    // A request captured before the grant cannot write through its completion.
    let stale = command(
        &harness.service,
        request(&second, edit(&id, &target, false)),
    );
    assert!(stale.error.is_some());
    assert!(stale.committed.is_none());
    assert_eq!(*stale.workspace.unwrap().document, *allowed.document);
    assert_failed(
        &harness.service,
        &allowed,
        edit(&id, &target, true),
        "already allowed",
    );
    let mut wrong = target.clone();
    wrong.issuer = deadpan_core::SoundHoldIssuer::Node {
        instance: deadpan_core::InstancePath {
            node: node("root"),
            repeats: Vec::new(),
        },
    };
    assert_failed(
        &harness.service,
        &allowed,
        edit(&id, &wrong, false),
        "identified pause changed",
    );
    let mut absent = target.clone();
    absent.at = ProjectFrame(100_000);
    assert_failed(
        &harness.service,
        &allowed,
        edit(&id, &absent, false),
        "inside a silent pause",
    );

    // Revocation remains available at a later sample of this exact issuer even
    // where the short sound itself is exhausted. It grants no fresh support.
    let late = pause_target(&allowed, &id, ProjectFrame(10)).unwrap();
    assert!(late.allowed && !late.selected_support);
    let (silenced, _) = changed(&harness.service, &allowed, edit(&id, &late, false));
    assert!(silenced.document.sound_allowances().is_empty());
    command(&harness.service, ProjectRequest::Close);
    let mut current = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*current.document, *silenced.document);
    for (redo, expected) in [(false, true), (false, false), (true, true), (true, false)] {
        let revision = current.document.revision_id().clone();
        let update = command(
            &harness.service,
            if redo {
                ProjectRequest::Redo {
                    expected_revision: revision,
                }
            } else {
                ProjectRequest::Undo {
                    expected_revision: revision,
                }
            },
        );
        assert!(update.error.is_none(), "{:?}", update.error);
        current = update.workspace.unwrap();
        assert_eq!(
            pause_target(&current, &id, target.at).unwrap().allowed,
            expected
        );
        assert!(!pause_target(&current, &other, target.at).unwrap().allowed);
        assert_eq!(current.document.nodes(), second.document.nodes());
        assert_eq!(current.document.sounds(), second.document.sounds());
    }
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn routed_gap_cannot_be_filled_but_a_new_sound_can_be_allowed_in_the_same_pause() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let (placed, id) = changed(
        &harness.service,
        &before,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&before),
            at: AudioSample(0),
        },
    );
    let id = id.unwrap();
    assert!(
        pause_target(&placed, &id, ProjectFrame(1))
            .unwrap_err()
            .contains("No single silent pause")
    );
    let paused = edited(
        &harness.service,
        &placed,
        ProjectEdit::InsertTime {
            at: ProjectFrame(1),
            duration: FrameDuration::new(2).unwrap(),
        },
    )
    .workspace
    .unwrap();
    let gap = pause_target(&paused, &id, ProjectFrame(1)).unwrap();
    assert!(!gap.selected_support);
    assert_failed(
        &harness.service,
        &paused,
        edit(&id, &gap, true),
        "cannot fill a timing gap",
    );
    let at = paused
        .document
        .presentation_basis()
        .frame_rate
        .audio_boundary(gap.at)
        .unwrap();
    let (fresh, new_id) = changed(
        &harness.service,
        &paused,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&paused),
            at,
        },
    );
    let new_id = new_id.unwrap();
    let target = pause_target(&fresh, &new_id, gap.at).unwrap();
    assert_eq!(target.issuer, gap.issuer);
    assert!(target.selected_support);
    let (allowed, _) = changed(&harness.service, &fresh, edit(&new_id, &target, true));
    assert_eq!(
        allowed.document.sound_routes(),
        fresh.document.sound_routes()
    );
    assert!(!pause_target(&allowed, &id, gap.at).unwrap().allowed);
    assert!(pause_target(&allowed, &new_id, gap.at).unwrap().allowed);
    same_picture(&fresh, &allowed);
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn current_pause_target_stays_indexed_and_names_one_of_a_million_repeat_plays() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(None);
    let before = catalog_at_rate(
        &harness,
        &scratch.path().join("repeat-allowance.deadpan"),
        deadpan_core::FrameRate::new(24, 1).unwrap(),
    );
    let repeated = edited(
        &harness.service,
        &before,
        ProjectEdit::WrapRepeat {
            node: node("hold"),
            plays: 1_000_000,
        },
    )
    .workspace
    .unwrap();
    let at = ProjectFrame(999_999 * 100_000);
    let onset = repeated
        .document
        .presentation_basis()
        .frame_rate
        .audio_boundary(at)
        .unwrap();
    let (placed, id) = changed(
        &harness.service,
        &repeated,
        ProjectSoundEdit::Place {
            asset: catalog_asset(&repeated),
            at: onset,
        },
    );
    let id = id.unwrap();
    let last = pause_target(&placed, &id, at).unwrap();
    let first = pause_target(&placed, &id, ProjectFrame(0)).unwrap();
    assert_eq!(last.issuer.instance().node, first.issuer.instance().node);
    assert_ne!(last.issuer, first.issuer);
    assert_eq!(last.issuer.instance().repeats.len(), 1);
    assert_eq!(last.issuer.instance().repeats[0].iteration.ordinal, 999_999);
    assert!(last.label.contains("play 1000000 of 1000000"));
    assert!(first.label.contains("play 1 of 1000000"));
    assert!(last.selected_support);
    let (allowed, _) = changed(&harness.service, &placed, edit(&id, &last, true));
    assert!(pause_target(&allowed, &id, at).unwrap().allowed);
    assert!(
        !pause_target(&allowed, &id, ProjectFrame(0))
            .unwrap()
            .allowed
    );
    assert_eq!(allowed.document.nodes(), placed.document.nodes());
    assert_eq!(allowed.document.sounds(), placed.document.sounds());
    command(&harness.service, ProjectRequest::Close);
}
