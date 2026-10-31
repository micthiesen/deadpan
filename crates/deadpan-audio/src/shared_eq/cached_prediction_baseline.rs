//! Producer for the immutable pre-fast-path shared-prediction witness.
//! Run and freeze this before changing prediction; never refresh OLD afterward.

use super::*;
use crate::stages::controlled_reads::{Provider, fixture_document_at_rate};
use deadpan_core::{
    AudioEdgePolicy, AudioTreatments, ClipGain, GainClock, GainCurve, GainDb, GainEnvelope,
    GainRange, GainSegment, HoldAudio, HoldRecipe, HoldVideo, NodeId, ProjectDocument, Saturation,
    SoundEvent, SoundOverflowPolicy, SourceSpan, SourceTimeBase, SourceTimestamp,
};
use serde_json::{Value, json};

fn fixture() -> (Arc<RenderPlan>, Provider) {
    let (document, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(1408)).unwrap();
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range,
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(1408),
                GainDb::new(3000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let treatment = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::UNITY, false, vec![envelope], vec![]).unwrap(),
    )
    .with_saturation(Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()))
    .unwrap();
    let source = SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: 1408,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap(),
    };
    let sound = SoundEvent {
        owner: NodeId::new("root").unwrap(),
        label: "Independent prediction witness".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, FrameRate::new(48_000, 1).unwrap())
            .unwrap(),
        source,
        offset: AudioSample(0),
        treatments: AudioTreatments::default()
            .with_trim(GainDb::new(-9000).unwrap())
            .unwrap(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(treatment).unwrap();
    wire["nodes"]["room"] = serde_json::to_value(deadpan_core::BeatNode::hold(
        "Explicit silent Hold",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(128).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    wire["assets"]["media"]["source_qualification"] = json!("c".repeat(64));
    wire["sounds"] = json!({"effect": sound});
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    assert!(plan.has_audio_component_automation());
    assert!(!plan.sounds().is_empty());
    (plan, provider)
}

fn graph<'plan, 'work>(plan: &'plan RenderPlan) -> Graph<'plan, 'work> {
    Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        wet_filters: std::array::from_fn(|_| None),
        event_tiles: BTreeMap::new(),
        event_nodes: Default::default(),
        event_filters: BTreeMap::new(),
        voices: BTreeMap::new(),
        permission_families: BTreeMap::new(),
        sends: None,
        skip: Vec::new(),
        original_only: false,
        original_inputs: original_inputs::Cache::default(),
    }
}

fn control<'work>(
    cancelled: &'work AtomicBool,
    work: &'work RefCell<ReadWork>,
) -> WorkControl<'work> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}

fn ranges(values: &[Range<AudioSample>]) -> Vec<[i64; 2]> {
    values
        .iter()
        .map(|range| [range.start.0, range.end.0])
        .collect()
}

fn snapshot(renderer: &StageAudio, work: &RefCell<ReadWork>, provider: &Provider) -> Value {
    let work = work.borrow();
    json!({
        "plan_work": work.plan_work,
        "cache_plan_work": work.cache_plan_work,
        "cache_key_scopes": work.cache_key_scopes,
        "source_checks": work.source_checks,
        "observed_dependencies": work.observed.len(),
        "preflight_assets": work.preflight_assets.len(),
        "prepared_stages": work.prepared_stages,
        "prepared_frames": work.prepared_frames,
        "projected_resident_frames": work.projected_resident_frames,
        "sound_context_frames": work.sound_context_frames,
        "cached_resident_frames": renderer.cached_resident_frames(),
        "cached_stages": renderer.cached_stage_count(),
        "cached_original_payloads": renderer.original_inputs.len(),
        "next_original_input": renderer.next_original_input,
        "active_frames": renderer.active_frames,
        "provider_calls": provider.calls,
        "phase_work": cached_prediction_trace::snapshot(),
    })
}

fn assert_phases(
    before: cached_prediction_trace::Snapshot,
    after: cached_prediction_trace::Snapshot,
    require_positive: bool,
) {
    use cached_prediction_trace::Phase;
    assert_eq!(before.active_depth, 0);
    assert_eq!(after.active_depth, 0);
    assert_eq!(after.max_active_depth, 1);
    assert!(!after.nested, "inclusive phase totals overlap");
    assert_eq!(after.overflow, 0);
    if require_positive {
        for phase in [Phase::Queries, Phase::Prediction, Phase::OriginalRead] {
            assert!(after.phase(phase).plan_work > before.phase(phase).plan_work);
        }
    }
    if cached_prediction_trace::legacy() {
        assert_eq!(after.phase(Phase::Probe).plan_work, 0);
        assert_eq!(after.phase(Phase::Probe).cache_plan_work, 0);
        assert_eq!(after.calls[3], 0);
    }
}

fn wide(block: &WideBlock) -> Value {
    json!({
        "f64_bits": block.samples.iter().map(|sample| sample.map(f64::to_bits)).collect::<Vec<_>>(),
        "suppressed": ranges(&block.suppressed),
        "dependencies": block.dependencies.iter().map(|(id, hash)| (id.as_str(), hash)).collect::<BTreeMap<_, _>>(),
        "relative_depth": block.relative_depth,
        "exhausted_field": "WideBlock has no exhausted field; exact raw exhaustion is recorded separately",
    })
}

fn query_oracle<'plan>(
    renderer: &mut StageAudio,
    plan: &'plan RenderPlan,
    context: &Context<'plan>,
    start: AudioSample,
    frames: u32,
    cached: &ReadBlock,
) -> Value {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let request = original_request::with_legacy_flattened_queries(|| match context {
        Context::Root => renderer.prepare_original_root(plan, start, frames, control, true),
        Context::Domain(domain) => {
            renderer.prepare_original_domain(domain, start, frames, control, true)
        }
        Context::Retained(_) => unreachable!("retained prediction is outside this corpus"),
    })
    .unwrap();
    let construction_work = work.borrow().plan_work;
    let queries = request.queries();
    let mut excluded = vec![false; frames as usize];
    let mut events = Vec::new();
    raw_policy::visit(
        queries,
        start,
        frames as usize,
        control,
        || control.spend_plan_work(1),
        |event| {
            let (kind, range) = match event {
                raw_policy::Event::SilentHold { range, .. } => ("silent_hold", Some(range)),
                raw_policy::Event::Suppressed { range, .. } => ("suppressed", Some(range)),
                raw_policy::Event::Exhausted(range) => ("exhausted", Some(range)),
                raw_policy::Event::RetainedEnvelope { span, .. } => {
                    events.push(json!({"kind": "retained_envelope", "range": [span.samples.start.0, span.samples.end.0]}));
                    return Ok(());
                }
            };
            if let Some(range) = range {
                events.push(json!({"kind": kind, "range": [range.start.0, range.end.0]}));
                for (index, bit) in excluded.iter_mut().enumerate() {
                    *bit |= range.contains(&AudioSample(start.0 + index as i64));
                }
            }
            Ok(())
        },
    )
    .unwrap();
    let cached_excluded: Vec<_> = (0..frames)
        .map(|offset| {
            let at = AudioSample(start.0 + i64::from(offset));
            cached
                .suppressed
                .iter()
                .chain(&cached.exhausted)
                .any(|range| range.contains(&at))
        })
        .collect();
    assert_eq!(
        excluded, cached_excluded,
        "query policy must independently match cached masks"
    );
    let zero_but_permitted: Vec<_> = cached
        .samples
        .iter()
        .zip(&excluded)
        .enumerate()
        .filter_map(|(index, (sample, excluded))| {
            (!excluded && *sample == [0.0; 2]).then_some(start.0 + index as i64)
        })
        .collect();
    json!({
        "query_construction_work": construction_work,
        "query_visitor_work": work.borrow().plan_work - construction_work,
        "flattened_spans": queries.flattened.spans.len(),
        "processing": queries.processing.spans.iter().map(|span| match &span.content {
            AudioSignalContent::Leaf(_) => "leaf",
            AudioSignalContent::Stage(_) => "stage",
            AudioSignalContent::Bound(_) => "bound",
            AudioSignalContent::ProjectedStage(_) => "projected_stage",
        }).collect::<Vec<_>>(),
        "events": events,
        "query_excluded": excluded,
        "cached_excluded": cached_excluded,
        "zero_pcm_with_permitted_raw_policy": zero_but_permitted,
        "raw_start": cached.start.0,
        "raw_f32_bits": cached.samples.iter().map(|sample| sample.map(f32::to_bits)).collect::<Vec<_>>(),
        "raw_suppressed": ranges(&cached.suppressed),
        "raw_exhausted": ranges(&cached.exhausted),
        "raw_dependencies": cached.dependencies.iter().map(|(id, hash)| (id.as_str(), hash)).collect::<BTreeMap<_, _>>(),
        "raw_relative_depth": cached.relative_depth,
    })
}

pub(super) fn matrix() -> Value {
    let mut cases = Vec::new();
    for domain in [false, true] {
        for (name, start, frames) in [
            ("source", 0, 16),
            ("silent_hold_crossing", 496, 64),
            ("preserve_stage", 704, 16),
        ] {
            let (plan, mut provider) = fixture();
            // This externally owned immutable fixture context lives through
            // both reads; real owner queries inside each read remain admitted.
            let context = if domain {
                let owners = plan
                    .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
                    .unwrap();
                let owner = &owners.spans()[0].owners()[0];
                let input = plan
                    .audio_owner_input(owner, AudioSample(0), Default::default())
                    .unwrap();
                Context::Domain(Arc::new(input.context().clone()))
            } else {
                Context::Root
            };
            let contexts = Contexts {
                original: context.clone(),
                sounds: Sounds::Root,
            };
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let control = control(&cancelled, &work);
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            let reservation = u64::from(frames)
                .checked_mul(3)
                .and_then(|n| n.checked_add(original_inputs::Cache::resident_frames().ok()?))
                .unwrap();
            renderer.make_room(reservation, false, control).unwrap();
            renderer.active_frames = renderer.active_frames.checked_add(reservation).unwrap();
            let mut graph = graph(&plan);
            let program = Program {
                context: &contexts,
                stop: None,
                allowed: None,
                output_voice: None,
                parent: usize::MAX,
                depth: 0,
            };
            let start = AudioSample(start);
            cached_prediction_trace::reset();
            let before = snapshot(&renderer, &work, &provider);
            let before_first_phases = cached_prediction_trace::snapshot();
            let first_calls = provider.calls;
            let first = renderer
                .read_shared_program(&mut provider, &mut graph, program, start, frames, control)
                .unwrap();
            assert_phases(
                before_first_phases,
                cached_prediction_trace::snapshot(),
                true,
            );
            let after_first = snapshot(&renderer, &work, &provider);
            assert!(provider.calls > first_calls);
            assert_eq!(renderer.original_inputs.len(), 1);
            let published_key = renderer.next_original_input;
            let range = start..AudioSample(start.0 + i64::from(frames));
            let original_inputs::Read::Cached(first_pin) = renderer
                .shared_original_input(
                    &mut provider,
                    &mut graph,
                    &context,
                    range.clone(),
                    control,
                    0,
                )
                .unwrap()
            else {
                panic!("completed shared read must publish its actual Original input");
            };
            let oracle = query_oracle(&mut renderer, &plan, &context, start, frames, &first_pin);
            if name == "silent_hold_crossing" {
                assert!(
                    oracle["query_excluded"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|bit| bit.as_bool() == Some(true))
                );
            }
            if name == "preserve_stage" {
                assert!(
                    oracle["processing"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|kind| kind.as_str() == Some("stage"))
                );
            }
            let before_second = snapshot(&renderer, &work, &provider);
            let before_second_phases = cached_prediction_trace::snapshot();
            let second_calls = provider.calls;
            let second = renderer
                .read_shared_program(&mut provider, &mut graph, program, start, frames, control)
                .unwrap();
            assert_phases(
                before_second_phases,
                cached_prediction_trace::snapshot(),
                cached_prediction_trace::legacy(),
            );
            let after_second = snapshot(&renderer, &work, &provider);
            assert!(provider.calls > second_calls);
            assert_eq!(renderer.original_inputs.len(), 1);
            assert_eq!(renderer.next_original_input, published_key);
            let original_inputs::Read::Cached(second_pin) = renderer
                .shared_original_input(&mut provider, &mut graph, &context, range, control, 0)
                .unwrap()
            else {
                panic!("repeated shared read must retain the same Original input");
            };
            assert!(Arc::ptr_eq(&first_pin, &second_pin));
            assert_eq!(wide(&first), wide(&second));
            let after_arc_verification = snapshot(&renderer, &work, &provider);
            let result = json!({
                "name": name,
                "context": if domain { "exact_domain_arc" } else { "root" },
                "range": [start.0, start.0 + i64::from(frames)],
                "prediction_enabled": plan.has_audio_component_automation(),
                "same_cached_arc": true,
                "fixed_graph_reservation": reservation,
                "before": before,
                "after_first": after_first,
                "before_second": before_second,
                "after_second": after_second,
                "after_arc_verification": after_arc_verification,
                "first": wide(&first),
                "second": wide(&second),
                "independent_query_oracle": oracle,
            });
            drop(first_pin);
            drop(second_pin);
            renderer.release_shared_graph(graph, reservation);
            assert!(renderer.original_inputs.is_empty());
            assert_eq!(renderer.active_frames, 0);
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert_eq!(work.borrow().projected_resident_frames, 0);
            let cleanup = snapshot(&renderer, &work, &provider);
            drop(renderer);
            assert_eq!(work.borrow().sound_context_frames, 0);
            cases.push(json!({"result": result, "cleanup": cleanup}));
        }
    }
    json!({
        "schema": 1,
        "phase_order": ["queries", "prediction", "original_read", "probe"],
        "producer": "actual shared read_shared_program observations",
        "legacy_query_mode": cached_prediction_trace::legacy(),
        "coverage": "Root and exact Domain allocation; nonconstant gain plus saturation forces components/prediction; real Source, explicit SilentHold crossing, one Preserve Stage; same Graph and same Arc for repeat",
        "unexercised": ["Bound", "nested Preserve", "retained-envelope prefix/suffix exhaustion", "signed Domain history", "Context::Retained", "source revocation", "reservation eviction", "deliberate mask corruption"],
        "measurement_note": "Arc verification performs separately visible cache hits outside first/second snapshot intervals; independent query oracle has its own actual work ledger; Preserve prepared cache is dropped with renderer after Graph cleanup",
        "cases": cases,
    })
}

#[test]
#[ignore = "produces immutable OLD evidence only before production changes"]
fn produce_old_cached_prediction_matrix() {
    use std::io::Write;
    let path = std::env::var_os("DEADPAN_CACHED_PREDICTION_BASELINE").expect("set OLD output path");
    let result = cached_prediction_trace::with_legacy(matrix);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&result).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
}

fn zero_fixture() -> (Arc<RenderPlan>, Provider, [u8; 32]) {
    use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
    use deadpan_media::source_index::SourceContentIdentity;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    let (plan, mut provider) = fixture();
    let mut bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
    )
    .unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let mut at = 12_usize;
    let mut signed_pcm = false;
    let mut zeroed_data = false;
    while at.checked_add(8).unwrap() <= bytes.len() {
        let length = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = at.checked_add(8).unwrap().checked_add(length).unwrap();
        assert!(end <= bytes.len());
        match &bytes[at..at + 4] {
            b"fmt " => {
                assert!(length >= 16);
                assert_eq!(
                    u16::from_le_bytes(bytes[at + 8..at + 10].try_into().unwrap()),
                    1
                );
                assert_eq!(
                    u16::from_le_bytes(bytes[at + 10..at + 12].try_into().unwrap()),
                    2
                );
                assert_eq!(
                    u16::from_le_bytes(bytes[at + 22..at + 24].try_into().unwrap()),
                    16
                );
                signed_pcm = true;
            }
            b"data" => {
                assert!(length > 0);
                bytes[at + 8..end].fill(0);
                assert!(bytes[at + 8..end].iter().all(|byte| *byte == 0));
                zeroed_data = true;
            }
            _ => {}
        }
        at = end.checked_add(length % 2).unwrap();
    }
    assert!(signed_pcm && zeroed_data);
    let sha256: [u8; 32] = Sha256::digest(&bytes).into();
    let cancelled = AtomicBool::new(false);
    let session = AudioSession::open_verified(
        &mut Cursor::new(&bytes),
        SourceContentIdentity::new(sha256, bytes.len() as u64).unwrap(),
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let index = session.index().clone();
    provider.prepared = crate::PreparedSource::with_layout(
        session,
        &index,
        provider.prepared.matrix_layout(),
        &cancelled,
    )
    .unwrap();
    (plan, provider, sha256)
}

/// Separate supplement: numerical zero Source audio keeps its raw permissions.
pub(super) fn zero_matrix() -> Value {
    let mut cases = Vec::new();
    for domain in [false, true] {
        let (plan, mut provider, wav_sha256) = zero_fixture();
        {
            let declared = plan
                .audio(AudioSample(0)..AudioSample(16), Default::default())
                .unwrap();
            assert!(!declared.spans.is_empty());
            assert!(
                declared
                    .spans
                    .iter()
                    .all(|span| matches!(&span.content, AudioContent::Source { .. }))
            );
        }
        let context = if domain {
            let owners = plan
                .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
                .unwrap();
            let input = plan
                .audio_owner_input(
                    &owners.spans()[0].owners()[0],
                    AudioSample(0),
                    Default::default(),
                )
                .unwrap();
            Context::Domain(Arc::new(input.context().clone()))
        } else {
            Context::Root
        };
        let contexts = Contexts {
            original: context.clone(),
            sounds: Sounds::Root,
        };
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let frames = 16_u32;
        let reservation = u64::from(frames)
            .checked_mul(3)
            .and_then(|n| n.checked_add(original_inputs::Cache::resident_frames().ok()?))
            .unwrap();
        renderer.make_room(reservation, false, control).unwrap();
        renderer.active_frames = renderer.active_frames.checked_add(reservation).unwrap();
        let mut graph = graph(&plan);
        let program = Program {
            context: &contexts,
            stop: None,
            allowed: None,
            output_voice: None,
            parent: usize::MAX,
            depth: 0,
        };
        cached_prediction_trace::reset();
        let before = snapshot(&renderer, &work, &provider);
        let first_phases = cached_prediction_trace::snapshot();
        let first_calls = provider.calls;
        let first = renderer
            .read_shared_program(
                &mut provider,
                &mut graph,
                program,
                AudioSample(0),
                frames,
                control,
            )
            .unwrap();
        assert_phases(first_phases, cached_prediction_trace::snapshot(), true);
        let after_first = snapshot(&renderer, &work, &provider);
        assert!(provider.calls > first_calls);
        assert_eq!(renderer.original_inputs.len(), 1);
        let published_key = renderer.next_original_input;
        let original_inputs::Read::Cached(first_pin) = renderer
            .shared_original_input(
                &mut provider,
                &mut graph,
                &context,
                AudioSample(0)..AudioSample(16),
                control,
                0,
            )
            .unwrap()
        else {
            panic!("zero Source must publish actual cached PCM");
        };
        assert!(first_pin.samples.iter().all(|sample| *sample == [0.0; 2]));
        assert!(first_pin.suppressed.is_empty());
        assert!(first_pin.exhausted.is_empty());
        assert!(!first_pin.dependencies.is_empty());
        let oracle = query_oracle(
            &mut renderer,
            &plan,
            &context,
            AudioSample(0),
            frames,
            &first_pin,
        );
        assert!(
            oracle["query_excluded"]
                .as_array()
                .unwrap()
                .iter()
                .all(|bit| bit.as_bool() == Some(false))
        );
        assert_eq!(
            oracle["zero_pcm_with_permitted_raw_policy"]
                .as_array()
                .unwrap()
                .len(),
            frames as usize
        );
        assert!(first.samples.iter().all(|sample| *sample == [0.0; 2]));
        assert!(
            first.suppressed.is_empty(),
            "numerical zero must retain shared contribution permission"
        );
        let before_second = snapshot(&renderer, &work, &provider);
        let second_phases = cached_prediction_trace::snapshot();
        let second_calls = provider.calls;
        let second = renderer
            .read_shared_program(
                &mut provider,
                &mut graph,
                program,
                AudioSample(0),
                frames,
                control,
            )
            .unwrap();
        assert_phases(
            second_phases,
            cached_prediction_trace::snapshot(),
            cached_prediction_trace::legacy(),
        );
        let after_second = snapshot(&renderer, &work, &provider);
        assert!(provider.calls > second_calls);
        assert_eq!(renderer.next_original_input, published_key);
        let original_inputs::Read::Cached(second_pin) = renderer
            .shared_original_input(
                &mut provider,
                &mut graph,
                &context,
                AudioSample(0)..AudioSample(16),
                control,
                0,
            )
            .unwrap()
        else {
            panic!("zero Source repeat must retain actual cached PCM");
        };
        assert!(Arc::ptr_eq(&first_pin, &second_pin));
        assert_eq!(wide(&first), wide(&second));
        let after_arc_verification = snapshot(&renderer, &work, &provider);
        let result = json!({
            "name": "verified_zero_wav_source",
            "context": if domain { "exact_domain_arc" } else { "root" },
            "range": [0, 16],
            "wav_sha256": wav_sha256,
            "declared_source_audio": true,
            "all_zero_pcm_permitted": true,
            "prediction_enabled": plan.has_audio_component_automation(),
            "same_cached_arc": true,
            "fixed_graph_reservation": reservation,
            "before": before,
            "after_first": after_first,
            "before_second": before_second,
            "after_second": after_second,
            "after_arc_verification": after_arc_verification,
            "first": wide(&first),
            "second": wide(&second),
            "independent_query_oracle": oracle,
        });
        drop(first_pin);
        drop(second_pin);
        renderer.release_shared_graph(graph, reservation);
        assert!(renderer.original_inputs.is_empty());
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(work.borrow().projected_resident_frames, 0);
        let cleanup = snapshot(&renderer, &work, &provider);
        drop(renderer);
        cases.push(json!({"result": result, "cleanup": cleanup}));
    }
    json!({
        "schema": 1,
        "phase_order": ["queries", "prediction", "original_read", "probe"],
        "producer": "actual shared read_shared_program zero Source supplement",
        "legacy_query_mode": cached_prediction_trace::legacy(),
        "coverage": "Root and exact Domain with verified signed-16 stereo WAV whose data is all zero; declared SourceAudio, empty raw exclusion and all requested zero samples permitted",
        "cases": cases,
    })
}

#[test]
#[ignore = "produces separate immutable OLD zero-Source supplement"]
fn produce_old_cached_prediction_zero_matrix() {
    use std::io::Write;
    let path = std::env::var_os("DEADPAN_CACHED_PREDICTION_ZERO_BASELINE")
        .expect("set new zero supplement output path");
    let result = cached_prediction_trace::with_legacy(zero_matrix);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&result).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
}

#[path = "flattened_original_admission_tests.rs"]
mod flattened_original_admission;
