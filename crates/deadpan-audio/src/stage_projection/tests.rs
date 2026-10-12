use super::*;
use deadpan_core::{FrameRate, NodeId, NodeKind, SourceAudioMapping, SourceSpan};
use deadpan_plan::AudioSourceVoiceRecipe;

fn fixture() -> (
    Arc<RenderPlan>,
    super::super::controlled_reads::Provider,
    AudioSourceVoiceRecipe,
) {
    let (document, provider) = super::super::controlled_reads::fixture_document_at_rate(
        FrameRate::new(48_000, 1).unwrap(),
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("d".repeat(64));
    wire["assets"]["media"]["still_image"] = serde_json::json!(false);
    let document = deadpan_core::ProjectDocument::from_json(&wire.to_string()).unwrap();
    let NodeKind::Source { source } = &document.nodes()[&NodeId::new("a").unwrap()].kind else {
        panic!()
    };
    let mut source = source.audio.clone().unwrap();
    let mut end = source.span.end();
    end.ticks = 128;
    source.span = SourceSpan::new(source.span.start(), end).unwrap();
    let recipe = AudioSourceVoiceRecipe {
        mapping: SourceAudioMapping::natural_rate(source.span, FrameRate::new(48_000, 1).unwrap())
            .unwrap(),
        source,
        offset: AudioSample(0),
    };
    (
        Arc::new(RenderPlan::compile(&document).unwrap()),
        provider,
        recipe,
    )
}

fn projection(plan: &RenderPlan, recipe: AudioSourceVoiceRecipe) -> Arc<AudioStageProjection<'_>> {
    plan.source_voice_occurrence(
        InstancePath {
            node: NodeId::new("b").unwrap(),
            repeats: vec![],
        },
        recipe,
        Default::default(),
    )
    .unwrap()
    .processing_projection()
    .unwrap()
    .clone()
}

#[test]
fn source_projection_reconstruction_reuses_pcm_without_aliasing_recipes_or_plans() {
    let (plan, mut provider, recipe) = fixture();
    let mut reader = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let budget = PreparationBudget::new(Duration::from_secs(10), &cancelled).unwrap();
    let control = budget.control();
    let first_projection = projection(&plan, recipe.clone());
    let first = reader
        .prepare_projected(&first_projection, &mut provider, control, 1)
        .unwrap();
    for _ in 0..80 {
        let rebuilt = projection(&plan, recipe.clone());
        assert_ne!(rebuilt.identity(), first_projection.identity());
        assert_eq!(rebuilt.source_key(), first_projection.source_key());
        let block = reader
            .prepare_projected(&rebuilt, &mut provider, control, 1)
            .unwrap();
        assert!(Arc::ptr_eq(&first, &block));
    }
    assert_eq!(control.work.borrow().prepared_stages, 1);
    assert!(provider.calls >= 81, "every reuse re-admits its dependency");
    let mut shifted = recipe.clone();
    shifted.offset = AudioSample(11);
    let shifted = reader
        .prepare_projected(&projection(&plan, shifted), &mut provider, control, 1)
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &shifted));
    assert_ne!(
        first
            .samples
            .read_window(0..256, control.cancelled)
            .unwrap(),
        shifted
            .samples
            .read_window(0..256, control.cancelled)
            .unwrap()
    );
    assert_eq!(control.work.borrow().prepared_stages, 2);

    let other = Arc::new((*plan).clone());
    let foreign = projection(&other, recipe);
    assert!(matches!(
        reader.prepare_projected(&foreign, &mut provider, control, 1),
        Err(StageAudioError::ForeignDomain)
    ));
    let independent = reader
        .with_plan_scope(Arc::clone(&other), |reader| {
            reader.prepare_projected(&foreign, &mut provider, control, 1)
        })
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &independent));
    assert_eq!(control.work.borrow().prepared_stages, 3);
}

#[test]
fn source_projection_reuse_rechecks_depth_source_and_preparation_limits() {
    let (plan, mut provider, recipe) = fixture();
    let mut reader = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let cancelled = AtomicBool::new(false);
    let budget = PreparationBudget::new(Duration::from_secs(10), &cancelled).unwrap();
    let control = budget.control();
    reader
        .prepare_projected(
            &projection(&plan, recipe.clone()),
            &mut provider,
            control,
            1,
        )
        .unwrap();
    assert!(matches!(
        reader.prepare_projected(
            &projection(&plan, recipe.clone()),
            &mut provider,
            control,
            reader.limits.maximum_depth + 1
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    struct Missing;
    impl AudioSourceProvider for Missing {
        fn source(
            &mut self,
            _: &ProjectId,
            _: &RevisionId,
            _: &AssetId,
            _: &AtomicBool,
        ) -> Result<&crate::PreparedSource, PreparationError> {
            Err(PreparationError::SourceUnavailable("revoked".into()))
        }
    }
    assert!(matches!(
        reader.prepare_projected(&projection(&plan, recipe.clone()), &mut Missing, control, 1),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    let mut shifted = recipe;
    shifted.offset = AudioSample(11);
    assert!(matches!(
        reader.prepare_projected(&projection(&plan, shifted), &mut provider, control, 1),
        Err(StageAudioError::Limit("preflight stages per read"))
    ));
}
