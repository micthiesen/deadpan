use super::*;
use deadpan_core::{
    AssetId, AssetRecord, AudioBindingState, AudioEdgePolicy, AudioTimingId, AudioTimingRecord,
    BeatNode, BeatSound, ColorPolicy, FrameDuration, FrameRate, FrozenAudioLayout, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    SoundClockJournal, SoundClockReference, SoundId, SoundOverflowPolicy, SourceAudio,
    SourceAudioMapping, SourceQualificationId, SourceSpan, SourceTimeBase, SourceTimestamp,
};
use std::collections::BTreeMap;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn source_span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 10,
            time_base,
        },
        SourceTimestamp {
            ticks: 4_810,
            time_base,
        },
    )
    .unwrap()
}

fn asset(label: &str) -> AssetRecord {
    AssetRecord {
        label: label.into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(source_span()),
        still_image: false,
        frame_count: None,
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}

fn beat_sound(label: &str, asset: &str) -> BeatSound {
    let source = SourceAudio {
        asset: AssetId::new(asset).unwrap(),
        span: source_span(),
    };
    BeatSound {
        label: label.into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            FrameRate::new(30_000, 1_001).unwrap(),
        )
        .unwrap(),
        source,
        offset: AudioSample(0),
        treatments: deadpan_core::AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn document(event_assets: &[&str], clocks_per_event: usize) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1_001).unwrap();
    let owner = id("owner");
    let root = id("root");
    let document = ProjectDocument::new(
        ProjectId::new("sound-clock-cache").unwrap(),
        revision("live"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (root, BeatNode::sequence("Root", vec![owner.clone()])),
        (
            owner.clone(),
            BeatNode::hold(
                "Owner",
                HoldRecipe {
                    duration: FrameDuration::new(100).unwrap(),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(
        event_assets
            .iter()
            .map(|name| (AssetId::new((*name).to_owned()).unwrap(), asset(name)))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        owner.clone(),
        event_assets
            .iter()
            .enumerate()
            .map(|(index, asset_name)| {
                let sound = SoundId::new(format!("sound-{index}")).unwrap();
                (sound, beat_sound(&format!("Sound {index}"), asset_name))
            })
            .collect::<BTreeMap<_, _>>(),
    )]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let layout = FrozenAudioLayout::capture(&document).unwrap();

    let mut timing_records = Vec::new();
    let mut clocks_by_sound = BTreeMap::new();
    for (event_index, _) in event_assets.iter().enumerate() {
        let sound = SoundId::new(format!("sound-{event_index}")).unwrap();
        let mut clocks = Vec::with_capacity(clocks_per_event);
        for ordinal in 0..clocks_per_event {
            let timing = AudioTimingId {
                allocation: revision(&format!("timing-{event_index}-{ordinal}")),
                ordinal: 0,
            };
            clocks.push(SoundClockReference::new(
                timing.clone(),
                owner.clone(),
                owner.clone(),
            ));
            timing_records.push(AudioTimingRecord {
                id: timing,
                layout: layout.clone(),
            });
        }
        clocks_by_sound.insert(
            sound,
            SoundClockJournal::new(owner.clone(), clocks).unwrap(),
        );
    }
    let bindings = AudioBindingState::new_with_sound_clocks(
        timing_records,
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::from([(owner, clocks_by_sound)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

macro_rules! fixture {
    ($plan:ident, $historical:ident, $scopes:ident, $original:ident, $current:ident) => {
        let document = document(&["asset-a"], 2);
        let $plan = RenderPlan::compile(&document).unwrap();
        let $scopes = $plan
            .beat_sound_clock_scopes(&id("owner"), &SoundId::new("sound-0").unwrap(), 65_536)
            .unwrap();
        let $historical = $plan
            .compile_sound_processing_layout(
                $scopes[0].historical_layout(),
                &std::collections::BTreeSet::from([AssetId::new("asset-a").unwrap()]),
            )
            .unwrap();
        let event = &document.beat_sounds()[&id("owner")][&SoundId::new("sound-0").unwrap()];
        let recipe = crate::AudioSourceVoiceRecipe {
            source: event.source.clone(),
            mapping: event.mapping,
            offset: event.offset,
        };
        let instance = InstancePath {
            node: id("owner"),
            repeats: Vec::new(),
        };
        let $original = $historical
            .source_voice_occurrence(instance.clone(), recipe.clone(), Default::default())
            .unwrap();
        let $current = $plan
            .source_voice_occurrence(instance, recipe, Default::default())
            .unwrap();
    };
}

#[test]
fn placement_cursor_matches_individual_queries_and_each_exact_budget() {
    fixture!(plan, historical, scopes, original, current);
    let mut expected = vec![6];
    let mut rows = Vec::new();
    let mut first_origin = None;
    for scope in &scopes {
        let (instance, work) = scope
            .try_remap_instance_with_work(current.instance(), 65_536)
            .unwrap();
        expected.push(work);
        let placement = scope
            .historical_layout()
            .sound_occurrence_placement(&instance.unwrap(), None, 65_536)
            .unwrap();
        expected.push(placement.work);
        let first = *first_origin.get_or_insert(placement.owner_origin);
        let start = original
            .extent()
            .start
            .checked_add(placement.owner_origin.checked_sub(first).unwrap())
            .unwrap();
        rows.push((
            start,
            placement.allocation,
            placement.delivery_start.checked_sub(start).unwrap(),
            false,
        ));
    }
    let placement = scopes[0]
        .live_layout()
        .sound_occurrence_placement(current.instance(), None, 65_536)
        .unwrap();
    expected.push(placement.work);
    let start = original
        .extent()
        .start
        .checked_add(
            placement
                .owner_origin
                .checked_sub(first_origin.unwrap())
                .unwrap(),
        )
        .unwrap();
    rows.push((
        start,
        placement.allocation,
        placement.delivery_start.checked_sub(start).unwrap(),
        false,
    ));
    for (index, needed) in expected.iter().copied().enumerate() {
        for exact in [true, false] {
            let mut cursor = current
                .placement_history_cursor(&original, &scopes, None)
                .unwrap();
            for previous in &expected[..index] {
                assert_eq!(cursor.advance(65_536).unwrap(), *previous);
            }
            let result = cursor.advance(if exact { needed } else { needed - 1 });
            if exact {
                assert_eq!(result.unwrap(), needed);
            } else {
                assert!(result.is_err());
                assert!(cursor.advance(65_536).is_err());
                assert!(cursor.finish().is_err());
            }
        }
    }
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, None)
        .unwrap();
    for work in expected {
        assert_eq!(cursor.advance(65_536).unwrap(), work);
    }
    assert!(cursor.is_complete());
    let proof = cursor.finish().unwrap();
    assert_eq!(proof.placements().len(), rows.len());
    for (row, expected) in proof.placements().iter().zip(rows) {
        assert_eq!(
            (
                row.complete_start(),
                row.allocation(),
                row.anchor(),
                row.expands_history()
            ),
            expected
        );
    }
}

#[test]
fn placement_cursor_rejects_partial_poisoned_and_oversized_journals() {
    fixture!(plan, historical, scopes, original, current);
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, None)
        .unwrap();
    assert_eq!(cursor.rows.capacity(), 0);
    assert_eq!(cursor.advance(6).unwrap(), 6);
    assert!(cursor.finish().is_err());
    let repeated = vec![scopes[0].clone(); 128];
    let mut cursor = current
        .placement_history_cursor(&original, &repeated, None)
        .unwrap();
    assert_eq!(cursor.rows.capacity(), 0);
    assert!(cursor.advance(65_536).is_err());
    assert_eq!(cursor.rows.capacity(), 0);
    assert!(cursor.finish().is_err());
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, None)
        .unwrap();
    assert!(cursor.advance(65_537).is_err());
    assert!(cursor.finish().is_err());
    assert!(
        current
            .placement_history_cursor(&original, &[], None)
            .is_err()
    );
}

#[test]
fn placement_cursor_keeps_context_remapping_a_separate_admitted_step() {
    fixture!(plan, historical, scopes, original, current);
    let context = plan
        .audio_sound_instance_context(current.instance(), Default::default())
        .unwrap();
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, Some(&context))
        .unwrap();
    assert_eq!(cursor.advance(6).unwrap(), 6);
    for scope in &scopes {
        let (instance, remap_work) = scope
            .try_remap_instance_with_work(current.instance(), 65_536)
            .unwrap();
        assert_eq!(cursor.advance(remap_work).unwrap(), remap_work);
        let (mapped, context_work) = scope.try_remap_context_with_work(&context, 65_536).unwrap();
        assert_eq!(cursor.advance(context_work).unwrap(), context_work);
        let placement = scope
            .historical_layout()
            .sound_occurrence_placement(&instance.unwrap(), mapped.as_ref(), 65_536)
            .unwrap();
        assert_eq!(cursor.advance(placement.work).unwrap(), placement.work);
    }
    let placement = scopes[0]
        .live_layout()
        .sound_occurrence_placement(current.instance(), Some(context.owner()), 65_536)
        .unwrap();
    assert_eq!(cursor.advance(placement.work).unwrap(), placement.work);
    let proof = cursor.finish().unwrap();
    assert!(proof.placements().iter().all(|row| row.expands_history()));
}

#[test]
fn placement_cursor_row_storage_matches_the_replaced_scalar_rows() {
    #[allow(dead_code)]
    struct OldRow {
        complete_start: ExactRatio,
        allocation: Range<ExactRatio>,
        anchor: ExactRatio,
        expands_history: bool,
    }
    assert_eq!(
        std::mem::size_of::<AudioSoundPlacement>(),
        std::mem::size_of::<OldRow>()
    );
    fixture!(plan, historical, scopes, original, current);
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, None)
        .unwrap();
    while !cursor.is_complete() {
        cursor.advance(65_536).unwrap();
    }
    assert_eq!(cursor.rows.len(), scopes.len() + 1);
    assert_eq!(cursor.rows.capacity(), scopes.len() + 1);
    eprintln!(
        "placement sizes: row={} cursor={} proof={} phase={} authorities={} capacity={}",
        std::mem::size_of::<AudioSoundPlacement>(),
        std::mem::size_of::<AudioSoundPlacementHistoryCursor<'_, '_, '_>>(),
        std::mem::size_of::<AudioSoundPlacementHistory<'_, '_, '_>>(),
        std::mem::size_of::<Phase>(),
        std::mem::size_of::<Authorities<'_, '_, '_>>(),
        cursor.rows.capacity()
    );
}

#[test]
fn placement_cursor_skips_absent_repeat_births_without_issuing_placement_queries() {
    use deadpan_core::{
        Command, CommandRequest, RepeatSelectionIdentities, SliceCaptureSelection, SplitIdentities,
    };
    fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
        let transition = deadpan_core::apply(
            document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: revision(name),
                command,
            },
        )
        .unwrap();
        transition.forward.apply(document).unwrap()
    }
    let timing = |name| AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    };
    let document = document(&["asset-a"], 1);
    let document = edit(
        &document,
        "wrapped",
        Command::RepeatSelection {
            parent: id("root"),
            selection: SliceCaptureSelection::Child { node: id("owner") },
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: id("repeat"),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing("wrapped"),
        },
    );
    let document = edit(
        &document,
        "three",
        Command::SetRepeatPlays {
            node: id("repeat"),
            plays: 3,
            timing: timing("three"),
        },
    );
    let document = edit(
        &document,
        "four",
        Command::SetRepeatPlays {
            node: id("repeat"),
            plays: 4,
            timing: timing("four"),
        },
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let event = &document.beat_sounds()[&id("owner")][&SoundId::new("sound-0").unwrap()];
    let recipe = crate::AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            recipe.clone(),
            AudioSample(320_320)..AudioSample(320_321),
            Default::default(),
        )
        .unwrap();
    let current = &batch.voices()[0];
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner"), &SoundId::new("sound-0").unwrap(), 65_536)
        .unwrap();
    let (first, instance) = scopes
        .iter()
        .find_map(|scope| {
            scope
                .try_remap_instance_with_work(current.instance(), 65_536)
                .unwrap()
                .0
                .map(|instance| (scope, instance))
        })
        .unwrap();
    let historical = plan
        .compile_sound_processing_layout(
            first.historical_layout(),
            &std::collections::BTreeSet::from([event.source.asset.clone()]),
        )
        .unwrap();
    let original = historical
        .source_voice_occurrence(instance, recipe, Default::default())
        .unwrap();
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, None)
        .unwrap();
    cursor.advance(6).unwrap();
    let mut skipped = 0;
    let mut applicable = 0;
    for scope in &scopes {
        let (instance, work) = scope
            .try_remap_instance_with_work(current.instance(), 65_536)
            .unwrap();
        assert_eq!(cursor.advance(work).unwrap(), work);
        if let Some(instance) = instance {
            applicable += 1;
            let placement = scope
                .historical_layout()
                .sound_occurrence_placement(&instance, None, 65_536)
                .unwrap();
            assert_eq!(cursor.advance(placement.work).unwrap(), placement.work);
        } else {
            skipped += 1;
        }
    }
    assert!(skipped > 0 && applicable > 0);
    let placement = scopes[0]
        .live_layout()
        .sound_occurrence_placement(current.instance(), None, 65_536)
        .unwrap();
    assert_eq!(cursor.advance(placement.work).unwrap(), placement.work);
    assert_eq!(cursor.finish().unwrap().placements().len(), applicable + 1);
}

#[test]
fn placement_authority_checks_charge_exact_borrows_and_semantic_fallback() {
    fixture!(plan, historical, scopes, original, unscoped);
    let context = plan
        .audio_sound_instance_context(unscoped.instance(), Default::default())
        .unwrap();
    let current = context
        .source_voice_occurrence(
            unscoped.instance().clone(),
            unscoped.recipe().clone(),
            Default::default(),
        )
        .unwrap();
    let mut cursor = current
        .placement_history_cursor(&original, &scopes, Some(&context))
        .unwrap();
    while !cursor.is_complete() {
        cursor.advance(65_536).unwrap();
    }
    let proof = cursor.finish().unwrap();
    assert_eq!(
        proof
            .validate(&original, &current, &scopes, &context, 4)
            .unwrap(),
        4
    );
    assert!(
        proof
            .validate(&original, &current, &scopes, &context, 3)
            .is_err()
    );
    let cloned_original = original.clone();
    let cloned_current = current.clone();
    let cloned_scopes = scopes.clone();
    let fallback_work = 4 + 22 + 6 * scopes.len();
    assert_eq!(
        proof
            .validate(
                &cloned_original,
                &cloned_current,
                &cloned_scopes,
                &context,
                fallback_work
            )
            .unwrap(),
        fallback_work
    );
    assert!(
        proof
            .validate(
                &cloned_original,
                &cloned_current,
                &cloned_scopes,
                &context,
                fallback_work - 1
            )
            .is_err()
    );
}
