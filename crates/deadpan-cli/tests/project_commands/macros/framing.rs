//! Saved framing intent resolves on the invocation's qualified picture and range.

use std::num::NonZeroU32;

use deadpan_core::{
    AssetId, ExactRatio, FramingPose, FramingValue, PauseLength, PitchPolicy, ProjectFrame,
    SemanticInstruction as I, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};
use deadpan_plan::RenderPlan;

use super::*;

#[path = "framing/scoped.rs"]
mod scoped;

#[path = "framing/scoped_duration.rs"]
mod scoped_duration;

#[path = "framing/follow_creep.rs"]
mod follow_creep;

#[path = "framing/follow_retune.rs"]
mod follow_retune;

#[path = "framing/follow_creep_admission.rs"]
mod follow_creep_admission;

#[path = "framing/ranged_follow_creep.rs"]
mod ranged_follow_creep;

#[path = "framing/hold_follow_creep.rs"]
mod hold_follow_creep;

#[path = "framing/sequence_follow_creep.rs"]
mod sequence_follow_creep;

#[path = "framing/repeat_follow_creep.rs"]
mod repeat_follow_creep;

#[path = "framing/retime_follow_creep.rs"]
mod retime_follow_creep;

#[path = "framing/scoped_follow_creep.rs"]
mod scoped_follow_creep;

/// Verify physical isolation independently of value/picture comparisons. The
/// command comes from durable history, so this follows the identities actually
/// committed, rather than the separate random IDs allocated by dry-run.
fn assert_scoped_follow_commit(
    package: &Path,
    before: &ProjectDocument,
    result: &Value,
) -> Result<(
    ProjectDocument,
    deadpan_core::SemanticScopedContext,
    Vec<i64>,
)> {
    let after = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    let mapped: deadpan_core::SemanticScopedContext =
        serde_json::from_value(result["context"]["scoped"].clone())?;
    assert!(
        mapped
            .target
            .matches_instance(&after, &mapped.presentation)?
    );
    let raw: String = rusqlite::Connection::open(package.join("project.sqlite"))?.query_row(
        "SELECT request FROM history WHERE revision_id=?1",
        [after.revision_id().as_str()],
        |row| row.get(0),
    )?;
    let request: deadpan_core::CommandRequest = serde_json::from_str(&raw)?;
    let Command::Compound { transaction } = &request.command else {
        panic!("scoped semantic Compound")
    };
    let baseline = RenderPlan::compile(before)?;
    let mut identities = BTreeMap::new();
    for frame in 0..baseline.duration().frames() {
        let sample = baseline.picture(ProjectFrame(frame))?;
        identities.insert(sample.instance.clone(), sample.instance);
        for layer in sample.framing {
            identities.insert(layer.instance.clone(), layer.instance);
        }
    }
    let mut inverse_nodes = BTreeMap::<String, String>::new();
    let mut staged = before.clone();
    for step in transaction.steps() {
        let leaf = step.edit().unwrap();
        let Command::EditScoped {
            target,
            edit,
            identities: allocated,
        } = leaf.command.as_command()
        else {
            panic!("scoped framing leaf")
        };
        let requirements = staged.scoped_edit_requirements(target, edit)?;
        assert_eq!(allocated.nodes.len(), requirements.nodes);
        assert_eq!(allocated.marks.len(), requirements.marks);
        let prepared = deadpan_core::prepare_scoped_edit(&staged, &leaf.request(&staged))?;
        let mut used = std::collections::BTreeSet::new();
        for isolation in prepared.isolation().steps() {
            assert!(
                isolation
                    .before_prefix()
                    .iter()
                    .all(|step| matches!(step.branch, deadpan_core::RepeatEditBranch::Play { .. }))
            );
            for (old, new) in isolation.nodes() {
                assert!(allocated.nodes.contains(new));
                assert!(!staged.nodes().contains_key(new));
                assert!(used.insert(new.clone()));
                let origin = inverse_nodes
                    .get(old.as_str())
                    .cloned()
                    .unwrap_or_else(|| old.as_str().to_owned());
                inverse_nodes.insert(new.as_str().to_owned(), origin);
            }
        }
        assert_eq!(used.len(), allocated.nodes.len());
        for instance in identities.values_mut() {
            *instance = prepared.map_instance(&staged, instance)?;
        }
        staged = prepared.document;
    }
    // Leaf preparation ends on its private step revision. The aggregate below
    // restores the caller's outer revision and must match it exactly.
    same_document(&staged, &after)?;
    let replay =
        deadpan_core::replay_compound::<deadpan_core::EditError>(before, &request, |_| Ok(()))?;
    assert_eq!(replay.document, after);
    assert_eq!(replay.edit.inverse.apply(&after)?, *before);
    fn canonical(value: &mut Value, ids: &BTreeMap<String, String>) {
        match value {
            Value::String(id) => {
                if let Some(original) = ids.get(id) {
                    *id = original.clone();
                }
            }
            Value::Array(values) => {
                for value in values {
                    canonical(value, ids);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    canonical(value, ids);
                }
            }
            _ => (),
        }
    }
    for (fresh, original) in &inverse_nodes {
        let mut actual = serde_json::to_value(&after.nodes()[&node(fresh)])?;
        canonical(&mut actual, &inverse_nodes);
        let mut expected = serde_json::to_value(&before.nodes()[&node(original)])?;
        if fresh == mapped.target.node.as_str() {
            expected["framing"] = actual["framing"].clone();
        }
        assert_eq!(actual, expected, "clone {fresh} from {original}");
    }
    for (id, old) in before.nodes() {
        let mut expected = old.clone();
        if id == &mapped.target.node {
            expected.framing = after.nodes()[id].framing.clone();
        }
        assert_eq!(after.nodes()[id], expected, "retained template {id}");
    }
    assert_eq!(after.assets(), before.assets());
    assert_eq!(after.targets(), before.targets());
    assert_eq!(after.duration()?, before.duration()?);
    let edited = RenderPlan::compile(&after)?;
    let mut changed_frames = vec![];
    for frame in 0..baseline.duration().frames() {
        let a = baseline.picture(ProjectFrame(frame))?;
        let b = edited.picture(ProjectFrame(frame))?;
        assert_eq!(
            &b.instance, &identities[&a.instance],
            "physical provider {frame}"
        );
        assert_eq!(a.picture, b.picture);
        assert_eq!(a.picture_context, b.picture_context);
        assert_eq!(a.local_position, b.local_position);
        assert_eq!(a.gap_after, b.gap_after);
        let mut captions = serde_json::to_value(&b.captions)?;
        canonical(&mut captions, &inverse_nodes);
        assert_eq!(captions, serde_json::to_value(&a.captions)?);
        assert_eq!(a.framing.len(), b.framing.len());
        let selected = b
            .framing
            .iter()
            .position(|layer| layer.instance == mapped.presentation && !layer.escalation);
        for (index, (old, actual)) in a.framing.iter().zip(&b.framing).enumerate() {
            assert_eq!(
                &actual.instance, &identities[&old.instance],
                "physical layer {frame}"
            );
            let mut expected = old.clone();
            expected.instance = actual.instance.clone();
            if actual.instance == mapped.presentation && !actual.escalation {
                expected.pose = actual.pose;
                changed_frames.push(frame);
            } else if selected.is_some_and(|selected| index > selected)
                && !actual.escalation
                && let Some(framing) = &after.nodes()[&actual.instance.node].framing
                && matches!(framing.value, FramingValue::Follow { .. })
            {
                // An ancestor Follow remains authored identically, but must
                // reproject its subject through the edited inner view. Even
                // same-target centering can shift one unit after Q32 rounding.
                let value = framing
                    .evaluate_with_follow_exact(actual.local_position, actual.effective_duration)?;
                let followed = value.follow.unwrap();
                let target = followed.target;
                let live_scale = followed.scale;
                let mut pose = value.pose;
                if let Some((asset, point)) = b.picture.follow_point()
                    && &after.targets()[target].asset == asset
                    && let Some((region, _)) = after.targets()[target].region_at(point)
                {
                    assert!(
                        b.picture_context.is_none(),
                        "ancestor oracle fixture has no captured canvas"
                    );
                    let mut center = region.center_ratio();
                    if let Some(aspect) = after.targets()[target].display_aspect {
                        center = deadpan_core::fit_source_point(
                            center,
                            aspect,
                            [
                                after.presentation_basis().width,
                                after.presentation_basis().height,
                            ],
                        )
                        .unwrap();
                    }
                    let half = ratio(1, 2);
                    let mut visible = true;
                    for inner in &b.framing[..index] {
                        if let Some(inner) = inner.pose {
                            center = [
                                center[0]
                                    .checked_sub(inner.center_x)?
                                    .checked_mul(inner.scale)?
                                    .checked_add(half)?,
                                center[1]
                                    .checked_sub(inner.center_y)?
                                    .checked_mul(inner.scale)?
                                    .checked_add(half)?,
                            ];
                        }
                        visible &= center.iter().all(|x| {
                            !x.compare(ExactRatio::ZERO).is_lt()
                                && !x.compare(ExactRatio::ONE).is_gt()
                        });
                    }
                    if visible {
                        let quantize = |x: ExactRatio| -> std::result::Result<
                            ExactRatio,
                            deadpan_core::TimeError,
                        > {
                            let grid = 1_i128 << 32;
                            ExactRatio::new(x.checked_mul(ratio(grid, 1))?.round_even()?, grid)
                        };
                        pose = FramingPose::new(
                            quantize(center[0])?,
                            quantize(center[1])?,
                            quantize(live_scale)?,
                        )?;
                    }
                }
                expected.pose = Some(pose);
            }
            assert_eq!(actual, &expected, "scope semantics {frame}");
        }
    }
    let end = baseline.audio_duration()?;
    assert_eq!(end, edited.audio_duration()?);
    let expected = baseline.audio(deadpan_core::AudioSample(0)..end, Default::default())?;
    let actual = edited.audio(deadpan_core::AudioSample(0)..end, Default::default())?;
    for span in &actual.spans {
        assert!(
            identities
                .values()
                .any(|instance| instance == &span.instance)
        );
    }
    let mut actual = serde_json::to_value(actual.spans)?;
    canonical(&mut actual, &inverse_nodes);
    assert_eq!(actual, serde_json::to_value(expected.spans)?);
    Ok((after, mapped, changed_frames))
}

#[cfg(all(target_os = "macos", feature = "synthetic-worker"))]
#[allow(dead_code)]
#[path = "../../preview_export/recipes.rs"]
mod recipes;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn count(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}

/// The retained CFR fixture is 120 measured pictures at 30000/1001. Setup
/// registers the real video through the public CLI; no fabricated index or
/// unqualified AssetRecord can satisfy the framing resolver.
fn qualified_project(root: &Path) -> Result<PathBuf> {
    let package = create(root)?;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?;
    let retained = success(&[
        "project",
        "retain-original",
        package.to_str().unwrap(),
        source.to_str().unwrap(),
    ])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let input = root.join("qualified-source.json");
    fs::write(
        &input,
        json!({"protocol":1,"registration":{
        "expected_revision":before.revision_id(),"new_revision":"qualified",
        "original":retained["retained_original"]["record"]["object"]["content"],
        "new_asset_id":"camera","label":"Qualified CFR Original",
        "insertion":{"parent":before.root(),"index":0,"node":"camera-source","label":"Original"}
    },"streams":{"type":"video_only"}})
        .to_string(),
    )?;
    success(&[
        "project",
        "register-source",
        package.to_str().unwrap(),
        "--request-json",
        input.to_str().unwrap(),
    ])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    assert_eq!(document.duration()?.frames(), 120);
    assert_eq!(
        store
            .source_video_index(document.revision_id(), &AssetId::new("camera")?)?
            .frames()
            .len(),
        120
    );
    assert!(
        document.assets()[&AssetId::new("camera")?]
            .source_qualification
            .is_some()
    );
    Ok(package)
}

fn command(
    package: &Path,
    input: &Path,
    command: Command,
    revision: &str,
) -> Result<ProjectDocument> {
    let before = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(
        input,
        json!({"protocol":1,"project_id":before.project_id(),
        "expected_revision":before.revision_id(),"new_revision":revision,"command":command})
        .to_string(),
    )?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    Ok(ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?)
}

fn zoom(kind: ZoomKind, center: SemanticZoomCenter) -> I {
    I::Zoom { kind, center }
}

fn invocation(
    document: &ProjectDocument,
    version: u64,
    register: &str,
    owner: &str,
    cursor: i64,
    selection: Option<(i64, i64, bool)>,
    revision: &str,
) -> Value {
    let mut request = run(document, version, register, cursor);
    request["operation"]["selected_child"] = json!(owner);
    request["operation"]["new_revision"] = json!(revision);
    request["operation"]["visual_selection"] = selection.map_or(Value::Null, |(anchor, head, extending)| {
        json!({"type":"time","anchor":anchor,"head":head,"extending":extending})
    });
    request
}

fn pose(document: &ProjectDocument, owner: &str, frame: i64) -> FramingPose {
    RenderPlan::compile(document)
        .unwrap()
        .picture(ProjectFrame(frame))
        .unwrap()
        .framing
        .into_iter()
        .find(|layer| {
            layer.instance.node == node(owner)
                && !layer.escalation
                && layer.instance.repeats.is_empty()
        })
        .unwrap()
        .pose
        .unwrap_or_default()
}

/// Step revisions are freshly allocated on every planning call. Normalize only
/// those opaque IDs, retaining their equality/order relationships and every
/// scope, selection, resolved range, cursor and instruction in the trace.
fn comparable_trace(output: &Value, base: &deadpan_core::RevisionId) -> Value {
    let mut trace = output["trace"].clone();
    let mut revisions = BTreeMap::new();
    for row in trace.as_array_mut().unwrap() {
        let revision = row["before_revision"].as_str().unwrap();
        if revision != base.as_str() {
            let next = format!("staged-{}", revisions.len());
            let revision = revisions.entry(revision.to_owned()).or_insert(next);
            row["before_revision"] = json!(revision);
        }
    }
    trace
}

fn preview_framing(
    package: &Path,
    request: &Value,
    owner: &str,
) -> Result<(Option<deadpan_core::Framing>, i64)> {
    // This is the same public JSON parser/preparation API the CLI and native
    // host share, giving the full predicted document behind --dry-run's report.
    let request = Request::from_json(&serde_json::to_vec(request)?)?;
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let prepared = deadpan_cli::macros::prepare(&store, &request)?;
    let document = &prepared
        .plan
        .as_ref()
        .ok_or("expected semantic plan")?
        .document;
    Ok((
        document.nodes()[&node(owner)].framing.clone(),
        document.duration()?.frames(),
    ))
}

/// Reusing an installed Follow's exact creep parameters is a valid preview,
/// preserving the complete recipe without history or register writes.
fn assert_unchanged_follow_retune_preview(
    package: &Path,
    input: &Path,
    request: &Value,
    owner: &str,
) -> Result {
    let before = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    let framing = before.nodes()[&node(owner)].framing.clone();
    assert!(matches!(
        &framing,
        Some(deadpan_core::Framing {
            value: FramingValue::Follow {
                scale_transition: Some(_),
                ..
            },
            ..
        })
    ));
    assert_eq!(
        preview_framing(package, request, owner)?,
        (framing, before.duration()?.frames())
    );
    let frozen = state(package)?;
    invoke(package, input, request, true)?;
    assert_eq!(state(package)?, frozen);
    Ok(())
}

fn assert_ranged_follow_preview(
    package: &Path,
    input: &Path,
    request: &Value,
    owner: &str,
    owner_start: i64,
    frames: i64,
    selection: (i64, i64),
) -> Result {
    let frozen = state(package)?;
    let (framing, _) = preview_framing(package, request, owner)?;
    invoke(package, input, request, true)?;
    assert_eq!(state(package)?, frozen);
    let Some(deadpan_core::Framing {
        value:
            FramingValue::Follow {
                scale_transition: Some(transition),
                ..
            },
        ..
    }) = framing
    else {
        panic!("ranged Follow preview")
    };
    let range = transition.range.ok_or("missing explicit Follow range")?;
    assert_eq!(
        range.start,
        ratio(
            i128::from(selection.0.min(selection.1) - owner_start),
            i128::from(frames)
        )
    );
    assert_eq!(
        range.end,
        ratio(
            i128::from(selection.0.max(selection.1) - owner_start),
            i128::from(frames)
        )
    );
    assert_eq!(range.from_factor, ExactRatio::ONE);
    Ok(())
}

#[test]
fn headless_zoom_macro_dry_run_rebinds_backward_range_and_changed_owner_duration_with_durable_undo()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("framing.json");
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let instruction = zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(2),
        },
        SemanticZoomCenter::OriginalCenter,
    );
    invoke(
        &package,
        &input,
        &save(
            &before,
            0,
            "z",
            serde_json::to_value([instruction.clone()])?,
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let request = invocation(
        &before,
        bank.version,
        "z",
        "camera-source",
        20,
        Some((20, 40, false)),
        "range-framed",
    );
    let frozen = state(&package)?;
    let predicted = preview_framing(&package, &request, "camera-source")?;
    let preview = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    assert_eq!(preview["edit"]["duration_delta"], 0);
    assert_eq!(
        preview["trace"][1]["instruction"],
        serde_json::to_value(&instruction)?
    );
    assert_eq!(
        preview["trace"][1]["resolved_range"],
        json!({"start":20,"end":40})
    );
    let result = invoke(&package, &input, &request, false)?;
    assert_eq!(result["trace"], preview["trace"]);
    assert_eq!(result["context"], preview["context"]);
    assert_eq!(result["edit"], preview["edit"]);
    assert!(result["committed_registers"].is_null());
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.nodes()[&node("camera-source")].framing, predicted.0);
    assert_eq!(after.duration()?.frames(), predicted.1);
    for frame in [0, 19, 40, 119] {
        assert_eq!(
            pose(&after, "camera-source", frame),
            FramingPose::identity()
        );
    }
    for frame in [20, 30, 39] {
        assert_eq!(
            pose(&after, "camera-source", frame).scale,
            ExactRatio::integer(2)
        );
    }
    assert_eq!(writer.registers()?, bank);
    writer.undo(after.revision_id(), RevisionId::new("framing-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("framing-undo")?,
        RevisionId::new("framing-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.undo(
        &RevisionId::new("framing-redo")?,
        RevisionId::new("framing-restored")?,
    )?;
    drop(writer);
    let retimed = command(
        &package,
        &input,
        Command::WrapRetime {
            node: node("camera-source"),
            id: node("retime"),
            duration: FrameDuration::new(80)?,
            pitch: PitchPolicy::Preserve,
        },
        "retimed",
    )?;
    let replay = invocation(
        &retimed,
        bank.version,
        "z",
        "retime",
        50,
        Some((70, 50, true)),
        "retime-framed",
    );
    let predicted = preview_framing(&package, &replay, "retime")?;
    let dry = invoke(&package, &input, &replay, true)?;
    let result = invoke(&package, &input, &replay, false)?;
    assert_eq!(result["trace"], dry["trace"]);
    assert_eq!(
        result["context"]["visual_selection"],
        replay["operation"]["visual_selection"]
    );
    assert_eq!(
        result["trace"][1]["resolved_range"],
        json!({"start":50,"end":70})
    );
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.nodes()[&node("retime")].framing, predicted.0);
    assert_eq!(after.duration()?.frames(), 80);
    for frame in [20, 39, 49, 70, 79] {
        assert_eq!(pose(&after, "retime", frame), FramingPose::identity());
    }
    for frame in [50, 60, 69] {
        assert_eq!(pose(&after, "retime", frame).scale, ExactRatio::integer(2));
    }
    assert_eq!(writer.registers()?, bank);
    writer.undo(after.revision_id(), RevisionId::new("retime-framing-undo")?)?;
    same_document(&writer.snapshot()?, &retimed)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn headless_creep_macro_resolves_after_staged_hold_duration_edit_and_nested_call_in_one_undo()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("staged-framing.json");
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let index = store.source_video_index(document.revision_id(), &AssetId::new("camera")?)?;
    let timestamp = deadpan_core::SourceTimestamp {
        ticks: index.frames()[0].pts,
        time_base: index.time_base(),
    };
    drop(store);
    let before = command(
        &package,
        &input,
        Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node("freeze"),
                nodes: BTreeMap::from([(
                    node("freeze"),
                    BeatNode::hold(
                        "Freeze",
                        HoldRecipe {
                            duration: FrameDuration::new(100)?,
                            picture_context: None,
                            video: HoldVideo::Freeze {
                                asset: AssetId::new("camera")?,
                                timestamp,
                            },
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "freeze-added",
    )?;
    let creep = zoom(
        ZoomKind::Creep {
            from: None,
            to: ratio(3, 2),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::OriginalCenter,
    );
    invoke(
        &package,
        &input,
        &save(&before, 0, "z", serde_json::to_value([creep.clone()])?),
        false,
    )?;
    let body = vec![
        I::SetHoldDuration {
            length: PauseLength::Frames { frames: count(80) },
        },
        I::MoveFrames {
            forward: true,
            count: count(10),
        },
        I::BeginSelection,
        I::MoveFrames {
            forward: true,
            count: count(20),
        },
        I::FinishSelection,
        I::Call {
            register: RegisterName::new('z')?,
            count: count(1),
        },
    ];
    invoke(
        &package,
        &input,
        &save(&before, 1, "a", serde_json::to_value(body)?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let request = invocation(
        &before,
        bank.version,
        "a",
        "freeze",
        0,
        None,
        "staged-framed",
    );
    let frozen = state(&package)?;
    let predicted = preview_framing(&package, &request, "freeze")?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    assert_eq!(dry["edit"]["duration_delta"], -20);
    let result = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&result, before.revision_id()),
        comparable_trace(&dry, before.revision_id())
    );
    assert_eq!(result["context"], dry["context"]);
    assert_eq!(result["context"]["cursor"], 30);
    assert_eq!(
        result["context"]["visual_selection"],
        json!({"type":"time","anchor":10,"head":30,"extending":false})
    );
    let row = result["trace"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["instruction"] == serde_json::to_value(&creep).unwrap())
        .unwrap();
    assert_eq!(row["depth"], 2);
    assert_eq!(row["resolved_range"], json!({"start":10,"end":30}));
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.nodes()[&node("freeze")].framing, predicted.0);
    assert_eq!(after.duration()?.frames(), predicted.1);
    assert_eq!(after.node_duration(&node("freeze"))?.frames(), 80);
    let FramingValue::Envelope { envelope } = &after.nodes()[&node("freeze")]
        .framing
        .as_ref()
        .unwrap()
        .value
    else {
        panic!("ranged creep");
    };
    assert_eq!(envelope.segments[0].end, ratio(1, 8));
    assert_eq!(envelope.segments[1].end, ratio(3, 8));
    assert_eq!(pose(&after, "freeze", 9).scale, ExactRatio::ONE);
    assert!(
        pose(&after, "freeze", 20)
            .scale
            .compare(ExactRatio::ONE)
            .is_gt()
    );
    for frame in [30, 79] {
        assert_eq!(pose(&after, "freeze", frame).scale, ratio(3, 2));
    }
    assert_eq!(writer.registers()?, bank);
    writer.undo(after.revision_id(), RevisionId::new("staged-framing-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("staged-framing-undo")?,
        RevisionId::new("staged-framing-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn headless_zoom_macro_late_range_path_and_provider_refusals_leave_captures_marks_and_history_untouched()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("refused-framing.json");
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let before = command(
        &package,
        &input,
        Command::Insert {
            parent: document.root().clone(),
            index: 1,
            subtree: Subtree {
                root: node("blank"),
                nodes: BTreeMap::from([(
                    node("blank"),
                    BeatNode::hold(
                        "Blank",
                        HoldRecipe {
                            duration: FrameDuration::new(20)?,
                            picture_context: None,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "blank-added",
    )?;
    let instruction = zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(2),
        },
        SemanticZoomCenter::Keep,
    );
    for (version, register, suffix) in [
        (0, "r", vec![instruction.clone()]),
        (1, "p", vec![instruction.clone(), instruction.clone()]),
        (
            2,
            "b",
            vec![
                I::MoveBeats {
                    forward: true,
                    count: count(1),
                },
                instruction,
            ],
        ),
    ] {
        let mut body = vec![
            I::YankBeat {
                register: RegisterName::new('w')?,
            },
            I::SetMark { letter: 'k' },
        ];
        body.extend(suffix);
        invoke(
            &package,
            &input,
            &save(&before, version, register, serde_json::to_value(body)?),
            false,
        )?;
    }
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    for (register, selection, message) in [
        ("r", Some((20, 20, false)), "nonempty range"),
        ("p", Some((20, 40, false)), "camera path"),
        ("b", None, "no picture to frame"),
    ] {
        let request = invocation(
            &before,
            bank.version,
            register,
            "camera-source",
            20,
            selection,
            "must-not-exist",
        );
        let error = reject(&package, &input, &request)?;
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .contains(message),
            "{error}"
        );
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert!(
            store
                .snapshot_at(&RevisionId::new("must-not-exist")?)
                .is_err()
        );
        assert!(store.snapshot()?.marks().is_empty());
    }
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "synthetic-worker"))]
#[test]
fn headless_zoom_macro_resolves_real_accepted_generated_contract_with_one_durable_undo() -> Result {
    use deadpan_cli::picture::{PreparedPicture, ProjectPictureSession};
    use deadpan_render::{
        FitMode, FrameMetadata, PictureGeometry, Primaries, Rotation, SampleAspectRatio,
        SourceColor, Transfer,
    };
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    // The reusable recipe runs actual conditioning, worker/host qualification,
    // retained-object publication and acceptance. Only model inference is the
    // synthetic provider; this tests the complete accepted-store resolver path.
    let scratch = tempfile::tempdir()?;
    let fixture = recipes::generated::generated_pause(&scratch.path().join("generated"))?;
    let package = fixture.package;
    let input = scratch.path().join("generated-framing.json");
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let deadpan_core::NodeKind::Hold { recipe } = &before.nodes()[&node("black")].kind else {
        panic!("generated Hold");
    };
    let HoldVideo::Generated { accepted } = &recipe.video else {
        panic!("accepted generated provider");
    };
    assert!(accepted.artifact.content_aspect.is_some());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let origin = store
        .accepted_generation_origin(&accepted.artifact)?
        .ok_or("accepted origin receipt")?;
    let dimensions = origin.plan().native_dimensions();
    let bank = store.registers()?;
    drop(store);
    // Decode one actual admitted generated picture as an independent metadata
    // oracle. This is test qualification, outside the decoder-free resolver.
    let cancelled = AtomicBool::new(false);
    let mut pictures =
        ProjectPictureSession::open_revision(&package, before.revision_id(), None, &cancelled)?;
    let picture = pictures.prepare(ProjectFrame(18), &cancelled)?;
    let PreparedPicture::Generated { frame, .. } = &picture.picture else {
        panic!("decoded accepted generated picture");
    };
    let metadata = *frame.metadata();
    let decoded = pictures
        .source_info()
        .ok_or("decoded generated interpretation")?;
    assert_eq!(
        [decoded.width, decoded.height],
        [dimensions.width(), dimensions.height()]
    );
    assert_eq!(metadata.clean_aperture, None);
    assert_eq!(metadata.sample_aspect_ratio, SampleAspectRatio::SQUARE);
    assert_eq!(metadata.rotation, Rotation::None);
    assert_eq!(
        metadata.color,
        SourceColor {
            transfer: Transfer::Srgb,
            primaries: Primaries::Rec709
        }
    );
    assert_eq!(
        metadata.pts.time_base,
        deadpan_core::SourceTimeBase::new(1, 1000)?
    );
    assert_eq!(metadata.pts.ticks, 100);
    let raw = FrameMetadata {
        width: dimensions.width(),
        height: dimensions.height(),
        row_stride_bytes: dimensions.width() * 4,
        ..metadata
    };
    assert_eq!(
        deadpan_cli::picture::fill_canvas_aspect_metadata(
            raw,
            accepted.artifact.content_aspect.unwrap()
        ),
        metadata
    );
    let layers = picture.render_layers()?;
    let owner_layer = picture
        .framing
        .iter()
        .position(|layer| {
            layer.instance.node == node("black")
                && layer.instance.repeats.is_empty()
                && !layer.escalation
        })
        .unwrap();
    let geometry = PictureGeometry::composed(
        &metadata,
        picture.picture_context.as_deref(),
        picture.canvas,
        picture.canvas,
        FitMode::Fit,
        &layers,
    )?;
    let projected = geometry
        .source_to_input(
            owner_layer + usize::from(picture.gap_after.is_some()),
            [0.5, 0.5],
        )?
        .ok_or("visible generated center")?;
    let projected = projected.map(|value| {
        ExactRatio::new(
            (value * deadpan_core::FRAMING_NUMERIC_SCALE as f64).round_ties_even() as i128,
            i128::from(deadpan_core::FRAMING_NUMERIC_SCALE),
        )
        .unwrap()
    });
    drop(pictures);
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(2),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::OriginalCenter,
    );
    invoke(
        &package,
        &input,
        &save(
            &before,
            bank.version,
            "g",
            serde_json::to_value([instruction])?,
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let request = invocation(
        &before,
        bank.version,
        "g",
        "black",
        18,
        Some((18, 23, false)),
        "generated-framed",
    );
    let frozen = state(&package)?;
    let predicted = preview_framing(&package, &request, "black")?;
    let FramingValue::Envelope { envelope } = &predicted.0.as_ref().unwrap().value else {
        panic!("ranged generated creep");
    };
    let arrival = envelope
        .segments
        .iter()
        .find(|segment| segment.end == ratio(2, 3))
        .ok_or("generated creep arrival")?;
    assert_eq!([arrival.pose.center_x, arrival.pose.center_y], projected);
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let result = invoke(&package, &input, &request, false)?;
    assert_eq!(result["trace"], dry["trace"]);
    assert_eq!(result["context"], dry["context"]);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    // The accepted request remains Current. Direct store history navigation
    // needs the same relevance resolver the production CLI writer installs.
    writer.set_generation_context_resolver(Arc::new(
        deadpan_cli::generation_context::BoundaryContextResolver::default(),
    ));
    let after = writer.snapshot()?;
    assert_eq!(after.nodes()[&node("black")].framing, predicted.0);
    assert_eq!(after.duration()?.frames(), predicted.1);
    assert_eq!(
        after.nodes()[&node("black")].kind,
        before.nodes()[&node("black")].kind
    );
    assert_eq!(pose(&after, "black", 17).scale, ExactRatio::ONE);
    for frame in [23, 26] {
        assert_eq!(pose(&after, "black", frame).scale, ExactRatio::integer(2));
    }
    assert_eq!(writer.registers()?, bank);
    writer.undo(
        after.revision_id(),
        RevisionId::new("generated-framing-undo")?,
    )?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.set_generation_context_resolver(Arc::new(
        deadpan_cli::generation_context::BoundaryContextResolver::default(),
    ));
    writer.redo(
        &RevisionId::new("generated-framing-undo")?,
        RevisionId::new("generated-framing-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}
