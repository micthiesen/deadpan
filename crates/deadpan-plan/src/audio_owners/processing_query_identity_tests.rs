use super::*;

fn replace_asset_identity(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(value) if value == old => *value = new.to_owned(),
        Value::Array(values) => {
            for value in values {
                replace_asset_identity(value, old, new);
            }
        }
        Value::Object(values) => {
            if let Some(asset) = values.remove(old) {
                assert!(values.insert(new.to_owned(), asset).is_none());
            }
            for value in values.values_mut() {
                replace_asset_identity(value, old, new);
            }
        }
        _ => {}
    }
}

fn processing_source_edges(query: &crate::AudioProcessingQuery<'_>, asset: &str) -> usize {
    query
        .spans
        .iter()
        .map(|span| match &span.content {
            crate::AudioSignalContent::Leaf(
                crate::AudioContent::Source { source, .. }
                | crate::AudioContent::RoomTone { source, .. }
                | crate::AudioContent::Reverse { source, .. },
            ) => usize::from(source.asset.as_str() == asset),
            _ => 0,
        })
        .sum()
}

type ProcessingLayoutReceipt = std::collections::BTreeMap<(usize, usize), usize>;

fn processing_asset_observation(
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    asset: &str,
) -> (ProcessingLayoutReceipt, usize, Value) {
    let ordinary = match parent {
        Parent::Root(plan) => plan.audio_processing(samples.clone(), Default::default()),
        Parent::Domain(domain) => domain.processing(samples.clone(), Default::default()),
    }
    .unwrap();
    let ordinary_snapshot = processing::query_snapshot(parent.plan(), &ordinary);
    let edges = processing_source_edges(&ordinary, asset);
    assert!(
        edges > 0,
        "ordinary processing query must own this source asset"
    );

    let mut policy = Policy::<true>::new(usize::MAX);
    let mut observer = ProcessingObserver {
        policy: &mut policy,
        layouts: Vec::new(),
        spends: Vec::new(),
    };
    let admitted = processing_admitted(parent, samples, Default::default(), &mut observer).unwrap();
    assert!(admitted.was_forwarded());
    assert_eq!(admitted.canonical_units(), ordinary.work);
    assert_eq!(
        admitted.work(),
        admitted.canonical_units() + admitted.additional_units()
    );
    assert_eq!(observer.policy.0.borrow().forwarded, admitted.work());
    let mut admitted_snapshot = processing::query_snapshot(parent.plan(), admitted.query());
    admitted_snapshot["work"] = ordinary_snapshot["work"].clone();
    assert_eq!(
        admitted_snapshot, ordinary_snapshot,
        "admission changes only total work in this source-only query"
    );
    let mut receipt = ProcessingLayoutReceipt::new();
    for event in &observer.layouts {
        *receipt.entry((event.size, event.align)).or_default() += 1;
    }
    drop(admitted);
    assert_eq!(observer.policy.0.borrow().live, 0);
    (receipt, edges, ordinary_snapshot)
}

fn assert_processing_asset_layout_delta(
    original: &ProcessingLayoutReceipt,
    changed: &ProcessingLayoutReceipt,
    original_layout: Layout,
    changed_layout: Layout,
    source_edges: usize,
) {
    assert!(source_edges > 0);
    let original_key = (original_layout.size(), original_layout.align());
    let changed_key = (changed_layout.size(), changed_layout.align());
    assert_ne!(original_key, changed_key);
    let mut expected = original.clone();
    let count = expected
        .get_mut(&original_key)
        .expect("old asset Layout is independently reserved");
    assert!(*count >= source_edges);
    *count -= source_edges;
    *expected.entry(changed_key).or_default() += source_edges;
    expected.retain(|_, count| *count != 0);
    let mut actual = changed.clone();
    actual.retain(|_, count| *count != 0);
    assert_eq!(
        actual, expected,
        "only the source AssetId owning-edge Layouts change"
    );
}

fn processing_source_domain<'plan>(
    plan: &'plan RenderPlan,
    at: AudioSample,
) -> crate::AudioOwnerInput<'plan> {
    let observed = plan
        .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
        .unwrap();
    let owner = observed.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "voice")
        .expect("checked source fixture exposes the voice owner");
    plan.audio_owner_input(owner, at, Default::default())
        .unwrap()
}

#[test]
fn processing_asset_layout_delta_matches_each_root_and_domain_source_edge() {
    let original_asset = "processing-baseline-owning-media";
    let changed_asset = format!("long-asset-{}", "x".repeat(96));
    let original_layout = processing_identity_layout(original_asset);
    let changed_layout = processing_identity_layout(&changed_asset);
    assert_ne!(
        (original_layout.size(), original_layout.align()),
        (changed_layout.size(), changed_layout.align())
    );

    let mut checked = false;
    processing_visit(|parent, case| {
        if case["name"] != "leaf/source_fit_beat/root" {
            return;
        }
        let Parent::Root(original_plan) = parent else {
            panic!("source baseline must be an actual Root fixture");
        };
        assert_eq!(
            case["fixture_source"]["document"]["nodes"]["voice"]["kind"]["source"]["audio"]["asset"],
            original_asset
        );
        let original_samples = processing_request(processing_full(case));
        let (root_original, root_edges, root_body) =
            processing_asset_observation(parent, original_samples.clone(), original_asset);
        assert_eq!(root_edges, 1);
        assert_eq!(
            root_body,
            processing_full(case)["output"]["query"],
            "original ordinary Root body matches the frozen OLD oracle"
        );

        let mut changed_document = case["fixture_source"]["document"].clone();
        replace_asset_identity(&mut changed_document, original_asset, &changed_asset);
        let changed_document =
            deadpan_core::ProjectDocument::from_json(&changed_document.to_string()).unwrap();
        let changed_plan = RenderPlan::compile(&changed_document).unwrap();
        let changed_parent = Parent::Root(&changed_plan);
        let (root_changed, changed_root_edges, changed_root_body) =
            processing_asset_observation(changed_parent, original_samples, &changed_asset);
        assert_eq!(changed_root_edges, root_edges);
        let mut normalized_changed_root = changed_root_body;
        replace_asset_identity(&mut normalized_changed_root, &changed_asset, original_asset);
        assert_eq!(
            normalized_changed_root,
            processing_full(case)["output"]["query"],
            "changed ordinary Root output differs from frozen OLD body only in AssetId text"
        );
        assert_processing_asset_layout_delta(
            &root_original,
            &root_changed,
            original_layout,
            changed_layout,
            root_edges,
        );

        let at = AudioSample(0);
        let original_input = processing_source_domain(original_plan, at);
        let original_domain = original_input.context();
        let original_domain_samples = original_domain.root_samples();
        assert_eq!(original_domain_samples, AudioSample(0)..AudioSample(8));
        let (domain_original, domain_edges, domain_body) = processing_asset_observation(
            Parent::Domain(original_domain),
            original_domain_samples,
            original_asset,
        );
        assert_eq!(domain_edges, 1);

        let changed_input = processing_source_domain(&changed_plan, at);
        let changed_domain = changed_input.context();
        let (domain_changed, changed_domain_edges, changed_domain_body) =
            processing_asset_observation(
                Parent::Domain(changed_domain),
                changed_domain.root_samples(),
                &changed_asset,
            );
        assert_eq!(changed_domain_edges, domain_edges);
        let mut normalized_changed_domain = changed_domain_body;
        replace_asset_identity(
            &mut normalized_changed_domain,
            &changed_asset,
            original_asset,
        );
        assert_eq!(normalized_changed_domain, domain_body);
        assert_processing_asset_layout_delta(
            &domain_original,
            &domain_changed,
            original_layout,
            changed_layout,
            domain_edges,
        );
        checked = true;
    });
    assert!(checked);
}
