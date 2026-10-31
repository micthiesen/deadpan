//! Additional ordinary OLD policy observations over existing validated timelines.
use super::*;

fn range_value(samples: Range<AudioSample>) -> Value {
    json!({"start":samples.start.0,"end":samples.end.0})
}

fn range_from(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().expect("sample start"))
        ..AudioSample(value["end"].as_i64().expect("sample end"))
}

fn source_descriptor(
    name: &str,
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    source: Value,
) -> Value {
    let (parent_kind, parent_identity) = match parent {
        Parent::Root(plan) => (
            "Root",
            json!({
                "tag":"ImmutablePlanRoot",
                "project_id":plan.metadata.project_id.as_str(),
                "revision_id":plan.metadata.revision_id.as_str(),
                "root":plan.nodes[plan.root].inspection.id.as_str(),
            }),
        ),
        Parent::Domain(physical) => (
            "Domain",
            json!({
                "tag":"ActualCheckedPhysicalDomain",
                "physical_domain":super::domain(physical),
            }),
        ),
    };
    json!({
        "name":name,
        "parent_kind":parent_kind,
        "parent_identity":parent_identity,
        "window":range_value(samples),
        "fixture_source":source,
    })
}

/// Revisit only checked parents that carry the requested retained clocks,
/// repeat-gap, fractional, or split/source semantics. Their existing checked
/// documents and setup provenance remain embedded in the descriptors.
pub(in crate::plan) fn visit_cases(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let mut visited = 0;
    let retained_names = [
        "wide/repeat_actual_default_gap_domain_full/domain",
        "wide/hidden_resumed_actual_capture_domain/domain",
        "wide/hidden_actual_retained_reference_domain/domain",
        "wide/fractional_actual_physical_capture_domain/domain",
        "wide/split_actual_physical_capture_domain/domain",
    ];
    super::super::processing_query_baseline::visit_cases(|parent, original| {
        let Some(name) = original["name"].as_str() else {
            return;
        };
        if !retained_names.contains(&name) {
            return;
        }
        let mut value = original.clone();
        value["name"] = json!(format!("retained_policy/{name}"));
        value["requested_observation"] =
            json!("literal default and bounded edge observations from matrix()");
        visited += 1;
        visit(parent, &value);
    });

    // Existing independently checked Preserve/Shift and signed Partition
    // constructors. The visitor's document, owner input and physical Domain
    // are all the actual values used for the measured policy call.
    super::super::policy_binding_baseline::visit_additional(|name, document, input, physical| {
        if !matches!(
            name,
            "exposed_preserve_and_silent_hold"
                | "shift_at_unity_and_silent_hold"
                | "inside_preserve_actual_group_domain"
                | "opaque_bound_stage_with_preserve_and_silence"
                | "cropped_group_domain_with_inherited_edit_retime"
                | "hidden_signed_partition_group_with_unrelated_binding"
        ) {
            return;
        }
        let samples = physical.root_samples();
        let parent = Parent::Domain(physical);
        let descriptor = source_descriptor(
            &format!("retained_policy/additional/{name}/domain"),
            parent,
            samples,
            json!({
                "kind":"checked_additional_owner_input",
                "path":"src/audio_owners/legacy_snapshot_v1/policy_binding_baseline.rs",
                "symbol":"visit_additional",
                "case":name,
                "document":document,
                "setup_input":super::input(input),
            }),
        );
        visited += 1;
        visit(parent, &descriptor);
    });
    assert_eq!(visited, 11, "retained OLD policy fixture coverage changed");
}

/// Direct one-sample and adjacent-page calls cannot use ordinary_case's
/// clipped-range matrix (which intentionally requires at least two samples).
/// Each returned observation is a literal old API result with complete private
/// policy payload, lookup, work and any actual Domain second-query events.
pub(in crate::plan) fn edge_observations(
    parent: Parent<'_, '_>,
    name: &str,
    samples: Range<AudioSample>,
) -> Value {
    assert!(samples.start < samples.end);
    let limits = AudioQueryLimits::default();
    let mut rows = Vec::new();
    let midpoint = AudioSample(samples.start.0 + (samples.end.0 - samples.start.0) / 2);
    let mut anchors = vec![samples.start, AudioSample(samples.end.0 - 1)];
    if midpoint > samples.start {
        anchors.push(AudioSample(midpoint.0 - 1));
    }
    anchors.sort_by_key(|sample| sample.0);
    anchors.dedup();
    for sample in anchors {
        let page = sample..AudioSample(sample.0.checked_add(1).expect("one-sample edge window"));
        let label = format!(
            "{name}/sample_{}_parity_{}",
            sample.0,
            sample.0.rem_euclid(2)
        );
        rows.push(
            call(parent, Route::Ordinary, page.clone(), limits).observation(&label, page, limits),
        );
    }
    if samples.start < midpoint && midpoint < samples.end {
        for (label, page) in [
            (format!("{name}/page_before_seam"), samples.start..midpoint),
            (format!("{name}/page_after_seam"), midpoint..samples.end),
        ] {
            rows.push(
                call(parent, Route::Ordinary, page.clone(), limits)
                    .observation(&label, page, limits),
            );
        }
    }
    json!({
        "name":name,
        "window":range_value(samples),
        "purpose":"literal one-sample and adjacent-page OLD policy calls; no result is synthesized or normalized",
        "observations":rows,
    })
}

/// The selected source/retime document mirrors the checked source-endpoint
/// scenario. It starts from actual public types, installs an actual captured
/// lattice and source-endpoint reanchor, then runs ProjectDocument validation and
/// RenderPlan compilation before producing the query parent.
pub(in crate::plan) fn source_endpoint_parent(
    endpoint: AudioSourceEndpoint,
) -> (ProjectDocument, RenderPlan) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let base = ProjectDocument::new(
        ProjectId::new("policy-source-endpoint").unwrap(),
        RevisionId::new("old").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )
    .unwrap();
    let hold = |label: &str, duration: i64| {
        BeatNode::hold(
            label,
            HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(duration).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    };
    let id = |value: &str| NodeId::new(value).unwrap();
    let source = BeatNode {
        label: "Source".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: FrameDuration::new(6).unwrap(),
                edit_window: None,
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("endpoint-media").unwrap(),
                    span,
                }),
                audio_mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
                audio_offset: AudioSample(7),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let crop = |start: i64, end: i64| BeatNode {
        label: "Crop".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("source"),
            duration: FrameDuration::new(end - start).unwrap(),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let mut old_wire = serde_json::to_value(base).unwrap();
    old_wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("crop")])).unwrap();
    old_wire["nodes"]["lead"] = serde_json::to_value(hold("Lead", 5)).unwrap();
    old_wire["nodes"]["crop"] = serde_json::to_value(crop(0, 1)).unwrap();
    old_wire["nodes"]["source"] = serde_json::to_value(source).unwrap();
    old_wire["assets"]["endpoint-media"] = serde_json::to_value(AssetRecord {
        label: "Planning metadata only".into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(span),
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    let old = ProjectDocument::from_json(&old_wire.to_string()).unwrap();
    let captured = capture_unbound_audio_bindings(
        &old,
        AudioTimingId {
            allocation: RevisionId::new("endpoint-capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut bindings = captured.bindings().clone();
    let binding = bindings
        .get_mut(&id("source"))
        .expect("captured Source binding");
    binding
        .reanchors
        .push(AudioReanchorStep::for_source_endpoint(
            binding.lattice.clone(),
            endpoint,
        ));
    let state = AudioBindingState::new(
        captured
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
    )
    .unwrap();
    old_wire["nodes"]["crop"] = serde_json::to_value(crop(2, 3)).unwrap();
    old_wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let current = ProjectDocument::from_json(&old_wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&current).unwrap();
    (current, plan)
}

/// One actual Source endpoint/reanchor Root plus a physical Domain derived
/// from its checked owner input. Uses exact NTSC-oriented windows from the
/// existing source endpoint test, with one-sample and adjacent-page reads.
pub(in crate::plan) fn source_endpoint_observations() -> Value {
    let variants = [AudioSourceEndpoint::Start, AudioSourceEndpoint::End]
        .into_iter()
        .map(source_endpoint_variant_observations)
        .collect::<Vec<_>>();
    json!({"endpoint_variants":variants})
}

fn source_endpoint_variant_observations(endpoint: AudioSourceEndpoint) -> Value {
    let (endpoint_name, prefix, fixture_kind) = match endpoint {
        AudioSourceEndpoint::Start => (
            "Start",
            "source_endpoint_reanchor_start",
            "checked_fixture_variant_with_endpoint_changed",
        ),
        AudioSourceEndpoint::End => (
            "End",
            "source_endpoint_reanchor",
            "checked_port_of_source_endpoint_test",
        ),
    };
    let (document, plan) = source_endpoint_parent(endpoint);
    let root = Parent::Root(&plan);
    let root_samples = AudioSample(8008)..AudioSample(8264);
    let root_descriptor = source_descriptor(
        &format!("{prefix}/root"),
        root,
        root_samples.clone(),
        json!({
            "kind":fixture_kind,
            "path":"tests/audio_bindings/source_endpoints.rs",
            "symbol":"source_endpoint_anchor_projects_outside_final_crop_without_clamping",
            "document":document,
            "endpoint":endpoint_name,
            "binding":"captured source lattice plus AudioReanchorStep::for_source_endpoint",
        }),
    );
    let root_full = call(
        root,
        Route::Ordinary,
        root_samples.clone(),
        AudioQueryLimits::default(),
    )
    .observation(
        &format!("{prefix}/root_full"),
        root_samples.clone(),
        AudioQueryLimits::default(),
    );

    let observed = plan
        .audio_gain_owners(
            AudioSample(8008)..AudioSample(8009),
            AudioQueryLimits::default(),
        )
        .expect("checked actual source endpoint has an owner at its admitted sample");
    let owner = observed
        .spans()
        .iter()
        .flat_map(|span| span.owners())
        .find(|owner| owner.instance.node.as_str() == "source")
        .expect("actual Source owner");
    let input = plan
        .audio_owner_input(owner, AudioSample(8008), AudioQueryLimits::default())
        .expect("actual Source owner input");
    let physical = Parent::Domain(input.context());
    let domain_samples = AudioSample(8008)..AudioSample(8264);
    let domain_descriptor = source_descriptor(
        &format!("{prefix}/domain"),
        physical,
        domain_samples.clone(),
        json!({
            "kind":fixture_kind,
            "path":"tests/audio_bindings/source_endpoints.rs",
            "symbol":"source_endpoint_anchor_projects_outside_final_crop_without_clamping",
            "document":serde_json::to_value(&document).unwrap(),
            "setup_input":super::input(&input),
            "endpoint":endpoint_name,
        }),
    );
    let domain_full = call(
        physical,
        Route::Ordinary,
        domain_samples.clone(),
        AudioQueryLimits::default(),
    )
    .observation(
        &format!("{prefix}/domain_full"),
        domain_samples.clone(),
        AudioQueryLimits::default(),
    );
    let seam_windows = [
        ("before_crop_start", AudioSample(8007)..AudioSample(8008)),
        ("at_crop_start", AudioSample(8008)..AudioSample(8009)),
        ("last_crop_sample", AudioSample(9609)..AudioSample(9610)),
        ("at_crop_end", AudioSample(9610)..AudioSample(9611)),
    ];
    let seam_observations = seam_windows
        .iter()
        .flat_map(|(label, page)| {
            [
                call(
                    root,
                    Route::Ordinary,
                    page.clone(),
                    AudioQueryLimits::default(),
                )
                .observation(
                    &format!("{prefix}/root_{label}"),
                    page.clone(),
                    AudioQueryLimits::default(),
                ),
                call(
                    physical,
                    Route::Ordinary,
                    page.clone(),
                    AudioQueryLimits::default(),
                )
                .observation(
                    &format!("{prefix}/domain_{label}"),
                    page.clone(),
                    AudioQueryLimits::default(),
                ),
            ]
        })
        .collect::<Vec<_>>();
    json!({
        "endpoint":endpoint_name,
        "fixture_kind":fixture_kind,
        "fixture_source":[root_descriptor,domain_descriptor],
        "fresh_limit_matrices":{
            "root":fresh_limit_observations(root,Route::Ordinary,root_samples.clone()),
            "domain":fresh_limit_observations(physical,Route::Ordinary,domain_samples.clone())
        },
        "observations":[root_full,domain_full,
            edge_observations(root,&format!("{prefix}/root_edges"),root_samples),
            edge_observations(physical,&format!("{prefix}/domain_edges"),domain_samples)],
        "exact_endpoint_seams":seam_observations,
    })
}

/// One bounded capture group for integration into the parent OLD producer.
/// Each default query is recorded literally, including typed errors and actual
/// input limits; no success assumption or work normalization is made here.
pub(in crate::plan) fn matrix() -> Value {
    let mut cases = Vec::new();
    visit_cases(|parent, descriptor| {
        let samples = range_from(&descriptor["window"]);
        let limits = AudioQueryLimits::default();
        let default = call(parent, Route::Ordinary, samples.clone(), limits).observation(
            "dedicated_default_policy",
            samples.clone(),
            limits,
        );
        let edges = edge_observations(parent, "dedicated_sample_and_page_edges", samples);
        cases.push(json!({"descriptor":descriptor,"default":default,"edges":edges}));
    });
    assert_eq!(cases.len(), 11);
    json!({
        "schema":"old-retained-policy-cases-v1",
        "scope":"additional actual ordinary Root/Domain policy calls on checked retained-history, resume, gap, Preserve/Shift, signed/fractional and source-endpoint inputs",
        "limitations":[
            "one-sample and adjacent-page rows are literal separate calls; they are not merged into a synthetic whole result",
            "default typed errors are retained without forcing success",
            "source endpoint builder is a port of the existing checked source_endpoints test and is validated again as a ProjectDocument and RenderPlan",
            "ordinary API observations only; construction admission is not exercised"
        ],
        "cases":cases,
        "source_endpoint":source_endpoint_observations(),
    })
}
