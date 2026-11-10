//! A sound attached to a beat, as the beat inspector lists it, then changed
//! and detached through the native revision-bound service, with exact Undo.

use super::*;
use deadpan_cli::sound_events::attached_sounds;

#[test]
fn attached_sounds_list_change_detach_and_undo_through_the_service() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = catalog(&harness);
    let root = before.document.root().clone();
    let owner = before.document.children(&root).next().unwrap().clone();
    let update = command(
        &harness.service,
        request(
            &before,
            ProjectSoundEdit::Attach {
                asset: catalog_asset(&before),
                parent: root.clone(),
                owner: owner.clone(),
                at: ProjectFrame(2),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let attached = update.workspace.unwrap();
    // The inspector's list: one sound, two frames into its beat, audible.
    let [listed] = attached_sounds(&attached.document, &owner)
        .try_into()
        .unwrap_or_else(|rows: Vec<_>| panic!("{rows:?}"));
    assert_eq!(listed.owner, owner);
    assert_eq!(listed.start, ExactRatio::integer(2));
    assert!(listed.audible.is_some());
    assert_eq!(listed.plays, None);
    let id = listed.id.clone();
    let event = attached.document.beat_sounds()[&owner][&id].clone();

    // `:sound-gain` on the selected attached sound keeps its place.
    let mut louder = event.clone();
    louder.treatments = louder
        .treatments
        .with_trim(deadpan_core::GainDb::new(-4500).unwrap())
        .unwrap();
    let update = command(
        &harness.service,
        request(
            &attached,
            ProjectSoundEdit::SetAttached {
                owner: owner.clone(),
                id: id.clone(),
                event: Box::new(louder.clone()),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let changed = update.workspace.unwrap();
    assert_eq!(changed.document.beat_sounds()[&owner][&id], louder);
    assert_eq!(changed.document.nodes(), before.document.nodes());
    // Moving it is the beat's business: refused, nothing saved.
    let mut moved = louder.clone();
    moved.mapping = SourceAudioMapping::Placement {
        start: ExactRatio::integer(3),
        frames: moved.mapping.duration_frames(FrameDuration::ZERO).unwrap(),
    };
    assert_failed(
        &harness.service,
        &changed,
        ProjectSoundEdit::SetAttached {
            owner: owner.clone(),
            id: id.clone(),
            event: Box::new(moved),
        },
        "keeps its place on the beat",
    );

    let update = command(
        &harness.service,
        request(
            &changed,
            ProjectSoundEdit::Detach {
                owner: owner.clone(),
                id: id.clone(),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let detached = update.workspace.unwrap();
    assert!(detached.document.beat_sounds().is_empty());
    assert!(attached_sounds(&detached.document, &owner).is_empty());
    assert_eq!(detached.document.nodes(), before.document.nodes());
    // A second detach of the same sound is refused readably.
    assert_failed(
        &harness.service,
        &detached,
        ProjectSoundEdit::Detach {
            owner: owner.clone(),
            id: id.clone(),
        },
        "no longer exists",
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: detached.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.beat_sounds()[&owner][&id], louder);
    assert_eq!(attached_sounds(&undone.document, &owner), vec![listed]);
    command(&harness.service, ProjectRequest::Close);
}
