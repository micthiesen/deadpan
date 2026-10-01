use super::*;
use deadpan_core::{LinkRelation, SourceAudioMapping};

#[test]
fn moments_outside_measured_audio_keep_dormant_link_through_history_and_reopen() -> Result {
    for ordinals in [0..10, 80..90] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = project(scratch.path())?;
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dormant-audio/middle-audio.mp4");
        let original = store
            .retain_original(&fixture, OriginalOwnership::Managed, limits(), &cancelled())?
            .record;
        let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
        let registration =
            primary_registration(&store, &original, "registered", "original", "full");
        store.register_prepared_source(&registration, &prepared, None, &cancelled())?;
        let before = store.snapshot()?;
        let before_counts = counts(&path)?;
        let mut input = paste(&store, "dormant-paste")?;
        input.ordinals = ordinals.clone();
        let preview = store.preview_prepared_source_moment(&input, &prepared, &cancelled())?;
        assert_eq!(counts(&path)?, before_counts);
        let expected = preview.forward.apply(&before)?;
        let NodeKind::Source { source } = &expected.nodes()[&input.node].kind else {
            panic!("moment lost its Source");
        };
        let audio = source
            .audio
            .as_ref()
            .expect("a silent moment must retain its dormant Original audio context");
        assert_eq!(source.link, LinkRelation::Linked);
        assert_eq!(audio.asset, input.asset);
        assert_eq!(Some(audio.span), before.assets()[&input.asset].audio);
        assert_eq!(audio.span.start().ticks, 43_076);
        assert_eq!(audio.span.end().ticks, 88_217);
        assert_eq!(source.duration.frames(), 10);
        assert!(matches!(
            source.audio_mapping,
            SourceAudioMapping::SelectedPlacement { .. }
        ));
        let selected = source.audio_mapping.selection_frames_with_offset(
            source.duration,
            source.audio_offset,
            before.presentation_basis().frame_rate,
        )?;
        assert_eq!(selected.start, selected.end, "{ordinals:?}");
        assert_eq!(expected.assets(), before.assets());
        store.commit_prepared_source_moment(&input, &prepared, None, &cancelled())?;
        assert_eq!(store.snapshot()?, expected);
        assert_eq!(
            counts(&path)?,
            (before_counts.0 + 1, before_counts.1 + 1, before_counts.2)
        );
        drop(store);

        let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(reopened.snapshot()?, expected);
        reopened.validate()?;
        reopened.undo(expected.revision_id(), revision("undo-dormant"))?;
        let undone = reopened.snapshot()?;
        assert_eq!(undone.nodes(), before.nodes());
        assert_eq!(undone.assets(), before.assets());
        assert_eq!(undone.audio_bindings(), before.audio_bindings());
        reopened.redo(undone.revision_id(), revision("redo-dormant"))?;
        let redone = reopened.snapshot()?;
        assert_eq!(redone.nodes(), expected.nodes());
        assert_eq!(redone.assets(), expected.assets());
        assert_eq!(redone.audio_bindings(), expected.audio_bindings());
        reopened.validate()?;
    }
    Ok(())
}
