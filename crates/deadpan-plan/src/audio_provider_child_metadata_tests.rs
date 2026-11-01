//! Checked public Source/Projected operands with nonempty nested metadata.
use super::*;
use crate::{AudioRootPlacement, AudioSignalTape, AudioSignalTapeRun};
use serde_json::json;

fn repeated_preserve_plan() -> (RenderPlan, InstancePath, AudioSourceVoiceRecipe) {
    let frozen = frozen();
    let mut wire = frozen["cases"][1]["document_wire"].clone();
    wire["nodes"]["root"]["kind"]["children"] = json!(["repeat"]);
    wire["nodes"]["repeat"] = json!({
        "label":"Repeat",
        "kind":{"type":"repeat","child":"stage",
            "iterations":{"runs":[{"allocation":"plays","first":0,"count":2}]},
            "gap":{"duration":1,"video":{"type":"background"},"audio":{"type":"silence"}},
            "escalation":null}
    });
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&document).unwrap();
    let instance: InstancePath = serde_json::from_value(json!({
        "node":"owner",
        "repeats":[{"node":"repeat","iteration":{"allocation":"plays","ordinal":0}}]
    }))
    .unwrap();
    let recipe = recipe_from(&frozen["cases"][1]["observations"]["attempted_recipe"]);
    (plan, instance, recipe)
}

fn checked_signal_rows<'plan>(
    plan: &'plan RenderPlan,
    instance: InstancePath,
    recipe: AudioSourceVoiceRecipe,
) -> Vec<(String, AudioSignal<'plan>, Value)> {
    let occurrence = plan
        .source_voice_occurrence(instance, recipe.clone(), Default::default())
        .unwrap();
    let projection = Arc::clone(
        occurrence
            .processing_projection()
            .expect("Preserve stage is public"),
    );
    let mut rows = Vec::new();
    if let Some(input) = projection.input_tape() {
        input.provider_before_visit(|index, signal, samples| {
            rows.push((
                format!("source_input_{index}"),
                signal.clone(),
                json!({"samples":samples}),
            ));
        });
    }
    projection
        .output_policy()
        .provider_before_visit(|index, signal, samples| {
            rows.push((
                format!("projected_policy_{index}"),
                signal.clone(),
                json!({"samples":samples}),
            ));
        });
    let full = ExactRatio::ZERO..ExactRatio::integer(projection.output_frames());
    let tape = AudioSignalTape::new(
        plan,
        full.clone(),
        vec![AudioSignalTapeRun::intrinsic(
            full.clone(),
            full,
            Arc::clone(&projection),
        )],
    )
    .unwrap();
    tape.provider_before_visit(|index, signal, samples| {
        rows.push((
            format!("intrinsic_projected_{index}"),
            signal.clone(),
            json!({"samples":samples}),
        ));
    });
    let source_voice = projection
        .stage()
        .output_signal()
        .source_voice(recipe)
        .unwrap();
    rows.push((
        "repeated_stage_source_input".into(),
        source_voice.input_signal(),
        Value::Null,
    ));
    rows.push((
        "repeated_stage_source_output".into(),
        source_voice.output_signal(),
        Value::Null,
    ));
    rows
}

fn full_samples(signal: &AudioSignal<'_>) -> Range<SignalSample> {
    let grid = signal
        .transform()
        .unwrap()
        .grid(AudioBoundaryRule::PointCeil)
        .unwrap();
    let support = signal.support();
    grid.boundary(support.start).unwrap()..grid.boundary(support.end).unwrap()
}

fn compare_public_child(plan: &RenderPlan, name: &str, signal: &AudioSignal<'_>) {
    let before = signal.provider_before_snapshot(plan);
    let samples = full_samples(signal);
    let limits = AudioQueryLimits::default();
    let ordinary = signal.query_inner(
        samples.clone(),
        limits,
        AudioBoundaryRule::PointCeil,
        false,
        false,
    );
    let expected = query_result(plan, &ordinary);
    assert_eq!(expected["result"], "Ok", "{name}: {expected}");
    assert!(
        !expected["query"]["spans"].as_array().unwrap().is_empty(),
        "{name}: the checked query must construct actual span metadata"
    );
    drop(ordinary);
    let mut backend = Backend::new(Fault::None);
    let observer = backend.observer();
    let (actual, receipt) = run_current(
        signal,
        samples.clone(),
        limits,
        AudioBoundaryRule::PointCeil,
        (false, false),
        &mut backend,
    );
    let actual_body = current_snapshot(plan, &actual);
    assert_values(&expected, &actual_body, &["span_capacity"]);
    if actual.is_ok() {
        assert_success_work(&expected, &actual_body, &receipt, &observer);
        assert!(
            observer.live() > 0,
            "{name}: returned child retains its real guard"
        );
    }
    drop(backend);
    if actual.is_ok() {
        assert!(
            observer.live() > 0,
            "{name}: output remains guarded after backend drop"
        );
    }
    drop(actual);
    observer.assert_released();
    for endpoints in [false, true] {
        let ordinary = signal.policy_on_grid(
            samples.clone(),
            limits,
            AudioBoundaryRule::PointCeil,
            endpoints,
        );
        let expected = policy_result(&ordinary);
        drop(ordinary);
        let mut backend = Backend::new(Fault::None);
        let observer = backend.observer();
        let (actual, receipt) = run_policy(
            signal,
            samples.clone(),
            limits,
            AudioBoundaryRule::PointCeil,
            endpoints,
            &mut backend,
        );
        let actual_body = admitted_policy_result(&actual);
        assert_values(
            &expected,
            &actual_body,
            &["contents_capacity", "suppressed_capacity"],
        );
        if actual.is_ok() {
            assert_success_work(&expected, &actual_body, &receipt, &observer);
        }
        drop(actual);
        observer.assert_released();
    }
    assert_eq!(signal.provider_before_snapshot(plan), before);
}

#[test]
fn checked_repeat_source_and_intrinsic_projected_metadata_cross_the_admitted_child() {
    let (plan, instance, recipe) = repeated_preserve_plan();
    let occurrence = plan
        .source_voice_occurrence(instance.clone(), recipe.clone(), Default::default())
        .unwrap();
    let projected = occurrence
        .processing_projection()
        .expect("checked Preserve occurrence");
    let descriptor = projected.provider_before_snapshot(&plan);
    assert!(
        !descriptor["stage"]["descriptor"]["instance"]["repeats"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the checked Preserve descriptor must carry its concrete Repeat scope"
    );
    let rows = checked_signal_rows(&plan, instance, recipe);
    assert!(!rows.is_empty());
    let mut witnesses = (0, 0, 0);
    for (name, signal, _) in rows {
        let snapshot = signal.provider_before_snapshot(&plan);
        if name.starts_with("repeated_stage_source_") {
            assert!(
                !snapshot["repeats"].as_array().unwrap().is_empty(),
                "{name}: AudioStage::output_signal carries the actual Repeat scope"
            );
            witnesses.0 += 1;
        }
        if snapshot["provider"]["tag"] == "Silence" {
            assert!(!snapshot["repeats"].as_array().unwrap().is_empty());
            witnesses.1 += 1;
        }
        if snapshot["provider"]["tag"] == "Projected" {
            assert!(!snapshot["repeats"].as_array().unwrap().is_empty());
            witnesses.2 += 1;
        }
        compare_public_child(&plan, &name, &signal);
    }
    assert_eq!(witnesses.0, 2);
    assert!(witnesses.1 > 0 && witnesses.2 > 0);
}

#[test]
fn checked_repeat_gap_definition_preserves_definition_and_gap_metadata() {
    let (plan, _, recipe) = repeated_preserve_plan();
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        })
        .unwrap();
    let point = definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::integer(1),
                ExactRatio::ZERO..ExactRatio::integer(1),
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap();
    let signal = point.signal().source_voice(recipe).unwrap().output_signal();
    let before = signal.provider_before_snapshot(&plan);
    assert_eq!(before["definition"]["type"], "repeat_gap");
    assert!(
        !before["gap"].is_null(),
        "RepeatGap's actual Hold remains attached"
    );
    compare_public_child(&plan, "repeat_gap_source_output", &signal);
}

#[test]
fn checked_nested_metadata_children_release_each_real_callback_failure_prefix() {
    let (plan, instance, recipe) = repeated_preserve_plan();
    let mut children = checked_signal_rows(&plan, instance, recipe.clone());
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        })
        .unwrap();
    let point = definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::integer(1),
                ExactRatio::ZERO..ExactRatio::integer(1),
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap();
    children.push((
        "repeat_gap_source_output".into(),
        point.signal().source_voice(recipe).unwrap().output_signal(),
        Value::Null,
    ));
    for (name, signal, _) in children {
        let samples = full_samples(&signal);
        let limits = AudioQueryLimits::default();
        let mut baseline = Backend::new(Fault::None);
        let baseline_observer = baseline.observer();
        let (value, _) = run_current(
            &signal,
            samples.clone(),
            limits,
            AudioBoundaryRule::PointCeil,
            (false, false),
            &mut baseline,
        );
        assert!(
            value.is_ok(),
            "{name}: baseline checked query should complete"
        );
        let reservations = baseline_observer.reservations();
        let spends = baseline_observer.spends();
        drop(value);
        baseline_observer.assert_released();
        for index in 1..=reservations {
            let mut backend = Backend::new(Fault::RefuseReserve(index));
            let observer = backend.observer();
            let (result, _) = run_current(
                &signal,
                samples.clone(),
                limits,
                AudioBoundaryRule::PointCeil,
                (false, false),
                &mut backend,
            );
            assert!(
                matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
                ),
                "{name} reserve {index}"
            );
            drop(result);
            observer.assert_released();
        }
        for index in 1..=spends {
            let mut backend = Backend::new(Fault::RefuseSpend(index));
            let observer = backend.observer();
            let (result, _) = run_current(
                &signal,
                samples.clone(),
                limits,
                AudioBoundaryRule::PointCeil,
                (false, false),
                &mut backend,
            );
            assert!(
                matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide work refusal"))
                ),
                "{name} spend {index}"
            );
            drop(result);
            observer.assert_released();
        }
    }
}

#[test]
fn public_owned_projection_reaches_projected_definition_metadata() {
    let mut exercised = false;
    visit_fixtures(|plan, name, _signal, _old| {
        if name != "owned_input_via_projection" {
            return;
        }
        let projection = plan
            .owned_preserve_projection(AudioDefinitionSelector::Node { node: id("stage") })
            .unwrap();
        let descriptor = projection.provider_before_snapshot(plan);
        assert!(!descriptor["stage"]["descriptor"]["definition"].is_null());
        let full = ExactRatio::ZERO..ExactRatio::integer(projection.output_frames());
        let tape = AudioSignalTape::new(
            plan,
            full.clone(),
            vec![AudioSignalTapeRun::intrinsic(
                full.clone(),
                full,
                Arc::clone(&projection),
            )],
        )
        .unwrap();
        let mut children = Vec::new();
        tape.provider_before_visit(|index, signal, samples| {
            let before = signal.provider_before_snapshot(plan);
            assert_eq!(before["provider"]["tag"], "Projected");
            assert!(
                !before["definition"].is_null(),
                "Projected child keeps its definition"
            );
            children.push((format!("owned_projected_{index}"), signal.clone(), samples));
        });
        assert!(!children.is_empty());
        for (child, signal, _) in children {
            compare_public_child(plan, &child, &signal);
        }
        exercised = true;
    });
    assert!(
        exercised,
        "the frozen checked public projection fixture is required"
    );
}

#[test]
fn owned_source_output_rejects_its_carrier_after_current_inventory() {
    let recipe = recipe_from(&frozen()["cases"][0]["observations"]["attempted_recipe"]);
    let mut exercised = false;
    visit_fixtures(|plan, name, _signal, _old| {
        if name != "owned_input_via_projection" {
            return;
        }
        let projection = plan
            .owned_preserve_projection(AudioDefinitionSelector::Node { node: id("stage") })
            .unwrap();
        let voice = projection
            .stage()
            .output_signal()
            .source_voice(recipe.clone())
            .unwrap();
        let input = voice.input_signal();
        let output = voice.output_signal();
        let samples = full_samples(&input);
        let limits = AudioQueryLimits::default();
        let mut backend = Backend::new(Fault::None);
        let observer = backend.observer();
        let (current, current_usage) = run_policy(
            &input,
            samples.clone(),
            limits,
            AudioBoundaryRule::PointCeil,
            true,
            &mut backend,
        );
        let current = current.unwrap();
        assert!(!current.view().contents.is_empty());
        let current_forwarded = observer.forwarded();
        drop(current);
        observer.assert_released();

        let ordinary =
            output.policy_on_grid(samples.clone(), limits, AudioBoundaryRule::PointCeil, true);
        assert!(matches!(&ordinary, Err(PlanError::InvalidPlan(_))));
        let expected = policy_result(&ordinary);
        drop(ordinary);
        let mut backend = Backend::new(Fault::None);
        let observer = backend.observer();
        let (actual, usage) = run_policy(
            &output,
            samples,
            limits,
            AudioBoundaryRule::PointCeil,
            true,
            &mut backend,
        );
        assert_values(&expected, &admitted_policy_result(&actual), &[]);
        assert!(
            usage["attempted_canonical"].as_u64().unwrap()
                > current_usage["attempted_canonical"].as_u64().unwrap()
        );
        assert!(observer.forwarded() > current_forwarded);
        drop(actual);
        observer.assert_released();
        exercised = true;
    });
    assert!(exercised);
}

#[path = "audio_signal_policy_before.rs"]
mod signal_policy_before;
