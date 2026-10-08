use super::*;
use crate::generation_inputs::{ExtensionCapturePolicy, GenerationInputs, InputCaptureBudget};

fn extension(direction: ExtensionDirection, native_rate: FrameRate) -> GenerationCaptureSpec {
    GenerationCaptureSpec::Extension {
        direction,
        native_rate,
        context_frames: 9,
        policy: ExtensionCapturePolicy::TemporalContextV1,
    }
}

fn capture(
    connection: &Connection,
    document: &ProjectDocument,
    name: &str,
    capture: GenerationCaptureSpec,
) -> GenerationInputBinding {
    GenerationInputBinding::capture_with_plan(
        document,
        &RenderPlan::compile(document).unwrap(),
        &target(name),
        capture,
        None,
        &QualifiedGenerationPictures::new(connection),
        &mut InputCaptureBudget::default(),
    )
    .unwrap()
}

fn wide_document(reverse_names: bool) -> (ProjectDocument, Vec<String>) {
    let providers: Vec<_> = (0..24)
        .map(|index| {
            format!(
                "provider-{:02}",
                if reverse_names { 23 - index } else { index }
            )
        })
        .collect();
    let mut names = vec!["lead"];
    names.extend(providers.iter().map(String::as_str));
    names.extend(["consumer", "tail"]);
    let mut generated: Vec<_> = providers.iter().map(String::as_str).collect();
    generated.push("consumer");
    (fixture(&names, &generated), providers)
}

fn sparse_capture() -> GenerationCaptureSpec {
    // Exact 30-project-frame spacing covers twenty provider Holds with only
    // nine samples. This is a structural closure fixture, not provider admission
    // or a claim that the current runtime accepts this native rate.
    extension(ExtensionDirection::FromLeft, FrameRate::new(1, 1).unwrap())
}

fn context(document: &ProjectDocument) -> deadpan_plan::ScopedHoldContext {
    RenderPlan::compile(document)
        .unwrap()
        .scoped_hold_context(
            &deadpan_plan::ScopedHoldContextRequest {
                target: target("consumer"),
                direction: ExtensionDirection::FromLeft,
                native_rate: FrameRate::new(1, 1).unwrap(),
                frame_count: 9,
            },
            BoundaryQueryLimits::default(),
        )
        .unwrap()
}

fn hidden_provider(document: &ProjectDocument) -> String {
    let context = context(document);
    let sampled: BTreeSet<_> = context
        .pictures
        .iter()
        .map(|sample| &sample.instance.node)
        .collect();
    context
        .coverage
        .spans
        .iter()
        .map(|span| &span.start.instance.node)
        .find(|node| !sampled.contains(node))
        .unwrap()
        .as_str()
        .to_owned()
}

#[test]
fn hidden_support_provider_replacement_changes_final_binding_even_when_every_sample_is_unchanged() {
    let (document, providers) = wide_document(false);
    let connection = connection();
    let original = capture(&connection, &document, "consumer", sparse_capture());
    let hidden = hidden_provider(&document);
    save_origin(&connection, &document, "consumer", original.clone());
    // Test-only immutable metadata exercises closure, not extension Ready or
    // acceptance admission. Production origins still require real Bridge proof.
    save_origin(&connection, &document, &hidden, binding(None, None));
    let exclusions = providers
        .iter()
        .filter(|name| **name != hidden)
        .map(|name| target(name))
        .collect();
    let result = derive(&connection, document.clone(), exclusions, BTreeSet::new());
    let names: BTreeSet<_> = result
        .entries
        .iter()
        .map(|entry| entry.target.node.as_str())
        .collect();
    assert_eq!(names, BTreeSet::from(["consumer", hidden.as_str()]));
    let final_binding = &result.bindings[&target("consumer")];
    let (
        GenerationInputs::Extension {
            samples: before,
            support: before_support,
            ..
        },
        GenerationInputs::Extension {
            samples: after,
            support: after_support,
            ..
        },
    ) = (&original.inputs, &final_binding.inputs)
    else {
        panic!()
    };
    assert_eq!(
        before, after,
        "hidden provider is absent from all nine samples"
    );
    assert_ne!(
        before_support, after_support,
        "full intervening support must still change"
    );

    let after = apply(
        &document,
        Command::WithBoundaryReplacements {
            edit: BoundaryReplacementEdit::new(
                Command::Rename {
                    node: id("lead"),
                    label: "Changed label".into(),
                },
                result.entries,
            )
            .unwrap(),
        },
        "replaced",
    );
    assert_eq!(
        *final_binding,
        capture(&connection, &after, "consumer", sparse_capture()),
        "birth descriptor must equal a fresh canonical capture of the actual final providers"
    );
}

#[test]
fn temporal_equation_has_more_than_k_plus_two_distinct_provider_dependencies() {
    let (document, providers) = wide_document(false);
    let connection = connection();
    let plan = RenderPlan::compile(&document).unwrap();
    let context = plan
        .scoped_hold_context(
            &deadpan_plan::ScopedHoldContextRequest {
                target: target("consumer"),
                direction: ExtensionDirection::FromLeft,
                native_rate: FrameRate::new(1, 1).unwrap(),
                frame_count: 9,
            },
            BoundaryQueryLimits::default(),
        )
        .unwrap();
    let by_node: BTreeMap<_, _> = providers
        .iter()
        .enumerate()
        .map(|(index, name)| (id(name), index))
        .collect();
    let required = capture(&connection, &document, "consumer", sparse_capture());
    let observation = observation::Observation::context(
        &plan,
        &document,
        &context,
        &required.settings(),
        &by_node,
        &QualifiedGenerationPictures::new(&connection),
    )
    .unwrap();
    let dependencies: BTreeSet<_> = observation
        .terms(&required)
        .iter()
        .filter_map(|term| match term {
            decisions::MismatchTerm::IfReplaced(index)
            | decisions::MismatchTerm::IfRetained(index) => Some(*index),
            _ => None,
        })
        .collect();
    assert!(
        dependencies.len() > 9 + 2,
        "sparse samples cannot bound the full dependency degree"
    );
}

#[test]
fn wide_temporal_chain_propagates_in_both_canonical_node_orders() {
    for reverse in [false, true] {
        let (document, providers) = wide_document(reverse);
        let connection = connection();
        for (index, name) in providers.iter().enumerate() {
            let mut required = capture(&connection, &document, name, GenerationCaptureSpec::Bridge);
            if index == 0 {
                required.canvas[0] += 1;
            }
            save_origin(&connection, &document, name, required);
        }
        save_origin(
            &connection,
            &document,
            "consumer",
            capture(&connection, &document, "consumer", sparse_capture()),
        );
        let result = derive(&connection, document, BTreeSet::new(), BTreeSet::new());
        assert_eq!(result.entries.len(), providers.len() + 1);
        assert!(
            result
                .entries
                .windows(2)
                .all(|pair| pair[0].target < pair[1].target)
        );
        assert!(
            result
                .births
                .iter()
                .all(|birth| birth.cause == IntentCause::SourceBoundaryChanged)
        );
        let GenerationInputs::Extension {
            samples,
            support,
            terminal,
            ..
        } = &result.bindings[&target("consumer")].inputs
        else {
            panic!()
        };
        assert!(
            samples
                .iter()
                .all(|sample| sample.picture == GenerationPictureIdentity::AuthoredBlack)
        );
        assert!(support.iter().all(
            |span| span.first == GenerationPictureIdentity::AuthoredBlack
                && span.last == GenerationPictureIdentity::AuthoredBlack
        ));
        assert_eq!(terminal.picture, GenerationPictureIdentity::AuthoredBlack);
    }
}

#[test]
fn wide_temporal_cycle_uses_one_conservative_group_independent_of_node_order() {
    for reverse in [false, true] {
        let (document, providers) = wide_document(reverse);
        let connection = connection();
        for name in &providers {
            save_origin(
                &connection,
                &document,
                name,
                capture(&connection, &document, name, GenerationCaptureSpec::Bridge),
            );
        }
        let mut required = capture(&connection, &document, "consumer", sparse_capture());
        let GenerationInputs::Extension {
            samples,
            opposite,
            support,
            terminal,
            ..
        } = &mut required.inputs
        else {
            panic!()
        };
        for sample in samples.iter_mut().chain(opposite) {
            sample.picture = GenerationPictureIdentity::AuthoredBlack;
        }
        for span in support {
            span.first = GenerationPictureIdentity::AuthoredBlack;
            span.last = GenerationPictureIdentity::AuthoredBlack;
        }
        terminal.picture = GenerationPictureIdentity::AuthoredBlack;
        save_origin(&connection, &document, "consumer", required);
        let result = derive(&connection, document, BTreeSet::new(), BTreeSet::new());
        let mut members: Vec<_> = providers
            .iter()
            .map(|name| target(name))
            .chain([target("consumer")])
            .collect();
        members.sort();
        let expected = IntentCause::cyclic_group(&members).unwrap();
        assert_eq!(result.entries.len(), members.len());
        assert!(result.births.iter().all(|birth| birth.cause == expected));
    }
}

#[test]
fn extension_shortening_preserves_the_opposite_picture_in_both_directions() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let document = fixture(&["lead", "a", "tail"], &["a"]);
        let connection = connection();
        let capture_spec = extension(direction, FrameRate::new(24, 1).unwrap());
        let prior = capture(&connection, &document, "a", capture_spec);
        save_origin(&connection, &document, "a", prior.clone());
        let shortened = apply(
            &document,
            Command::SetHoldDuration {
                node: id("a"),
                duration: frames(3),
            },
            "shortened",
        );
        let result = derive(&connection, shortened, BTreeSet::new(), BTreeSet::new());
        assert!(result.entries.is_empty());
        let current = &result.bindings[&target("a")];
        assert_ne!(
            *current, prior,
            "duration and opposite relative offset are updated"
        );
        assert!(observation::matches_retained(current, &prior));
        let mut changed_picture = current.clone();
        let GenerationInputs::Extension {
            opposite: Some(opposite),
            ..
        } = &mut changed_picture.inputs
        else {
            panic!()
        };
        opposite.picture = generated(&document, "a", 0).unwrap();
        assert!(!observation::matches_retained(&changed_picture, &prior));
        let mut moved_opposite = current.clone();
        let GenerationInputs::Extension {
            opposite: Some(opposite),
            ..
        } = &mut moved_opposite.inputs
        else {
            panic!()
        };
        opposite.position = opposite
            .position
            .checked_add(ExactRatio::integer(1))
            .unwrap();
        assert!(!observation::matches_retained(&moved_opposite, &prior));
    }
}

#[test]
fn live_temporal_intent_uses_its_persisted_settings_and_all_final_support() {
    let (mut document, providers) = wide_document(false);
    document = apply(
        &document,
        Command::RevertGeneratedHold {
            node: id("consumer"),
        },
        "pending",
    );
    let connection = connection();
    let hidden = hidden_provider(&document);
    save_origin(&connection, &document, &hidden, binding(None, None));
    let exclusions = providers
        .iter()
        .filter(|name| **name != hidden)
        .map(|name| target(name))
        .collect();
    let result = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(document)).unwrap(),
        &exclusions,
        &BTreeMap::from([(
            id("consumer"),
            GenerationInputSettings {
                capture: sparse_capture(),
                region: None,
            }
            .into(),
        )]),
        None,
    )
    .unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].target, target(&hidden));
    assert_eq!(
        result.bindings[&target("consumer")].capture_spec(),
        sparse_capture()
    );
    let GenerationInputs::Extension { support, .. } = &result.bindings[&target("consumer")].inputs
    else {
        panic!()
    };
    assert!(
        support
            .iter()
            .any(|span| span.first == GenerationPictureIdentity::AuthoredBlack)
    );
}

#[test]
fn unavailable_temporal_context_restores_fallback_without_store_writes() {
    let document = fixture(&["a", "tail"], &["a"]);
    let connection = connection();
    let mut required = binding(None, black());
    // Explicit test-only descriptor at an unavailable edge; no valid origin
    // admission is asserted. It exercises fail-closed transition behavior.
    required.inputs = GenerationInputs::Extension {
        capture: sparse_capture(),
        samples: vec![],
        opposite: None,
        support: vec![],
        terminal: crate::generation_inputs::RelativeGenerationPicture {
            position: ExactRatio::integer(0),
            picture: GenerationPictureIdentity::AuthoredBlack,
        },
    };
    save_origin(&connection, &document, "a", required);
    let before = connection.total_changes();
    let result = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(document)).unwrap(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].target, target("a"));
    assert!(result.bindings.is_empty());
    assert_eq!(result.unavailable[&target("a")].capture, sparse_capture());
    assert!(result.births[0].input_binding.is_none());
    assert_eq!(result.births[0].duration, frames(12));
    assert_eq!(connection.total_changes(), before);
}

#[test]
fn missing_context_does_not_discard_other_batch_results_and_live_context_can_return() {
    let document = fixture(&["a", "lead", "b", "tail"], &["a", "b"]);
    let connection = connection();
    let capture_spec = extension(ExtensionDirection::FromLeft, FrameRate::new(24, 1).unwrap());
    let required = capture(&connection, &document, "b", capture_spec);
    save_origin(&connection, &document, "b", required.clone());
    // A's geometric lack of an anchor must not drop B's valid observation.
    save_origin(&connection, &document, "a", required);
    let result = derive(
        &connection,
        document.clone(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].target, target("a"));
    assert!(result.unavailable.contains_key(&target("a")));
    assert!(result.bindings.contains_key(&target("b")));
    let fallback = apply(
        &document,
        Command::RevertGeneratedHold { node: id("a") },
        "fallback",
    );
    let settings = BTreeMap::from([(
        id("a"),
        GenerationInputSettings {
            capture: capture_spec,
            region: None,
        }
        .into(),
    )]);
    let absent = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(fallback.clone())).unwrap(),
        &BTreeSet::from([target("b")]),
        &settings,
        None,
    )
    .unwrap();
    assert!(absent.unavailable.contains_key(&target("a")));
    let restored = apply(
        &fallback,
        Command::Move {
            node: id("lead"),
            parent: id("root"),
            index: 0,
        },
        "restored",
    );
    let available = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(restored.clone())).unwrap(),
        &BTreeSet::from([target("b")]),
        &settings,
        None,
    )
    .unwrap();
    assert!(available.unavailable.is_empty());
    assert_eq!(
        available.bindings[&target("a")],
        capture(&connection, &restored, "a", capture_spec)
    );
}

#[test]
fn removing_a_selected_region_restores_fallback_and_retains_the_missing_selection() {
    let document = fixture(&["lead", "a", "tail"], &["a"]);
    let region = TargetId::new("selected-region").unwrap();
    let document = apply(
        &document,
        Command::SetTarget {
            id: region.clone(),
            target: AttentionTarget {
                label: "Selected region".into(),
                asset: accepted(&document, "a").artifact.sampled_asset.clone(),
                span: span(),
                region: TargetRegion {
                    center: [500_000, 500_000],
                    size: [250_000, 250_000],
                },
                samples: vec![],
                corrections: vec![],
                provenance: None,
            },
        },
        "region",
    );
    let connection = connection();
    let required = capture(&connection, &document, "a", GenerationCaptureSpec::Bridge)
        .with_region(&document, Some(&region))
        .unwrap();
    save_origin(&connection, &document, "a", required);
    let removed = apply(
        &document,
        Command::DeleteTarget { id: region.clone() },
        "removed-region",
    );
    let result = derive(&connection, removed, BTreeSet::new(), BTreeSet::new());
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].target, target("a"));
    let retained = result.bindings[&target("a")].region.as_ref().unwrap();
    assert_eq!(retained.id, region);
    assert!(retained.record.is_none());
    assert_eq!(
        result.births[0].input_binding.as_ref().unwrap().region,
        result.bindings[&target("a")].region
    );
}

#[test]
fn aggregate_observation_bounds_are_charged_before_measurement() {
    let metadata = MetadataBudget::new(2 * MAX_INPUT_BINDING_BYTES);
    let mut work = BoundaryQueryLimits {
        max_scopes: 41,
        max_comparisons: 41,
    };
    let mut atoms = 5;
    let mut spans = 1;
    reserve_observation(&metadata, &mut work, &mut atoms, &mut spans, 4, 1).unwrap();
    assert_eq!(
        (atoms, spans, work.max_scopes, work.max_comparisons),
        (0, 0, 0, 0)
    );
    assert!(reserve_observation(&metadata, &mut work, &mut atoms, &mut spans, 0, 0).is_err());
    let mut work = BoundaryQueryLimits::default();
    let mut atoms = MAX_OBSERVATION_ATOMS;
    let mut spans = MAX_OBSERVATION_SPANS;
    assert!(
        reserve_observation(
            &MetadataBudget::new(2 * MAX_INPUT_BINDING_BYTES - 1),
            &mut work,
            &mut atoms,
            &mut spans,
            1,
            0
        )
        .is_err()
    );
    assert_eq!(
        (atoms, spans),
        (MAX_OBSERVATION_ATOMS, MAX_OBSERVATION_SPANS)
    );
}
