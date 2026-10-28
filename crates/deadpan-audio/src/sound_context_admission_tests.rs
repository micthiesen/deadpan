use super::*;
use crate::stages::{authored_eq::eq_admission_fixture as eq, controlled_reads};
use deadpan_core::*;

fn wav_fixture() -> (Arc<RenderPlan>, controlled_reads::Provider) {
    let (doc, provider) =
        controlled_reads::fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let treatments = AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(
                [3000, -6000, 1500].map(|g| GainDb::new(g).unwrap()),
                None,
                None,
            )
            .unwrap(),
        ))
        .unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(treatments).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&doc).unwrap()), provider)
}
#[test]
fn actual_cache_is_evicted_before_sound_metadata_then_sources_readmit() {
    let (plan, mut provider) = wav_fixture();
    let cancelled = AtomicBool::new(false);
    let domain = plan
        .audio_domain_at(AudioSample(700), Default::default())
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let before = renderer
        .read_domain(
            &mut provider,
            &domain,
            AudioSample(700),
            1,
            Duration::from_secs(30),
            &cancelled,
        )
        .unwrap();
    let cached = renderer.cached_resident_frames();
    assert!(cached > 0 && !renderer.cache.is_empty());
    renderer.limits.maximum_resident_frames = u32::try_from(cached).unwrap();
    let work = RefCell::new(ReadWork::default());
    let control = eq::control(&cancelled, &work);
    let root = InstancePath {
        node: NodeId::new("root").unwrap(),
        repeats: Vec::new(),
    };
    let context = {
        let mut admission = Admission::new(&mut renderer, control);
        plan.audio_sound_instance_context_admitted(
            &root,
            control.query_limits().unwrap(),
            &mut admission,
        )
        .unwrap()
        .into_shared(&mut admission)
        .unwrap()
    };
    assert!(
        renderer.cache.is_empty(),
        "make_room must evict the actual prepared stage"
    );
    assert!(work.borrow().sound_context_frames > 0);
    drop(context);
    assert_eq!(work.borrow().sound_context_frames, 0);
    renderer.limits.maximum_resident_frames = StageLimits::default().maximum_resident_frames;
    let calls = provider.calls;
    let after = renderer
        .read_domain(
            &mut provider,
            &domain,
            AudioSample(700),
            1,
            Duration::from_secs(30),
            &cancelled,
        )
        .unwrap();
    assert_eq!(
        before
            .samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>(),
        after
            .samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert!(provider.calls > calls);
}
#[test]
fn real_wav_pcm_source_dependencies_and_depth_ignore_sound_metadata_path() {
    let (plan, mut left_provider) = wav_fixture();
    let (_, mut right_provider) = wav_fixture();
    let cancelled = AtomicBool::new(false);
    let mut left = StageAudio::new(Arc::clone(&plan));
    let mut right = StageAudio::new(Arc::clone(&plan));
    let a = RefCell::new(ReadWork::default());
    let b = RefCell::new(ReadWork::default());
    let expected = left
        .read_shared_input(
            &mut left_provider,
            AudioSample(128),
            32,
            eq::control(&cancelled, &a),
            shared_eq::InputSelection {
                original_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    let actual = right
        .read_shared_input(
            &mut right_provider,
            AudioSample(128),
            32,
            eq::control(&cancelled, &b),
            shared_eq::InputSelection::default(),
        )
        .unwrap();
    assert!(actual.samples.iter().flatten().any(|v| *v != 0.0));
    assert_eq!(
        expected
            .samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>(),
        actual
            .samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(expected.dependencies, actual.dependencies);
    assert_eq!(expected.suppressed, actual.suppressed);
    assert_eq!(expected.exhausted, actual.exhausted);
    assert_eq!(expected.relative_depth, actual.relative_depth);
    assert_eq!(left_provider.calls, right_provider.calls);
    assert!(right_provider.calls > 0);
    assert_eq!(b.borrow().sound_context_frames, 0);
    assert_eq!(right.active_frames, 0);
}
#[test]
fn equal_contents_do_not_equate_request_ledger_issuers() {
    let a = RefCell::new(ReadWork::default());
    let b = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    assert!(issuer(eq::control(&cancelled, &a)) != issuer(eq::control(&cancelled, &b)));
    assert!(issuer(eq::control(&cancelled, &a)) == issuer(eq::control(&cancelled, &a)));
}

#[test]
fn actual_cache_eviction_precedes_original_copy_and_source_grid_stays_identical() {
    let (plan, mut provider) = wav_fixture();
    let cancelled = AtomicBool::new(false);
    let domain = plan
        .audio_domain_at(AudioSample(700), Default::default())
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let expected = renderer
        .read_domain(
            &mut provider,
            &domain,
            AudioSample(700),
            8,
            Duration::from_secs(30),
            &cancelled,
        )
        .unwrap();
    let cached = renderer.cached_resident_frames();
    assert!(cached > 0 && !renderer.cache.is_empty());
    renderer.limits.maximum_resident_frames = u32::try_from(cached).unwrap();
    let work = RefCell::new(ReadWork::default());
    let control = eq::control(&cancelled, &work);
    let copy = {
        let mut admission = Admission::new(&mut renderer, control);
        domain
            .clone_shared_admitted(control.query_limits().unwrap(), &mut admission)
            .unwrap()
    };
    assert!(
        renderer.cache.is_empty(),
        "actual metadata reserve evicts actual PCM"
    );
    assert!(copy.lease().unwrap().frames() > 0);
    drop(copy);
    assert_eq!(work.borrow().sound_context_frames, 0);
    renderer.limits.maximum_resident_frames = StageLimits::default().maximum_resident_frames;
    let calls = provider.calls;
    let actual = renderer
        .read_domain(
            &mut provider,
            &domain,
            AudioSample(700),
            8,
            Duration::from_secs(30),
            &cancelled,
        )
        .unwrap();
    assert_eq!(
        expected
            .samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>(),
        actual
            .samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(expected, actual);
    assert!(provider.calls > calls);
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn joint_transfer_rejects_equal_content_foreign_ledger_before_any_commit() {
    let (plan, _) = wav_fixture();
    let domain = plan
        .audio_domain_at(AudioSample(700), Default::default())
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let left = RefCell::new(ReadWork::default());
    let right = RefCell::new(ReadWork::default());
    let a = eq::control(&cancelled, &left);
    let b = eq::control(&cancelled, &right);
    let mut renderer = StageAudio::new(plan.clone());
    let value = {
        let mut admission = Admission::new(&mut renderer, a);
        domain
            .clone_shared_admitted(a.query_limits().unwrap(), &mut admission)
            .unwrap()
    };
    let frames = value.lease().unwrap().frames();
    assert!(matches!(
        preflight_transfer(b, value.lease(), None),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(left.borrow().sound_context_frames, frames);
    assert_eq!(right.borrow().sound_context_frames, 0);
    drop(value);
    assert_eq!(left.borrow().sound_context_frames, 0);
}
