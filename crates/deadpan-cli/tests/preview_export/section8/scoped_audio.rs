//! A literal scoped audio program over six real Original-picture occurrences.
use super::*;
use deadpan_core::{
    AudioTreatmentStage as Stage, InstancePath, RepeatInstance, SemanticScopedContext,
};

pub const FRAMES: u64 = 72;
pub const SAMPLES: i64 = 115_315;

fn iteration(
    document: &ProjectDocument,
    repeat: &NodeId,
    index: u32,
) -> Result<deadpan_core::IterationId> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
        return Err("expected Repeat".into());
    };
    Ok(iterations.at(index).ok_or("Repeat play")?)
}

fn branch(document: &ProjectDocument, repeat: &NodeId, index: u32) -> Result<NodeId> {
    let NodeKind::Repeat { child, .. } = &document.nodes()[repeat].kind else {
        return Err("expected Repeat".into());
    };
    let iteration = iteration(document, repeat, index)?;
    Ok(document
        .overrides()
        .get(repeat)
        .and_then(|plays| plays.get(&iteration))
        .unwrap_or(child)
        .clone())
}

/// These expectations use literal setter values and the documented DSP equations,
/// never the setter's result or its compiled plan. The no-cut equal-gain EQ has
/// cancelling complementary boundaries and exactly one central impulse (see
/// equalizer_coefficients); it multiplies by 10^(3/20) before tanh saturation.
fn check_pcm(project: &Project) -> Result<Value> {
    let level = 10.0_f64.powf(-18.0 / 20.0);
    let selected = 0.5
        * (level
            * 10.0_f64.powf(-6.0 / 20.0)
            * 10.0_f64.powf(3.0 / 20.0)
            * 10.0_f64.powf(12.0 / 20.0))
        .tanh();
    let mut rows = Vec::new();
    for play in 0..6 {
        let expected_peak = if play == 3 { selected } else { level };
        let expected_ratio = if play == 3 { 2.0 } else { 0.75 };
        let start = boundary(play * 12 + 4);
        let mut peak = 0.0_f64;
        let mut squares = 0.0_f64;
        let mut count = 0;
        for at in (start..start + 4096).step_by(256) {
            let inspected = success(&[
                "inspect-audio",
                project.path(),
                "--samples",
                &at.to_string(),
                &(at + 256).to_string(),
                "--authored-bus",
            ])?;
            let samples: Vec<[f64; 2]> =
                serde_json::from_value(inspected["audio"]["samples"].clone())?;
            assert_eq!(samples.len(), 256);
            for [left, right] in samples {
                assert!(left.is_finite() && right.is_finite());
                assert!(
                    (right - expected_ratio * left).abs() < 0.000_002,
                    "play {play}: stereo ratio, {left}/{right}"
                );
                peak = peak.max(left.abs());
                squares += left * left;
                count += 1;
            }
        }
        assert!(
            (peak - expected_peak).abs() < 0.000_6,
            "play {play}: peak {peak}, independent expected {expected_peak}"
        );
        assert!(squares > 1.0, "play {play}: the tone must carry signal");
        rows.push(json!({"play":play+1,"samples":[start,start+4096],
            "expected_left_peak":expected_peak,"measured_left_peak":peak,
            "right_over_left":expected_ratio,"rms":(squares / f64::from(count)).sqrt()}));
    }
    Ok(
        json!({"equation":"L=.5*tanh(10^(-18/20)*10^(-6/20)*10^(3/20)*10^(12/20)); R=2L",
        "occurrences":rows}),
    )
}

pub fn build(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "scoped-audio-chain")?;
    project.shorten()?;
    project.delete_range(12, 30)?;
    let bleeped = project.apply_semantic(
        json!([
            {"type":"begin_selection"}, {"type":"move_frames","forward":true,"count":12},
            {"type":"bleep","register":"b","level":-18000}
        ]),
        0,
        None,
    )?;
    let leaf = root_child_at(&bleeped, 0)?.0;
    assert!(
        matches!(&bleeped.nodes()[&leaf].kind, NodeKind::Hold { recipe }
        if matches!(recipe.audio, HoldAudio::Tone { frequency_hz:1000, .. })
        && matches!(recipe.video, HoldVideo::Play { .. }))
    );
    project.apply_semantic(
        json!([
            {"type":"set_audio","change":{"type":"balance","balance":-25}}
        ]),
        0,
        Some(&leaf),
    )?;
    let inner = node("audio-inner")?;
    let outer = node("audio-outer")?;
    for (child, id, plays) in [(&leaf, &inner, 2), (&inner, &outer, 3)] {
        project.apply(|_, _, _| {
            Ok(Command::WrapRepeat {
                node: child.clone(),
                id: id.clone(),
                plays,
                gap: None,
                anchor_policy: WrapAnchorPolicy::First,
            })
        })?;
    }
    let (before, bank_version) = project.bank()?;
    assert_eq!(before.duration()?.frames(), FRAMES as i64);
    let baseline = before.nodes()[&leaf].audio_treatments.clone();
    let pairs = [
        (&outer, iteration(&before, &outer, 1)?),
        (&inner, iteration(&before, &inner, 1)?),
    ];
    let scoped = SemanticScopedContext {
        root: outer.clone(),
        target: ScopedNodeTarget {
            node: leaf.clone(),
            repeats: pairs
                .iter()
                .map(|(repeat, iteration)| RepeatEditStep {
                    repeat: (*repeat).clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration.clone(),
                    },
                })
                .collect(),
        },
        presentation: InstancePath {
            node: leaf.clone(),
            repeats: pairs
                .iter()
                .map(|(repeat, iteration)| RepeatInstance {
                    node: (*repeat).clone(),
                    iteration: iteration.clone(),
                })
                .collect(),
        },
    };
    project.macro_success(json!({"type":"apply", "parent":before.root(),
        "cursor":36, "selected_child":outer, "scoped":scoped,
        "program":{"instructions":[
            {"type":"set_audio","change":{"type":"trim","gain":-6000}},
            {"type":"set_audio","change":{"type":"saturation","drive":12000}},
            {"type":"set_audio","change":{"type":"balance","balance":50}},
            {"type":"set_audio","change":{"type":"equalizer","recipe":{"bass":3000,"middle":3000,"treble":3000}}}
        ]}}))?;
    let (after, after_bank) = project.bank()?;
    assert_ne!(after.revision_id(), before.revision_id());
    assert_eq!(
        after_bank, bank_version,
        "inline Apply must not alter registers"
    );
    assert_eq!(after.duration()?, before.duration()?);
    // Isolation adds owned branches; every pre-existing node remains identical.
    for (id, node) in before.nodes() {
        assert_eq!(after.nodes().get(id), Some(node), "{id}");
    }
    assert_eq!(
        after.overrides().get(&outer).ok_or("outer override")?.len(),
        1
    );
    for outer_index in 0..3 {
        let inner_owner = branch(&after, &outer, outer_index)?;
        for inner_index in 0..2 {
            let owner = branch(&after, &inner_owner, inner_index)?;
            let treatments = &after.nodes()[&owner].audio_treatments;
            if (outer_index, inner_index) != (1, 1) {
                assert_eq!(
                    treatments, &baseline,
                    "unselected {outer_index}/{inner_index}"
                );
                continue;
            }
            assert_ne!(owner, leaf);
            assert_eq!(
                treatments.order(),
                [
                    Stage::ClipGain,
                    Stage::Equalizer,
                    Stage::Saturation,
                    Stage::StereoBalance
                ]
            );
            assert_eq!(
                treatments
                    .clip_gain()
                    .ok_or("clip gain")?
                    .trim()
                    .millidecibels(),
                -6000
            );
            assert_eq!(
                treatments
                    .saturation()
                    .ok_or("saturation")?
                    .drive()
                    .millidecibels(),
                12000
            );
            assert_eq!(treatments.balance().ok_or("balance")?.percent(), 50);
            assert_eq!(
                treatments
                    .equalizer()
                    .ok_or("equalizer")?
                    .gains()
                    .map(|gain| gain.millidecibels()),
                [3000; 3]
            );
        }
    }
    let pcm = check_pcm(&project)?;
    fs::write(
        project.directory.join("planned-pcm-oracle.json"),
        serde_json::to_vec_pretty(&pcm)?,
    )?;
    let mut fixture = project.finish(vec!["Per-play overrides", "Semantic macros", "Clip gain", "Saturation", "Stereo balance", "EQ / filtering"],
        (0..FRAMES).map(|frame| (frame, original(12 + frame % 12))).collect(),
        vec![format!("Six 12-frame Original-picture tone occurrences; only outer 2 / inner 2 receives four literal setters. Planned PCM oracle: {pcm}")])?;
    fixture.signals = (0..6)
        .map(|play| Signal {
            start: boundary(play * 12 + 4),
            count: 4096,
            crossings_per_second: Some((2000.0, 0.03)),
            crest: None,
            peak: Some(if play == 3 {
                (0.169, 0.172)
            } else {
                (0.125, 0.127)
            }),
        })
        .collect();
    Ok(fixture)
}
