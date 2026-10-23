//! Public permission-family authority and nonlinear shared-input regression.
//! Child of new_group_birth.rs; parent owns registration and execution.
use super::*;
use deadpan_plan::{
    AudioQueryLimits, AudioSoundOwnerContext, AudioSoundPermissionFamily, RenderPlan,
};

fn split(document: &ProjectDocument, name: &str) -> Trial<ProjectDocument> {
    command(
        document,
        name,
        Command::Split {
            node: id("owner"),
            at: FrameDuration::new(2).unwrap(),
            identities: SplitIdentities {
                nodes: ["family-left", "family-right", "family-copy"]
                    .map(id)
                    .to_vec(),
            },
        },
    )
}
fn context<'p>(plan: &'p RenderPlan, node: &NodeId) -> Trial<AudioSoundOwnerContext<'p>> {
    plan.audio_sound_instance_context(
        &InstancePath {
            node: node.clone(),
            repeats: vec![],
        },
        Default::default(),
    )
    .map_err(|e| error("family context", e))
}
fn family<'p>(
    plan: &'p RenderPlan,
    context: &AudioSoundOwnerContext<'p>,
    owner: &NodeId,
) -> Trial<(Option<AudioSoundPermissionFamily<'p>>, usize)> {
    let event = &plan.beat_sounds()[owner][&sound_id("effect")];
    let voice = context
        .source_voice_occurrence(
            InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            deadpan_plan::AudioSourceVoiceRecipe {
                source: event.source.clone(),
                mapping: event.mapping,
                offset: event.offset,
            },
            Default::default(),
        )
        .map_err(|e| error("family occurrence", e))?;
    let clocks = plan
        .beat_sound_clock_scopes(owner, &sound_id("effect"), MAX_DOCUMENT_NODES)
        .map_err(|e| error("family clocks", e))?;
    voice
        .split_permission_family(&clocks, context, Default::default())
        .map_err(|e| error("family token", e))
}
fn some_family<'p>(
    plan: &'p RenderPlan,
    context: &AudioSoundOwnerContext<'p>,
    owner: &NodeId,
) -> Trial<AudioSoundPermissionFamily<'p>> {
    family(plan, context, owner)?
        .0
        .ok_or_else(|| error("family positive", "certified piece has no family"))
}
fn run_authority(directory: &std::path::Path) -> Trial<serde_json::Value> {
    let before = setup(directory)?;
    let before_plan = RenderPlan::compile(&before).map_err(|e| error("unsplit plan", e))?;
    let before_context = context(&before_plan, &id("birth-g"))?;
    let unsplit_none = family(&before_plan, &before_context, &id("owner"))?
        .0
        .is_none();
    let after = split(&before, "family-split")?;
    retain(directory, "split.json", &after)?;
    let owners = after.beat_sounds().keys().cloned().collect::<Vec<_>>();
    require(
        owners.len() == 2,
        "family fixture",
        "Split requires exactly two complementary event owners",
    )?;
    let plan = RenderPlan::compile(&after).map_err(|e| error("family plan", e))?;
    let g = context(&plan, &id("birth-g"))?;
    let a = some_family(&plan, &g, &owners[0])?;
    let b = some_family(&plan, &g, &owners[1])?;
    let equal = a == b;
    for owner in &owners {
        let scopes = plan
            .beat_sound_clock_scopes(owner, &sound_id("effect"), MAX_DOCUMENT_NODES)
            .map_err(|e| error("positive family aliases", e))?;
        let aliases = scopes
            .iter()
            .map(|scope| {
                scope
                    .try_remap_context_with_work(&g, MAX_DOCUMENT_NODES)
                    .map(|value| value.0)
                    .map_err(|e| error("positive family context alias", e))
            })
            .collect::<Trial<Vec<_>>>()?;
        require(
            aliases[0].is_none()
                && aliases.iter().skip(1).any(|alias| {
                    alias.as_ref().is_some_and(|instance| {
                        instance.node == id("birth-g") && instance.repeats.is_empty()
                    })
                }),
            "positive family history",
            "G must be absent before birth and genuinely aliased after Split",
        )?;
    }
    require(
        equal && a.comparison_work() > 0 && b.comparison_work() > 0 && unsplit_none,
        "family positive",
        "no equal genuine Split family or unsplit unexpectedly has family",
    )?;
    let root = context(&plan, after.root())?;
    let root_none = owners
        .iter()
        .map(|owner| family(&plan, &root, owner).map(|v| v.0.is_none()))
        .collect::<Trial<Vec<_>>>()?
        .into_iter()
        .all(|none| none);
    let left = context(&plan, &owners[0])?;
    let right = context(&plan, &owners[1])?;
    let left_family = family(&plan, &left, &owners[0])?.0;
    let right_family = family(&plan, &right, &owners[1])?.0;
    let leaves_do_not_match = match (&left_family, &right_family) {
        (Some(a), Some(b)) => a != b,
        _ => true,
    };
    let event = &after.beat_sounds()[&owners[0]][&sound_id("effect")];
    let voice = g
        .source_voice_occurrence(
            InstancePath {
                node: owners[0].clone(),
                repeats: vec![],
            },
            deadpan_plan::AudioSourceVoiceRecipe {
                source: event.source.clone(),
                mapping: event.mapping,
                offset: event.offset,
            },
            Default::default(),
        )
        .map_err(|e| error("negative current occurrence", e))?;
    let clocks = plan
        .beat_sound_clock_scopes(&owners[0], &sound_id("effect"), MAX_DOCUMENT_NODES)
        .map_err(|e| error("negative clocks", e))?;
    require(
        clocks.len() >= 2,
        "family negative fixture",
        "requires a nonempty truncated history",
    )?;
    let foreign = plan.clone();
    let foreign_context = context(&foreign, &id("birth-g"))?;
    let fresh_context = context(&plan, &id("birth-g"))?;
    let foreign_clocks = foreign
        .beat_sound_clock_scopes(&owners[0], &sound_id("effect"), MAX_DOCUMENT_NODES)
        .map_err(|e| error("foreign clocks", e))?;
    let mut reversed = clocks.clone();
    reversed.reverse();
    let mut negatives = Vec::new();
    let mut save = |name: &str,
                    result: std::result::Result<
        (Option<AudioSoundPermissionFamily<'_>>, usize),
        deadpan_plan::PlanError,
    >| {
        negatives.push(serde_json::json!({"case":name,"rejected":result.is_err(),
            "error":result.err().map(|e|format!("{e:?}"))}));
    };
    save(
        "foreign-context",
        voice.split_permission_family(&clocks, &foreign_context, Default::default()),
    );
    save(
        "fresh-same-G-context-arc",
        voice.split_permission_family(&clocks, &fresh_context, Default::default()),
    );
    save(
        "foreign-scopes",
        voice.split_permission_family(&foreign_clocks, &g, Default::default()),
    );
    save(
        "reordered-journal",
        voice.split_permission_family(&reversed, &g, Default::default()),
    );
    save(
        "truncated-prefix",
        voice.split_permission_family(&clocks[..clocks.len() - 1], &g, Default::default()),
    );
    save(
        "truncated-suffix",
        voice.split_permission_family(&clocks[1..], &g, Default::default()),
    );
    save(
        "too-small-budget",
        voice.split_permission_family(
            &clocks,
            &g,
            AudioQueryLimits {
                maximum_work: 1,
                ..Default::default()
            },
        ),
    );

    // Labels and trim are independent live fragment policy. Neither replaces
    // the retained source/mapping/offset recipe or its complementary permission.
    let mut label_event = event.clone();
    label_event.label.push_str(" renamed independently");
    let renamed = command(
        &after,
        "family-event-label",
        Command::SetBeatSound {
            owner: owners[0].clone(),
            id: sound_id("effect"),
            event: label_event.clone(),
        },
    )?;
    let renamed_plan = RenderPlan::compile(&renamed).map_err(|e| error("label event plan", e))?;
    let renamed_context = context(&renamed_plan, &id("birth-g"))?;
    let renamed_a = family(&renamed_plan, &renamed_context, &owners[0])?.0;
    let renamed_b = family(&renamed_plan, &renamed_context, &owners[1])?.0;
    let label_family_continuity = renamed_a.is_some() && renamed_a == renamed_b;
    label_event.treatments = label_event
        .treatments
        .with_trim(GainDb::new(-3000).unwrap())
        .unwrap();
    let changed = command(
        &renamed,
        "family-event-gain",
        Command::SetBeatSound {
            owner: owners[0].clone(),
            id: sound_id("effect"),
            event: label_event.clone(),
        },
    )?;
    retain(directory, "live-policy-events.json", &changed)?;
    let changed_plan = RenderPlan::compile(&changed).map_err(|e| error("trim event plan", e))?;
    let changed_context = context(&changed_plan, &id("birth-g"))?;
    let changed_a = family(&changed_plan, &changed_context, &owners[0])?.0;
    let changed_b = family(&changed_plan, &changed_context, &owners[1])?.0;
    let trim_family_continuity = changed_a.is_some() && changed_a == changed_b;
    let live_policy_keeps_journals = renamed.audio_bindings() == after.audio_bindings()
        && changed.audio_bindings() == after.audio_bindings();

    // One bounded recipe replacement uses the existing public setter. This
    // actual admitted2048-sample interval has exact natural duration1280/1001f,
    // less than2f, and fits the current leaf with offset0 and Reject overflow.
    // FitBeat is deliberately unavailable for independent sound events.
    label_event.source = audio(1024..3072);
    label_event.mapping = SourceAudioMapping::natural_rate(
        label_event.source.span,
        FrameRate::new(30_000, 1001).unwrap(),
    )
    .map_err(|e| error("family replacement natural mapping", e))?;
    label_event.offset = AudioSample(0);
    label_event.overflow = SoundOverflowPolicy::Reject;
    require(
        label_event
            .mapping
            .duration_frames(FrameDuration::new(2).unwrap())
            .map_err(|e| error("family replacement duration", e))?
            == ExactRatio::new(1280, 1001).unwrap(),
        "family replacement fixture",
        "requires exact2048-sample natural duration",
    )?;
    let replaced = command(
        &changed,
        "family-event-source-replacement",
        Command::SetBeatSound {
            owner: owners[0].clone(),
            id: sound_id("effect"),
            event: label_event,
        },
    )?;
    retain(directory, "replacement-source.json", &replaced)?;
    let replaced_plan = RenderPlan::compile(&replaced).map_err(|e| error("replacement plan", e))?;
    let replaced_context = context(&replaced_plan, &id("birth-g"))?;
    let replaced_family = family(&replaced_plan, &replaced_context, &owners[0])?.0;
    let replacement_retires_family = replaced_family.is_none()
        && !replaced
            .audio_bindings()
            .sound_clocks()
            .get(&owners[0])
            .is_some_and(|events| events.contains_key(&sound_id("effect")))
        && replaced.audio_bindings().sound_clocks().get(&owners[1])
            == changed.audio_bindings().sound_clocks().get(&owners[1]);

    // Compare original and pasted contexts within ONE live plan, so inequality
    // cannot pass merely because two separately compiled plans differ.
    let slice = CapturedEditSlice::capture_selection(
        &after,
        &id("top"),
        &SliceCaptureSelection::Child {
            node: id("birth-g"),
        },
        timing("family-capture"),
    )
    .map_err(|e| error("family whole-G capture", e))?;
    slice
        .validate_capture(&after)
        .map_err(|e| error("family capture validation", e))?;
    let pasted = command(
        &after,
        "family-paste",
        Command::SpliceSlice {
            parent: after.root().clone(),
            index: after.children(after.root()).count(),
            identities: slice_identities(&slice, "family-copy-group"),
            slice,
            timing: timing("family-paste"),
        },
    )?;
    retain(directory, "whole-G-copy.json", &pasted)?;
    let wrapper = pasted.children(pasted.root()).last().unwrap();
    let NodeKind::Sequence { children } = &pasted.nodes()[wrapper].kind else {
        return Err(error(
            "family copied wrapper",
            "expected ordinary neutral Sequence",
        ));
    };
    require(
        children.len() == 1,
        "family copied wrapper",
        "requires whole-G child",
    )?;
    let copied_g = children[0].clone();
    let copied_owners = pasted
        .beat_sounds()
        .keys()
        .filter(|owner| !after.beat_sounds().contains_key(*owner))
        .cloned()
        .collect::<Vec<_>>();
    require(
        copied_owners.len() == 2,
        "family copied forest",
        "requires both complementary pieces",
    )?;
    let pasted_plan = RenderPlan::compile(&pasted).map_err(|e| error("family pasted plan", e))?;
    let original_context = context(&pasted_plan, &id("birth-g"))?;
    let copied_context = context(&pasted_plan, &copied_g)?;
    let original_token = some_family(&pasted_plan, &original_context, &owners[0])?;
    let copied_a = some_family(&pasted_plan, &copied_context, &copied_owners[0])?;
    let copied_b = some_family(&pasted_plan, &copied_context, &copied_owners[1])?;
    let whole_copy_local = copied_a == copied_b && copied_a != original_token;
    let passed = equal
        && unsplit_none
        && root_none
        && leaves_do_not_match
        && label_family_continuity
        && trim_family_continuity
        && live_policy_keeps_journals
        && replacement_retires_family
        && whole_copy_local
        && negatives.iter().all(|row| row["rejected"] == true);
    Ok(
        serde_json::json!({"passed":passed,"same_G_equal":equal,"unsplit_none":unsplit_none,
        "root_none":root_none,"leaf_contexts_not_equal":leaves_do_not_match,
        "label_family_continuity":label_family_continuity,
        "trim_family_continuity":trim_family_continuity,
        "live_policy_keeps_journals":live_policy_keeps_journals,
        "source_replacement_retires_family":replacement_retires_family,
        "whole_copy_equal_only_in_its_context":whole_copy_local,
        "comparison_work":[a.comparison_work(),b.comparison_work()],"negatives":negatives,
        "executed_by_draft_author":false}),
    )
}
fn remove_events(document: &ProjectDocument, prefix: &str) -> Trial<ProjectDocument> {
    let mut result = document.clone();
    for (index, owner) in document.beat_sounds().keys().enumerate() {
        result = command(
            &result,
            &format!("{prefix}-{index}"),
            Command::DeleteBeatSound {
                owner: owner.clone(),
                id: sound_id("effect"),
            },
        )?;
    }
    Ok(result)
}
fn run_pcm(directory: &std::path::Path) -> Trial<serde_json::Value> {
    let base = setup(directory)?;
    let treatment = base.nodes()[&id("birth-g")]
        .audio_treatments
        .with_saturation(Some(Saturation::new(GainDb::new(12000).unwrap()).unwrap()))
        .unwrap();
    let before = command(
        &base,
        "family-g-saturation",
        Command::SetAudioTreatments {
            node: id("birth-g"),
            treatments: treatment.clone(),
        },
    )?;
    let after = split(&before, "family-shared-components-split")?;
    retain(directory, "before.json", &before)?;
    retain(directory, "after.json", &after)?;
    require(
        after.duration().unwrap() == before.duration().unwrap()
            && after.beat_sounds().len() == 2
            && after.nodes()[&id("birth-g")].audio_treatments == treatment,
        "nonlinear Split structure",
        "time, event aliases or G EQ+saturation changed",
    )?;
    let total = frame_sample_boundary(15);
    let expected = read_bus(&before, 0, total, &[1021])?;
    let actual = read_bus(&after, 0, total, &[37, 509, 1021, 173])?;
    let before_partitions = differences32(
        &expected,
        &read_bus(&before, 0, total, &[37, 509, 1021, 173])?,
    );
    let after_partitions = differences32(&actual, &read_bus(&after, 0, total, &[1021])?);
    let first = BIRTH_FIRST - 2048;
    let end = BIRTH_END + 2048;
    let before_raw = input(&before, &id("birth-g"), first, end, &[256])?;
    let after_raw = input(&after, &id("birth-g"), first, end, &[37, 173, 256])?;
    retain(directory, "raw-before.json", &input_evidence(&before_raw))?;
    retain(directory, "raw-after.json", &input_evidence(&after_raw))?;
    let raw = super::split::raw_comparison(
        first,
        &before_raw.samples,
        &before_raw.suppressed,
        &after_raw,
    );
    let mut ablations = Vec::new();
    let mut all_active = true;
    for (label, document, pcm) in [("before", &before, &expected), ("after", &after, &actual)] {
        let no_eq = command(
            document,
            &format!("family-{label}-no-eq"),
            Command::SetAudioTreatments {
                node: id("birth-g"),
                treatments: treatment.with_equalizer(None).unwrap(),
            },
        )?;
        let no_sat = command(
            document,
            &format!("family-{label}-no-sat"),
            Command::SetAudioTreatments {
                node: id("birth-g"),
                treatments: treatment.with_saturation(None).unwrap(),
            },
        )?;
        let no_event = remove_events(document, &format!("family-{label}-no-event"))?;
        let eq_pcm = read_bus(&no_eq, 0, total, &[1021])?;
        let sat_pcm = read_bus(&no_sat, 0, total, &[1021])?;
        let event_pcm = read_bus(&no_event, 0, total, &[1021])?;
        let counts = [
            super::split::active(pcm, &eq_pcm),
            super::split::active(pcm, &sat_pcm),
            super::split::active(pcm, &event_pcm),
        ];
        all_active &= counts.iter().all(|count| *count > 0);
        ablations.push(
            serde_json::json!({"side":label,"active_samples":{"eq":counts[0],
            "saturation":counts[1],"event":counts[2]},
            "no_eq_bits":eq_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
            "no_saturation_bits":sat_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
            "no_event_bits":event_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>() }),
        );
    }
    let mut cold = Vec::new();
    for at in [
        BIRTH_FIRST - 1,
        frame_sample_boundary(11) - 1,
        BIRTH_END - 1,
        total - 3,
    ] {
        let a = read_bus(&before, at, at + 3, &[3])?;
        let b = read_bus(&after, at, at + 3, &[3])?;
        cold.push(serde_json::json!({"first":at,
            "before":differences32(&expected[at as usize..at as usize+3],&a),
            "after":differences32(&actual[at as usize..at as usize+3],&b)}));
    }
    let comparison = differences32(&expected, &actual);
    let controls_valid = all_active
        && before_partitions["different_channel_samples"] == 0
        && after_partitions["different_channel_samples"] == 0
        && cold.iter().all(|row| {
            row["before"]["different_channel_samples"] == 0
                && row["after"]["different_channel_samples"] == 0
        });
    Ok(serde_json::json!({"controls_valid":controls_valid,
        "passed":raw["passed"]==true && before_raw.support==after_raw.support
            && comparison["different_channel_samples"]==0,
        "full_pcm":comparison,"raw_halo":raw,"before_partitions":before_partitions,
        "after_partitions":after_partitions,"cold":cold,"active_ablations":ablations,
        "expected_pcm_bits":expected.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "actual_pcm_bits":actual.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "executed_by_draft_author":false,"independent_dsp_claim":false}))
}
// Independent live-policy oracle: current event gain belongs before the shared
// G EQ. A label changes neither permission-family membership nor any PCM bit.
fn run_live_policy_pcm(directory: &std::path::Path) -> Trial<serde_json::Value> {
    let base = setup(directory)?;
    let before = split(&base, "family-policy-split")?;
    let plan = RenderPlan::compile(&before).map_err(|e| error("policy plan", e))?;
    // A Source leaf under a Split Partition retains the complete4f input
    // context; context.samples() therefore does not name its visible2f piece.
    // Enumerate public current STRUCTURAL occurrences to find the left piece.
    let mut owners = Vec::new();
    let mut piece_evidence = Vec::new();
    for owner in before.beat_sounds().keys() {
        let occurrences = plan
            .audio_owner_occurrences(
                owner,
                AudioSample(0)..AudioSample(frame_sample_boundary(15)),
                Default::default(),
            )
            .map_err(|e| error("policy visible occurrence", e))?;
        require(
            occurrences.occurrences().len() == 1,
            "policy occurrence count",
            "requires one visible structural occurrence per event owner",
        )?;
        let occurrence = &occurrences.occurrences()[0];
        let retained = context(&plan, owner)?.samples();
        piece_evidence.push(serde_json::json!({
            "owner":owner,"visible_frames":occurrence.visible_root_frames,
            "visible_samples":[occurrence.visible_root_samples.start.0,
                occurrence.visible_root_samples.end.0],
            "complete_context_samples":[retained.start.0,retained.end.0],
        }));
        owners.push((
            occurrence.visible_root_samples.start.0,
            occurrence.visible_root_samples.end.0,
            owner.clone(),
        ));
    }
    owners.sort();
    retain(directory, "policy-pieces.json", &piece_evidence)?;
    let seam = frame_sample_boundary(11);
    require(
        owners.len() == 2
            && owners[0].0 == BIRTH_FIRST
            && owners[0].1 == seam
            && owners[1].0 == seam
            && owners[1].1 == BIRTH_END,
        "policy pieces",
        format!("requires visible[9,11),[11,13) pieces; actual={piece_evidence:?}"),
    )?;
    let owner = owners[0].2.clone();
    let original_event = &before.beat_sounds()[&owner][&sound_id("effect")];
    require(
        original_event.treatments.is_empty()
            && original_event.start_edge == AudioEdgePolicy::Hard
            && original_event.end_edge == AudioEdgePolicy::Hard,
        "policy event",
        "independent trim oracle requires neutral Hard event policy",
    )?;
    let mut event = original_event.clone();
    event.label.push_str(" label-only PCM witness");
    let renamed = command(
        &before,
        "family-policy-label",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: sound_id("effect"),
            event: event.clone(),
        },
    )?;
    event.treatments = event
        .treatments
        .with_trim(GainDb::new(-3000).unwrap())
        .unwrap();
    let trimmed = command(
        &renamed,
        "family-policy-trim",
        Command::SetBeatSound {
            owner,
            id: sound_id("effect"),
            event,
        },
    )?;
    retain(directory, "policy-before.json", &before)?;
    retain(directory, "policy-renamed.json", &renamed)?;
    retain(directory, "policy-trimmed.json", &trimmed)?;
    require(
        before.audio_bindings() == renamed.audio_bindings()
            && before.audio_bindings() == trimmed.audio_bindings(),
        "policy journals",
        "label/trim setter must preserve every retained table and reference",
    )?;

    let first = BIRTH_FIRST - 2048;
    let end = BIRTH_END + 2048;
    let total = frame_sample_boundary(15);
    let before_input = input(&before, &id("birth-g"), first, end, &[256])?;
    let renamed_input = input(&renamed, &id("birth-g"), first, end, &[37, 173, 256])?;
    let trimmed_input = input(&trimmed, &id("birth-g"), first, end, &[37, 173, 256])?;
    retain(
        directory,
        "policy-input-before.json",
        &input_evidence(&before_input),
    )?;
    retain(
        directory,
        "policy-input-renamed.json",
        &input_evidence(&renamed_input),
    )?;
    retain(
        directory,
        "policy-input-trimmed.json",
        &input_evidence(&trimmed_input),
    )?;
    let label_raw = super::split::raw_comparison(
        first,
        &before_input.samples,
        &before_input.suppressed,
        &renamed_input,
    );
    // read_beat_sound_owner_input documents current event gain before any owner
    // treatments. Verify that contract against an independent per-fragment
    // multiplier before using this public G input in the Equalizer oracle.
    let gain = 10.0_f64.powf(-3000.0 / 20_000.0);
    let gain_expected = before_input
        .samples
        .iter()
        .enumerate()
        .map(|(index, value)| {
            if first + (index as i64) < seam {
                value.map(|v| v * gain)
            } else {
                *value
            }
        })
        .collect::<Vec<_>>();
    let trim_raw = super::split::raw_comparison(
        first,
        &gain_expected,
        &before_input.suppressed,
        &trimmed_input,
    );
    let full_nominal_permission = trimmed_input.suppressed
        [(BIRTH_FIRST - first) as usize..(BIRTH_END - first) as usize]
        .iter()
        .all(|v| !v);
    let input_partitions = super::split::raw_comparison(
        first,
        &trimmed_input.samples,
        &trimmed_input.suppressed,
        &input(&trimmed, &id("birth-g"), first, end, &[256])?,
    );

    let before_pcm = read_bus(&before, 0, total, &[1021])?;
    let label_pcm = read_bus(&renamed, 0, total, &[37, 509, 1021, 173])?;
    let trim_pcm = read_bus(&trimmed, 0, total, &[37, 509, 1021, 173])?;
    let recipe = trimmed.nodes()[&id("birth-g")]
        .audio_treatments
        .equalizer()
        .unwrap();
    // One public Equalizer processes the COMPLETE summed G input, after each
    // fragment's independent current gain. No per-piece isolated EQ is an oracle.
    let reference_pcm = super::split::equalize_delivered(
        &before_input,
        first,
        0,
        BIRTH_FIRST,
        BIRTH_END,
        total,
        recipe,
    )?;
    let reference_comparison = differences32(&reference_pcm, &before_pcm);
    let expected_pcm = super::split::equalize_delivered(
        &trimmed_input,
        first,
        0,
        BIRTH_FIRST,
        BIRTH_END,
        total,
        recipe,
    )?;
    let label_comparison = differences32(&before_pcm, &label_pcm);
    let trim_comparison = differences32(&expected_pcm, &trim_pcm);
    let mut partitions = Vec::new();
    let mut cold = Vec::new();
    for (name, document, pcm) in [
        ("before", &before, &before_pcm),
        ("renamed", &renamed, &label_pcm),
        ("trimmed", &trimmed, &trim_pcm),
    ] {
        let fresh_partition: &[u32] = if name == "before" {
            &[37, 509, 1021, 173]
        } else {
            &[1021]
        };
        partitions.push(
            serde_json::json!({"side":name,"comparison":differences32(pcm,
            &read_bus(document,0,total,fresh_partition)?)}),
        );
        for at in [BIRTH_FIRST - 1, seam - 1, BIRTH_END - 1, total - 3] {
            let fresh = read_bus(document, at, at + 3, &[3])?;
            cold.push(serde_json::json!({"side":name,"first":at,
                "comparison":differences32(&pcm[at as usize..at as usize+3],&fresh)}));
        }
    }
    let no_eq = command(
        &trimmed,
        "family-policy-no-eq",
        Command::SetAudioTreatments {
            node: id("birth-g"),
            treatments: AudioTreatments::default(),
        },
    )?;
    let no_events = remove_events(&trimmed, "family-policy-no-event")?;
    let no_eq_pcm = read_bus(&no_eq, 0, total, &[1021])?;
    let no_events_pcm = read_bus(&no_events, 0, total, &[1021])?;
    let active = [
        super::split::active(&before_pcm, &trim_pcm),
        super::split::active(&trim_pcm, &no_eq_pcm),
        super::split::active(&trim_pcm, &no_events_pcm),
    ];
    let controls_valid = active.iter().all(|v| *v > 0)
        && reference_comparison["different_channel_samples"] == 0
        && full_nominal_permission
        && input_partitions["passed"] == true
        && partitions
            .iter()
            .all(|r| r["comparison"]["different_channel_samples"] == 0)
        && cold
            .iter()
            .all(|r| r["comparison"]["different_channel_samples"] == 0);
    Ok(serde_json::json!({
        "controls_valid":controls_valid,
        "passed":label_raw["passed"]==true && trim_raw["passed"]==true
            && before_input.support==renamed_input.support
            && before_input.support==trimmed_input.support
            && label_comparison["different_channel_samples"]==0
            && trim_comparison["different_channel_samples"]==0,
        "label_raw":label_raw,"trim_raw_independent_gain":trim_raw,
        "untrimmed_public_input_G_eq_control":reference_comparison,
        "label_neutral_full_pcm":label_comparison,"trim_shared_G_eq_full_pcm":trim_comparison,
        "raw_input_partitions":input_partitions,"partitions":partitions,"cold":cold,
        "full_nominal_permission":full_nominal_permission,
        "active_samples":{"trim":active[0],"G_eq":active[1],"events":active[2]},
        "gain":gain,"seam":seam,
        "independent_gain_input_bits":gain_expected.iter().map(|s|s.map(f64::to_bits)).collect::<Vec<_>>(),
        "untrimmed_oracle_pcm_bits":reference_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "before_pcm_bits":before_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "renamed_pcm_bits":label_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "expected_trim_pcm_bits":expected_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "actual_trim_pcm_bits":trim_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "no_eq_pcm_bits":no_eq_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "no_event_pcm_bits":no_events_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "executed_by_draft_author":false,"independent_dsp_claim":false,
    }))
}
fn evidence(name: &str) -> std::path::PathBuf {
    let path = std::env::var_os("DEADPAN_SCOPE_PROBE_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("deadpan-family-{}", std::process::id()))
        })
        .join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn split_permission_family_is_genuine_local_ordered_and_recipe_specific() {
    let directory = evidence("new-group-birth-family-authority");
    let result = run_authority(&directory).unwrap_or_else(|failure| {
        serde_json::json!({
            "passed":false,"failure":failure,"classification":"setup or API failure",
        })
    });
    retain(&directory, "result.json", &result).unwrap();
    eprintln!(
        "permission-family authority: {}\n{result}",
        directory.display()
    );
    assert!(
        result["passed"] == true,
        "permission-family evidence retained"
    );
}
#[test]
fn neutral_split_keeps_shared_equalizer_and_saturation_pcm() {
    let directory = evidence("new-group-birth-family-pcm");
    let result=run_pcm(&directory).unwrap_or_else(|failure|serde_json::json!({
        "controls_valid":false,"passed":false,"failure":failure,"classification":"setup or API failure",
    }));
    retain(&directory, "result.json", &result).unwrap();
    eprintln!(
        "permission-family PCM: {}\ncontrols_valid={} passed={} failure={}",
        directory.display(),
        result["controls_valid"],
        result["passed"],
        result["failure"]
    );
    assert!(
        result["controls_valid"] == true,
        "nonlinear shared-input controls not established"
    );
    assert!(
        result["passed"] == true,
        "nonlinear Split PCM evidence retained"
    );
}

#[test]
fn split_family_label_is_pcm_neutral_and_fragment_trim_precedes_shared_g_eq() {
    let directory = evidence("new-group-birth-family-live-policy");
    let result = run_live_policy_pcm(&directory).unwrap_or_else(|failure| {
        serde_json::json!({
            "controls_valid":false,"passed":false,"failure":failure,
            "classification":"setup or API failure",
        })
    });
    retain(&directory, "result.json", &result).unwrap();
    eprintln!(
        "live family policy: {} controls_valid={} passed={} failure={}",
        directory.display(),
        result["controls_valid"],
        result["passed"],
        result["failure"]
    );
    assert!(
        result["controls_valid"] == true,
        "live-policy controls not established"
    );
    assert!(
        result["passed"] == true,
        "label/trim family PCM evidence retained"
    );
}
