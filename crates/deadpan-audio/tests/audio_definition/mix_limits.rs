use super::*;

use std::collections::BTreeSet;

use deadpan_plan::{
    AudioMixGate, AudioSignalContent, AudioSignalMix, AudioSignalTape, AudioSignalTapeRun,
    AudioStageProjection,
};

#[derive(Clone, Copy, Debug)]
enum HiddenHistory {
    Ordinary,
    Bound,
    Projected,
}

struct CountingProvider {
    prepared: PreparedSource,
    calls: usize,
    assets: BTreeSet<AssetId>,
}

impl CountingProvider {
    fn new() -> Self {
        Self {
            prepared: FixtureProvider::new().prepared,
            calls: 0,
            assets: BTreeSet::new(),
        }
    }
}

impl AudioSourceProvider for CountingProvider {
    fn source(
        &mut self,
        _project: &ProjectId,
        _revision: &RevisionId,
        asset: &AssetId,
        _cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.calls += 1;
        self.assets.insert(asset.clone());
        // Deliberate aliases of one qualified byte object isolate the distinct
        // asset-ID admission limit without opening a thousand physical files.
        Ok(&self.prepared)
    }
}

fn dependency_document(count: usize, bound: bool) -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let initial = document(
        rate,
        &["first"],
        [("first", source(rate, 1, 0..1))],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(initial).unwrap();
    let asset = wire["assets"]["media"].clone();
    let mut children = Vec::new();
    for ordinal in 0..count {
        let asset_id = AssetId::new(format!("dependency-{ordinal}")).unwrap();
        let node_id = id(&format!("input-{ordinal}"));
        let mut value = source(rate, 1, 0..1);
        let NodeKind::Source { source } = &mut value.kind else {
            unreachable!()
        };
        source.audio.as_mut().unwrap().asset = asset_id.clone();
        wire["assets"][asset_id.as_str()] = asset.clone();
        wire["nodes"][node_id.as_str()] = serde_json::to_value(value).unwrap();
        children.push(node_id);
    }
    wire["nodes"]["first"]["kind"]["source"]["audio"]["asset"] = serde_json::json!("dependency-0");
    wire["nodes"]["group"] =
        serde_json::to_value(BeatNode::sequence("All inputs", children)).unwrap();
    let input_frames = i64::try_from(count).unwrap();
    // Expansion keeps each 256-point output-policy query below the independent
    // 256-span cap, so the fixture isolates dependency admission.
    let output_frames = input_frames * 2;
    wire["nodes"]["inner"] = serde_json::to_value(retime(
        "group",
        output_frames,
        0..input_frames,
        PitchPolicy::Preserve,
    ))
    .unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("first"), id("inner")])).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    if !bound {
        return doc;
    }
    let captured = capture_unbound_audio_bindings(
        &doc,
        AudioTimingId {
            allocation: RevisionId::new("dependency-bindings").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let bindings = AudioBindingState::new(
        captured
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        BTreeMap::from([(id("inner"), captured.bindings()[&id("inner")].clone())]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn first_voice(plan: &RenderPlan) -> AudioSignalTape<'_> {
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ExactRatio::ONE,
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::ONE,
            plan.audio_definition(node("first")).unwrap().signal(),
        )],
    )
    .unwrap()
}

fn hidden_voice(plan: &RenderPlan, history: HiddenHistory) -> AudioSignalTape<'_> {
    let signal = plan.audio_definition(node("inner")).unwrap().signal();
    if matches!(history, HiddenHistory::Projected) {
        let stage = projection::stage(signal);
        let input = stage.input_signal();
        let input_support = input.support();
        let output_end = ExactRatio::integer(stage.descriptor().duration.frames());
        let tape = |end| {
            AudioSignalTape::new(
                plan,
                ExactRatio::ZERO..end,
                vec![AudioSignalTapeRun::new(
                    ExactRatio::ZERO..end,
                    input_support.clone(),
                    input.clone(),
                )],
            )
            .unwrap()
        };
        let duration = stage.descriptor().duration;
        let stage =
            AudioStageProjection::new(stage, tape(input_support.end), tape(output_end), duration)
                .unwrap();
        return AudioSignalTape::new(
            plan,
            ExactRatio::ZERO..ExactRatio::ONE,
            vec![AudioSignalTapeRun::intrinsic(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                stage,
            )],
        )
        .unwrap();
    }
    let query = signal
        .query(SignalSample(0)..SignalSample(1), Default::default())
        .unwrap();
    assert!(matches!(
        (&query.spans[0].content, history),
        (AudioSignalContent::Stage(_), HiddenHistory::Ordinary)
            | (AudioSignalContent::Bound(_), HiddenHistory::Bound)
    ));
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ExactRatio::ONE,
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::ONE,
            signal,
        )],
    )
    .unwrap()
}

fn mix(plan: &RenderPlan, history: HiddenHistory) -> AudioSignalMix<'_> {
    AudioSignalMix::new(
        plan,
        vec![first_voice(plan), hidden_voice(plan, history)],
        vec![AudioMixGate::new(
            ExactRatio::ZERO..ExactRatio::ONE,
            vec![1],
        )],
    )
    .unwrap()
}

#[test]
fn all_hidden_dependency_kinds_enforce_the_shared_cap_before_any_source_io_and_recover() {
    for history in [
        HiddenHistory::Ordinary,
        HiddenHistory::Bound,
        HiddenHistory::Projected,
    ] {
        let doc = dependency_document(1025, matches!(history, HiddenHistory::Bound));
        let plan = compile(&doc, false);
        let mix = mix(&plan, history);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let mut provider = CountingProvider::new();
        assert!(
            matches!(
                renderer.read_mix(
                    &mut provider,
                    &mix,
                    SignalSample(0),
                    1,
                    TIMEOUT,
                    &AtomicBool::new(false),
                ),
                Err(StageAudioError::Limit("source dependencies"))
            ),
            "{history:?}"
        );
        assert_eq!(
            provider.calls, 0,
            "{history:?}: an earlier audible voice must not read before hidden admission"
        );
        let recovery = AudioSignalMix::new(&plan, vec![first_voice(&plan)], vec![]).unwrap();
        let actual = renderer
            .read_mix(
                &mut provider,
                &recovery,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(actual.samples, vec![fixture_sample(0)]);
        assert_eq!(provider.calls, 1);
    }
}

#[test]
fn exactly_1024_distinct_dependencies_are_admitted_with_duplicate_and_gated_voices() {
    let doc = dependency_document(1024, false);
    let plan = compile(&doc, false);
    let mix = mix(&plan, HiddenHistory::Projected);
    let mut provider = CountingProvider::new();
    let actual = StageAudio::new(Arc::clone(&plan))
        .read_mix(
            &mut provider,
            &mix,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, vec![fixture_sample(0)]);
    assert!(actual.suppressed.is_empty());
    assert_eq!(provider.assets.len(), 1024);
    assert_eq!(
        provider.calls, 1025,
        "the shared first asset is admitted twice and the gated stage still prepares every input"
    );
}
