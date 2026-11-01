//! Ignored pre-G capture for the actual routed projected-mask return seam.
use super::*;
use deadpan_core::{
    BeatNode, ExactRatio, FrameDuration, FrameRange, FrameRate, HoldAudio, HoldRecipe, HoldVideo,
    NodeId, NodeKind, PitchPolicy, ProjectDocument, ProjectFrame, RetimePurpose, SoundRoute,
};
use deadpan_plan::{
    AudioBoundaryRule, AudioQueryLimits, AudioRoutedSignal, AudioSampleGrid, AudioSignalContent,
    AudioSignalTape, AudioSignalTapeRun, AudioSoundRoute, AudioStageProjection, RenderPlan,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn checked_plan_with_silent_hold() -> (Arc<RenderPlan>, super::super::controlled_reads::Provider) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (document, provider) = super::super::controlled_reads::fixture_document_at_rate(rate);
    let mut wire = serde_json::to_value(document).unwrap();
    let duration = |frames| FrameDuration::new(frames).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("stage")])).unwrap();
    wire["nodes"]["room"] = serde_json::to_value(BeatNode::hold(
        "Silent hold",
        HoldRecipe {
            picture_context: None,
            duration: duration(128),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    wire["nodes"]["b"] = serde_json::to_value(BeatNode::sequence(
        "Preserve input",
        vec![id("a"), id("room")],
    ))
    .unwrap();
    wire["nodes"]["stage"] = serde_json::to_value(BeatNode {
        framing: None,
        label: "Preserve".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("b"),
            duration: duration(768),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(640)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    })
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}

fn projected<'plan>(plan: &'plan RenderPlan) -> Arc<AudioStageProjection<'plan>> {
    let query = plan
        .audio_signal()
        .query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let AudioSignalContent::Stage(stage) = query.spans.into_iter().next().unwrap().content else {
        panic!("the checked root must contain the Preserve stage");
    };
    let signal = stage.input_signal();
    let input = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(640, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(640, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal.clone(),
        )],
    )
    .unwrap();
    let output_policy = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(768, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(768, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal,
        )],
    )
    .unwrap();
    AudioStageProjection::new(
        stage,
        input,
        output_policy,
        FrameDuration::new(768).unwrap(),
    )
    .unwrap()
}

fn routed<'plan>(projection: Arc<AudioStageProjection<'plan>>) -> AudioRoutedSignal<'plan> {
    let route = SoundRoute::identity(ratio(768, 1)).unwrap();
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let grid_count = route.nodes().len();
    let signal_route = AudioSoundRoute::<SignalSample>::new(route, vec![grid; grid_count]).unwrap();
    AudioRoutedSignal::projected(projection, signal_route).unwrap()
}

fn samples_as_bits(samples: &[[f32; 2]]) -> Vec<[u32; 2]> {
    samples
        .iter()
        .map(|sample| [sample[0].to_bits(), sample[1].to_bits()])
        .collect()
}

fn sha256_sample_bits(samples: &[[u32; 2]]) -> String {
    let mut digest = Sha256::new();
    for sample in samples {
        digest.update(sample[0].to_le_bytes());
        digest.update(sample[1].to_le_bytes());
    }
    let mut result = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut result, "{byte:02x}").unwrap();
    }
    result
}

fn one_case(
    case: &str,
    renderer: &mut StageAudio,
    provider: &mut super::super::controlled_reads::Provider,
    domain: &AudioRoutedSignal<'_>,
    start: i64,
    frames: u32,
    expect_owned: bool,
) -> Value {
    let policy = match domain.input() {
        deadpan_plan::AudioRoutedSignalInput::Projected(projection) => {
            projection
                .output_policy()
                .policy_after_preserve(
                    SignalSample(start)..SignalSample(start + i64::from(frames)),
                    AudioQueryLimits::default(),
                )
                .unwrap()
                .suppressed
        }
        _ => unreachable!("capture fixture uses a projected provider"),
    };
    super::projected_mask_before_trace::begin();
    let before_cache = renderer.cached_stage_count();
    let provider_calls_before = provider.calls;
    let result = renderer.read_routed_signal(
        provider,
        domain,
        SignalSample(start),
        frames,
        TIMEOUT,
        &AtomicBool::new(false),
    );
    let trace = super::projected_mask_before_trace::take();
    assert_eq!(trace.dropped, 0, "source-ready receipt rows were truncated");
    assert!(trace.rows.iter().all(|row| !row.source_ranges_truncated));
    let source_rows = trace.rows;
    let trace_attempted = trace.attempted;
    let trace_dropped = trace.dropped;
    if result.is_ok() {
        assert_eq!(
            trace_attempted, 1,
            "expected exactly one delivered projected span"
        );
        assert_eq!(source_rows.len(), 1, "receipt must identify the root span");
        assert_eq!(source_rows[0].start, start);
        assert_eq!(source_rows[0].frames, frames as usize);
    }
    match result {
        Ok(block) => {
            assert_eq!(
                block.suppressed.is_owned(),
                expect_owned,
                "returned mask ownership must match this capture mode"
            );
            let sample_bits = samples_as_bits(&block.samples);
            json!({
                "case": case,
                "preasserted_policy_ranges": policy.iter().map(|r| [r.start.0,r.end.0]).collect::<Vec<_>>(),
                "branch_reached": !source_rows.is_empty(),
                "trace_attempted": trace_attempted,
                "trace_dropped": trace_dropped,
                "source_rows": source_rows,
                "before_cached_stages": before_cache,
                "after_cached_stages": renderer.cached_stage_count(),
                "provider_calls_before": provider_calls_before,
                "provider_calls_after": provider.calls,
                "result": "Ok",
                "public_block": {
                    "schema_version": block.schema_version,
                    "stage": block.stage,
                    "project_id": serde_json::to_value(&block.project_id).unwrap(),
                    "revision_id": serde_json::to_value(&block.revision_id).unwrap(),
                    "allocation": [block.allocation.start.0, block.allocation.end.0],
                    "start": block.start.0,
                    "sample_len": block.samples.len(),
                    "sample_capacity": block.samples.capacity(),
                    "suppressed_ranges": block.suppressed.as_slice().iter().map(|r| [r.start.0,r.end.0]).collect::<Vec<_>>(),
                    "suppressed_len": block.suppressed.len(),
                    "suppressed_capacity": block.suppressed.capacity(),
                    "suppressed_owned": block.suppressed.is_owned(),
                },
                "result_samples_f32_bits": sample_bits,
                "result_samples_f32_bits_sha256_le": sha256_sample_bits(&sample_bits),
            })
        }
        Err(error) => json!({
            "case": case,
            "preasserted_policy_ranges": policy.iter().map(|r| [r.start.0,r.end.0]).collect::<Vec<_>>(),
            "branch_reached": !source_rows.is_empty(),
            "trace_attempted": trace_attempted,
            "trace_dropped": trace_dropped,
            "source_rows": source_rows,
            "before_cached_stages": before_cache,
            "after_cached_stages": renderer.cached_stage_count(),
            "provider_calls_before": provider_calls_before,
            "provider_calls_after": provider.calls,
            "result": "Err",
            "error": format!("{error:?}"),
        }),
    }
}

fn capture_cases_with_mode(owned_mode: bool) -> Value {
    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let domain = routed(Arc::clone(&projection));

    let mut empty_renderer = StageAudio::new(Arc::clone(&plan));
    let empty = one_case(
        "source_only_empty",
        &mut empty_renderer,
        &mut provider,
        &domain,
        0,
        256,
        false,
    );

    assert!(empty["branch_reached"].as_bool().unwrap());
    assert_eq!(empty["result"], "Ok");
    assert_eq!(empty["source_rows"][0]["source_len"], 0);
    assert!(
        empty["preasserted_policy_ranges"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let mut positive_renderer = StageAudio::new(Arc::clone(&plan));
    let positive = one_case(
        "explicit_silent_hold_positive",
        &mut positive_renderer,
        &mut provider,
        &domain,
        512,
        256,
        owned_mode,
    );

    assert_eq!(positive["result"], "Ok");
    assert!(
        !positive["preasserted_policy_ranges"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(positive["branch_reached"].as_bool().unwrap());
    assert_eq!(
        positive["source_rows"][0]["source_ranges"],
        positive["preasserted_policy_ranges"]
    );

    let mut shared_renderer = StageAudio::new(Arc::clone(&plan));
    let shared_first_empty = one_case(
        "shared_renderer_first_empty",
        &mut shared_renderer,
        &mut provider,
        &domain,
        0,
        256,
        false,
    );
    let shared_second_positive = one_case(
        "shared_renderer_second_positive",
        &mut shared_renderer,
        &mut provider,
        &domain,
        512,
        256,
        owned_mode,
    );
    assert!(shared_first_empty["branch_reached"].as_bool().unwrap());
    assert_eq!(shared_first_empty["result"], "Ok");
    assert_eq!(shared_first_empty["source_rows"][0]["source_len"], 0);
    assert!(shared_second_positive["branch_reached"].as_bool().unwrap());
    assert_eq!(shared_second_positive["result"], "Ok");
    assert_eq!(
        shared_second_positive["source_rows"][0]["source_ranges"],
        shared_second_positive["preasserted_policy_ranges"]
    );

    json!({
        "schema": if owned_mode {
            "deadpan.routed_projected_mask.active.v1"
        } else {
            "deadpan.routed_projected_mask.before.v2"
        },
        "capture_mode": if owned_mode {
            "G active destination ownership"
        } else {
            "pre-G source-ready only"
        },
        "source_only_empty": empty,
        "explicit_silent_hold_positive": positive,
        "shared_renderer_first_empty": shared_first_empty,
        "shared_renderer_second_positive": shared_second_positive,
        "provider_calls": provider.calls,
        "limitations": if owned_mode {
            vec![
                "Raw source-ready receipts must match literally across modes; destination admission occurs after those receipts.",
                "Each public read gets a fresh request-local projected memo; repeated calls only reuse StageAudio-owned caches if measured.",
                "No public negative/fractional retained-coordinate case is asserted.",
            ]
        } else {
            vec![
                "Source-ready receipts are not destination ownership receipts.",
                "Plan work values are cumulative at the suppression return seam, not an isolated query fee.",
                "Each public read gets a fresh request-local projected memo; repeated calls only reuse StageAudio-owned caches if measured.",
                "No public negative/fractional retained-coordinate case is asserted.",
            ]
        },
    })
}

pub(in crate::stages) fn capture_active_projected_cases() -> Value {
    capture_cases_with_mode(true)
}

#[test]
fn active_projected_masks_match_frozen_source_ready_corpus() {
    let before_bytes =
        include_bytes!("../tests/fixtures/projected-policy-destination-baseline-v1/corpus.json");
    let mut digest = Sha256::new();
    digest.update(before_bytes);
    let mut actual_sha256 = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut actual_sha256, "{byte:02x}").unwrap();
    }
    assert_eq!(
        actual_sha256, "93912d8a589912b7ef41f0bd523db8da2a6cdec0d2ea5db1b6ba6f9941dffa51",
        "frozen source-ready corpus bytes changed"
    );
    let before: Value = serde_json::from_slice(before_bytes).unwrap();
    let before = &before["source_ready_before"];
    let active = capture_active_projected_cases();
    let cases = [
        ("source_only_empty", false),
        ("explicit_silent_hold_positive", true),
        ("shared_renderer_first_empty", false),
        ("shared_renderer_second_positive", true),
    ];
    for (name, expect_owned) in cases {
        let old = &before[name];
        let current = &active[name];
        let old_case = old.as_object().unwrap();
        let current_case = current.as_object().unwrap();
        assert_eq!(
            old_case.len(),
            current_case.len(),
            "case keys changed for {name}"
        );
        for key in old_case.keys().filter(|key| key.as_str() != "public_block") {
            assert_eq!(
                current[key.as_str()],
                old[key.as_str()],
                "case field {key} changed for {name}"
            );
        }

        let old_block = old["public_block"].as_object().unwrap();
        let current_block = current["public_block"].as_object().unwrap();
        assert_eq!(
            old_block.len(),
            current_block.len(),
            "public block keys changed for {name}"
        );
        for key in old_block
            .keys()
            .filter(|key| key.as_str() != "suppressed_owned")
        {
            assert_eq!(
                current["public_block"][key.as_str()],
                old["public_block"][key.as_str()],
                "public block field {key} changed for {name}"
            );
        }
        assert_eq!(
            current["public_block"]["suppressed_owned"], expect_owned,
            "ownership mode mismatch for {name}"
        );
    }
    assert_eq!(active["provider_calls"], before["provider_calls"]);
}

#[test]
#[ignore = "manual immutable active projected-mask capture"]
fn capture_routed_projected_mask_after_g() {
    let path = std::env::var_os("DEADPAN_PROJECTED_POLICY_AFTER_OUTPUT")
        .expect("set output path to a fresh capture file");
    let body = capture_active_projected_cases();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::path::PathBuf::from(path))
        .expect("capture output must be a fresh path");
    file.write_all(&serde_json::to_vec_pretty(&body).unwrap())
        .unwrap();
}

#[test]
#[ignore = "manual immutable pre-G routed projected-mask baseline capture"]
fn capture_routed_projected_mask_before_g() {
    let path = std::env::var_os("DEADPAN_ROUTED_PROJECTED_MASK_BEFORE_OUTPUT")
        .expect("set output path to a fresh capture file");
    let body =
        super::super::projected_signal_mask_request::with_legacy_projected_signal_masks(|| {
            capture_cases_with_mode(false)
        });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::path::PathBuf::from(path))
        .expect("capture output must be a fresh path");
    file.write_all(&serde_json::to_vec_pretty(&body).unwrap())
        .unwrap();
}

#[path = "routed_projected_mask_lifetime_tests.rs"]
mod copy_lifetime;
