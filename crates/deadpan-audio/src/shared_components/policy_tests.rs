//! Metadata decisions are compared with both real numerical traversal orders.
//! This test oracle is not a production policy visitor or a resource proof.

use super::*;
use crate::stages::shared_components::program::tests::plan;
use deadpan_core::{
    AudioTreatments, BeatNode, ColorPolicy, FrameDuration, FrameRate, GainDb, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    TailEffect, TailSends,
};

const FLAGS: [[bool; 4]; 3] = [
    [true, false, true, false],
    [false, true, true, false],
    [false; 4],
];

type Call<'plan> = (
    AudioOwnerClock<'plan>,
    Temporal,
    AudioSample,
    Permissions,
    Option<usize>,
);

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn layout<'plan>(
    plan: &'plan RenderPlan,
    focus: &str,
    send: bool,
    mode: usize,
    control: WorkControl<'_>,
) -> Components<'plan> {
    let query = plan
        .audio_gain_owners(AudioSample(12)..AudioSample(16), Default::default())
        .unwrap();
    assert_eq!(query.spans().len(), 1);
    let layers: Vec<_> = query.spans()[0]
        .owners()
        .iter()
        .cloned()
        .map(|clock| {
            let until = clock.treatments().map_or(0, |recipe| recipe.order().len());
            (clock, until)
        })
        .collect();
    let focused = layers
        .iter()
        .find(|(clock, _)| clock.instance().node.as_str() == focus)
        .unwrap();
    let skip = match mode {
        1 => vec![authored_gain::identity(&layers[0].0)],
        2 => vec![authored_gain::identity(&focused.0)],
        _ => Vec::new(),
    };
    let stop = (mode == 3).then(|| OwnerKey::new(&focused.0));
    let mut components =
        Components::new(AudioSample(12), 4, send.then_some(TailEffect::Delay), &skip);
    for voice in 0..3 {
        let contribution = if voice == 0 {
            Contribution::Original
        } else {
            Contribution::Sound(
                TailSends::default()
                    .with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
            )
        };
        assert_eq!(
            components
                .route(
                    (voice, contribution),
                    layers.clone(),
                    0..4,
                    stop.as_ref(),
                    control
                )
                .unwrap(),
            voice
        );
    }
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
    components
}

fn populate(components: &mut Components<'_>, control: WorkControl<'_>) {
    components.allocate();
    for (id, flags) in FLAGS.iter().enumerate() {
        for (at, permitted) in flags.iter().enumerate() {
            if *permitted {
                components
                    .add(
                        id,
                        at,
                        ([0.17 - id as f64 * 0.07, -0.23], [0.01, -0.02]),
                        if id == 0 { 0 } else { 1500 },
                        control,
                    )
                    .unwrap();
            }
        }
    }
}

fn eq_calls<'plan>(
    components: &Components<'plan>,
    owner: &Owner<'plan>,
    stage: usize,
    at: usize,
    calls: &mut Vec<Call<'plan>>,
) {
    let active = |id: usize| FLAGS[id][at];
    let permissions = permissions(&components.routes, owner, active, false).unwrap();
    let clock = first_clock(&components.routes, owner, active, false);
    for (id, _) in members(&components.routes, owner, active, false) {
        calls.push((
            clock.as_ref().unwrap().clone(),
            Temporal::Equalizer(stage),
            AudioSample(components.start.0 + at as i64),
            permissions,
            Some(components.routes[id].voice),
        ));
    }
}

fn wet_calls<'plan>(
    components: &Components<'plan>,
    owner: &Owner<'plan>,
    id: usize,
    layer: usize,
    at: usize,
    calls: &mut Vec<Call<'plan>>,
) {
    if !FLAGS[id][at] || !components.routes[id].wet_allowed(layer) {
        return;
    }
    let context = wet_context(&components.routes, owner, |id| FLAGS[id][at]).unwrap();
    let sends = components.routes[id].layers[layer]
        .0
        .treatments()
        .map(|recipe| recipe.wet_sends())
        .unwrap_or_default();
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        if sends.level(effect).is_some() {
            calls.push((
                context.clock.as_ref().unwrap().clone(),
                Temporal::Wet(effect),
                AudioSample(components.start.0 + at as i64),
                context.permissions,
                Some(components.routes[id].voice),
            ));
        }
    }
}

/// Metadata bridge traversal over external flags, before PCM allocation.
/// Preserve both engine orders instead of pretending they are interchangeable.
fn expected_calls<'plan>(components: &Components<'plan>, dynamic: bool) -> Vec<Call<'plan>> {
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
    let owners = ordered_owners(&components.owners);
    let mut calls = Vec::new();
    if dynamic {
        for at in 0..components.frames {
            for owner in &owners {
                let recipe = owner.clock.treatments();
                for stage in 0..owner.until {
                    if is_equalizer(recipe.unwrap().order()[stage], recipe) {
                        eq_calls(components, owner, stage, at, &mut calls);
                    }
                }
                if owner.wet {
                    for &(id, layer) in &owner.members {
                        wet_calls(components, owner, id, layer, at, &mut calls);
                    }
                }
            }
        }
    } else {
        for owner in owners {
            let recipe = owner.clock.treatments();
            for stage in 0..owner.until {
                if is_equalizer(recipe.unwrap().order()[stage], recipe) {
                    for at in 0..components.frames {
                        eq_calls(components, owner, stage, at, &mut calls);
                    }
                }
            }
            if owner.wet {
                for &(id, layer) in &owner.members {
                    for at in 0..components.frames {
                        wet_calls(components, owner, id, layer, at, &mut calls);
                    }
                }
            }
        }
    }
    calls
}

fn same_calls(expected: &[Call<'_>], actual: &[Call<'_>]) {
    assert_eq!(expected.len(), actual.len());
    for (expected, actual) in expected.iter().zip(actual) {
        assert_eq!(expected.0.instance(), actual.0.instance());
        assert_eq!(expected.0.definition(), actual.0.definition());
        assert_eq!(expected.0.kind(), actual.0.kind());
        assert_eq!(expected.0.gap_after(), actual.0.gap_after());
        assert_eq!(expected.0.origin(), actual.0.origin());
        assert_eq!(
            expected.0.sampling().local_frames_per_sample(),
            actual.0.sampling().local_frames_per_sample()
        );
        assert_eq!(
            expected.0.sampling().local_at(expected.2).unwrap(),
            actual.0.sampling().local_at(actual.2).unwrap()
        );
        assert_eq!(
            expected.0.escalation_millidecibels(),
            actual.0.escalation_millidecibels()
        );
        assert!(expected.1 == actual.1);
        assert_eq!(expected.2, actual.2);
        assert!(expected.3 == actual.3);
        assert_eq!(expected.4, actual.4);
    }
}

#[test]
fn structural_decisions_match_both_numerical_callback_orders() {
    for dynamic in [false, true] {
        for gain_first in [false, true] {
            for muted in [false, true] {
                for send in [false, true] {
                    for mode in 0..4 {
                        let plan = plan(dynamic, muted, gain_first);
                        for selected in [None, Some(0), Some(1), Some(99)] {
                            let cancelled = AtomicBool::new(false);
                            let work = RefCell::new(ReadWork::default());
                            let control = control(&cancelled, &work);
                            let mut components = layout(&plan, "inner", send, mode, control);
                            let expected = expected_calls(&components, dynamic);
                            // Permission decisions above read only FLAGS and
                            // immutable route/owner layout, not a zero PCM run.
                            assert!(components.routes.iter().all(|route| route.pcm.is_empty()));
                            populate(&mut components, control);
                            let mut aliases = Permissions::default();
                            aliases.insert(1).unwrap();
                            let mut actual = Vec::new();
                            components
                                .finish(
                                    selected,
                                    aliases,
                                    control,
                                    |clock, kind, at, mask, voice| {
                                        actual.push((clock.clone(), kind, at, mask, voice));
                                        Ok([0.125, -0.25])
                                    },
                                )
                                .unwrap();
                            same_calls(&expected, &actual);
                            assert!(actual.iter().all(|call| call.2 != AudioSample(15)));
                            // Literal fixture facts do not share the policy
                            // helper implementation with the bridge oracle.
                            assert_eq!(
                                actual
                                    .iter()
                                    .filter(|call| matches!(call.1, Temporal::Equalizer(_)))
                                    .count(),
                                if mode == 2 { 0 } else { 4 }
                            );
                            assert_eq!(
                                actual
                                    .iter()
                                    .filter(|call| matches!(
                                        call.1,
                                        Temporal::Wet(TailEffect::Delay)
                                    ))
                                    .count(),
                                if mode == 2 || mode == 3 { 0 } else { 4 }
                            );
                            assert!(actual.iter().all(|call| call.4 != Some(2)));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn metadata_masks_use_voice_ids_and_choose_the_first_active_clock() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut components = layout(&plan, "inner", false, 0, control(&cancelled, &work));
    for (route, voice) in components.routes.iter_mut().zip([5, 2, 7]) {
        route.voice = voice;
    }
    let layer = components.routes[0]
        .layers
        .iter()
        .position(|(clock, _)| clock.instance().node.as_str() == "inner")
        .unwrap();
    let original_clock = components.routes[0].layers[layer].0.clone();
    let earlier = plan
        .audio_gain_owners(AudioSample(8)..AudioSample(12), Default::default())
        .unwrap();
    let sound_clock = earlier.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "inner")
        .unwrap()
        .clone();
    // Whole Repeat play gain may differ within the same structural owner.
    assert_eq!(OwnerKey::new(&original_clock), OwnerKey::new(&sound_clock));
    assert_eq!(original_clock.escalation_millidecibels(), 3000);
    assert_eq!(sound_clock.escalation_millidecibels(), 0);
    components.routes[1].layers[layer].0 = sound_clock;
    let owner = &components.owners[&OwnerKey::new(&original_clock)];
    for (at, expected) in [[true, false], [false, true], [true, true], [false, false]]
        .into_iter()
        .enumerate()
    {
        let mask = permissions(&components.routes, owner, |id| FLAGS[id][at], false).unwrap();
        assert_eq!(mask.contains(5), expected[0]);
        assert_eq!(mask.contains(2), expected[1]);
        assert!(!mask.contains(0));
        assert!(!mask.contains(1));
        assert!(!mask.contains(7));
        let clock = first_clock(&components.routes, owner, |id| FLAGS[id][at], false);
        assert_eq!(
            clock.map(|clock| clock.escalation_millidecibels()),
            match at {
                0 | 2 => Some(3000),
                1 => Some(0),
                _ => None,
            }
        );
    }
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
}

fn tail_plan() -> RenderPlan {
    let id = |name: &str| NodeId::new(name).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("policy-tail").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let hold = |duration, audio| {
        BeatNode::hold(
            "Hold",
            HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(duration).unwrap(),
                video: HoldVideo::Background,
                audio,
            },
        )
    };
    let mut root = BeatNode::sequence("Root", vec![id("lead"), id("tail")]);
    root.audio_treatments = AudioTreatments::default().with_wet_sends(
        TailSends::default().with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), root),
        (id("lead"), hold(8, HoldAudio::Silence)),
        (
            id("tail"),
            hold(
                24,
                HoldAudio::Tail {
                    maximum: FrameDuration::new(8).unwrap(),
                    effect: TailEffect::Reverb,
                    input: deadpan_core::TailInput::Original,
                },
            ),
        ),
    ]))
    .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}

#[test]
fn metadata_decisions_exclude_original_tail_returns_without_allocating_pcm() {
    let plan = tail_plan();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let components = layout(&plan, "tail", false, 0, control(&cancelled, &work));
    let root = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "root")
        .unwrap();
    assert!(!components.routes[0].wet_allowed(0));
    assert!(components.routes[1].wet_allowed(0));
    let dry = permissions(&components.routes, root, |id| id < 2, false).unwrap();
    assert!(dry.contains(0) && dry.contains(1));
    assert!(!dry.contains(2));
    let context = wet_context(&components.routes, root, |id| id < 2).unwrap();
    assert!(!context.permissions.contains(0));
    assert!(context.permissions.contains(1));
    assert!(!context.permissions.contains(2));
    assert!(context.clock.is_some());
    let original_only = wet_context(&components.routes, root, |id| id == 0).unwrap();
    assert!(original_only.permissions.is_empty());
    assert!(original_only.clock.is_none());
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
}

#[test]
fn member_permissions_keep_the_existing_voice_limit_failure() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut components = layout(&plan, "inner", false, 0, control(&cancelled, &work));
    components.routes[0].voice = super::super::super::shared_mix::MAX_VOICES;
    let owner = components.owners.values().next().unwrap();
    assert!(matches!(
        permissions(&components.routes, owner, |id| id == 0, false),
        Err(StageAudioError::Limit("shared EQ voice count"))
    ));
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
}
