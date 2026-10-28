//! Reuse checked fixture constructors from audio_owners/tests.rs.
use super::*;

fn tone(frames: i64) -> BeatNode {
    let mut node = hold(frames);
    let NodeKind::Hold { recipe } = &mut node.kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::Tone {
        frequency_hz: 440,
        level: GainDb::new(-6000).unwrap(),
    };
    node.audio_editorial_edges = AudioEditorialEdges {
        start: true,
        end: true,
    };
    node
}
fn owner<'plan>(plan: &'plan RenderPlan, name: &str, at: i64) -> AudioOwnerClock<'plan> {
    plan.audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap()
        .spans()[0]
        .owners()
        .iter()
        .find(|v| v.instance.node.as_str() == name)
        .unwrap()
        .clone()
}
fn fixture_json(name: &str, doc: &ProjectDocument, cases: Vec<Value>, extra: Value) -> Value {
    json!({"fixture":name,"document":serde_json::to_value(doc).unwrap(),"cases":cases,"witnesses":extra})
}
fn unbound() -> Value {
    let doc = document(48_000, &["tone"], vec![("tone", tone(512))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let clock = owner(&plan, "tone", 128);
    let input = plan
        .audio_owner_input(&clock, AudioSample(128), Default::default())
        .unwrap();
    let parent = Parent::Domain(input.context());
    let domain_errors = json!({
      "invalid_range":query_result(parent.query(AudioSample(0)..AudioSample(513),Default::default()),parent.plan(),0),
      "reversed_range_after_seed_copy":query_result(parent.query(AudioSample(2)..AudioSample(1),Default::default()),parent.plan(),0),
      "invalid_limits_after_seed_copy":query_result(parent.query(AudioSample(128)..AudioSample(129),
          AudioQueryLimits{maximum_spans:0,..Default::default()}),parent.plan(),0)
    });
    let other = RenderPlan::compile(&doc).unwrap();
    let foreign = other.audio_owner_input(&clock, AudioSample(128), Default::default());
    assert!(!clock.belongs_to(&other));
    fixture_json(
        "unbound_sequence_tone_and_actual_domain",
        &doc,
        vec![
            case(
                "unbound_root",
                Parent::Root(&plan),
                AudioSample(0)..AudioSample(512),
                AudioSample(128),
                "root",
                AudioOwnerKind::Node,
            ),
            case(
                "unbound_tone",
                Parent::Root(&plan),
                AudioSample(0)..AudioSample(512),
                AudioSample(128),
                "tone",
                AudioOwnerKind::Node,
            ),
            case(
                "captured_tone_domain",
                parent,
                AudioSample(0)..AudioSample(512),
                AudioSample(128),
                "tone",
                AudioOwnerKind::Node,
            ),
        ],
        json!({"domain_errors":domain_errors,"foreign_plan":input_result(foreign,0),
      "foreign_owner_belongs_to_other":false}),
    )
}
fn repeated() -> Value {
    // Existing repeat_play_and_gap_overrides_keep_live_owners_and_stable_scope.
    let base = document(
        48_000,
        &["repeat"],
        vec![("voice", tone(4)), ("repeat", repeat("voice", 3, 2))],
    );
    let play = |ordinal| IterationId {
        allocation: revision("plays"),
        ordinal,
    };
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["alternate"] = serde_json::to_value(tone(3)).unwrap();
    wire["nodes"]["gap-group"] = serde_json::to_value(sequence(&["gap-voice"])).unwrap();
    wire["nodes"]["gap-voice"] = serde_json::to_value(tone(1)).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("alternate"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(0),
            root: id("gap-group"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let first = owner(&plan, "voice", 0);
    let late = owner(&plan, "voice", 10);
    let whole0 = owner(&plan, "repeat", 0);
    let whole1 = owner(&plan, "repeat", 5);
    assert_eq!(coarse_clock(&whole0), coarse_clock(&whole1));
    assert_ne!(
        clock(&whole0, &plan, |v| v.0),
        clock(&whole1, &plan, |v| v.0)
    );
    let wrong = plan.audio_owner_input(&first, AudioSample(10), Default::default());
    assert!(wrong.is_err());
    assert_ne!(first.instance(), late.instance());
    let end = plan.audio_duration().unwrap();
    fixture_json(
        "stable_repeat_plays_default_gap_and_overrides",
        &doc,
        vec![
            case(
                "repeat_first",
                Parent::Root(&plan),
                AudioSample(0)..end,
                AudioSample(0),
                "repeat",
                AudioOwnerKind::Node,
            ),
            case(
                "repeat_override",
                Parent::Root(&plan),
                AudioSample(0)..end,
                AudioSample(5),
                "repeat",
                AudioOwnerKind::Node,
            ),
            case(
                "alternate_play",
                Parent::Root(&plan),
                AudioSample(0)..end,
                AudioSample(5),
                "alternate",
                AudioOwnerKind::Node,
            ),
            case(
                "overridden_gap",
                Parent::Root(&plan),
                AudioSample(0)..end,
                AudioSample(4),
                "gap-group",
                AudioOwnerKind::Node,
            ),
            case(
                "default_gap",
                Parent::Root(&plan),
                AudioSample(0)..end,
                AudioSample(8),
                "repeat",
                AudioOwnerKind::DefaultGap,
            ),
            case(
                "last_voice",
                Parent::Root(&plan),
                AudioSample(0)..end,
                AudioSample(10),
                "voice",
                AudioOwnerKind::Node,
            ),
        ],
        json!({"coarse_equal_whole_repeat":true,"whole_repeat_first":clock(&whole0,&plan,|v|v.0),
       "whole_repeat_override":clock(&whole1,&plan,|v|v.0),"wrong_repeat_branch":input_result(wrong,0)}),
    )
}
fn hidden_resumed() -> Value {
    // Literal fixture from retained_binding_is_not_reconstructed_from_its_nominal_descendant_clock.
    let nodes = || {
        vec![
            ("a", tone(2)),
            ("b", tone(2)),
            ("group", sequence(&["a", "b"])),
            (
                "stage",
                retime("group", 8, 0..4, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ]
    };
    let old = document(48_000, &["stage"], nodes());
    let mut moved = nodes();
    moved.push(("prefix", tone(1)));
    let current = document(48_000, &["prefix", "stage"], moved);
    let doc = bind(
        &current,
        &old,
        "stage",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(3, 1)),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let live = owner(&plan, "group", 1);
    let input = plan
        .audio_owner_input(&live, AudioSample(1), Default::default())
        .unwrap();
    let retained = input
        .retained_at(AudioSample(1), Default::default())
        .unwrap()
        .unwrap();
    let AudioBoundDomain::Root(reference) = retained.domain() else {
        panic!("actual retained RootRoundEven fixture")
    };
    let at = AudioSample(3);
    let seeded = reference
        .gain_owners(at..AudioSample(4), Default::default())
        .unwrap();
    let root = plan
        .audio_gain_owners(at..AudioSample(4), Default::default())
        .unwrap();
    let seeded_json = query(&seeded, reference.plan, |v| v.0);
    let root_json = query(&root, &plan, |v| v.0);
    assert_ne!(
        seeded_json, root_json,
        "a fake root substitution MUST fail this oracle"
    );
    let selected = seeded.spans()[0]
        .owners()
        .iter()
        .find(|v| v.instance.node.as_str() == "group")
        .unwrap();
    let substitute = plan.audio_owner_input(selected, at, Default::default());
    assert!(
        substitute.is_err(),
        "actual seeded member cannot be reconstructed from live root"
    );
    let live_at = root.spans()[0]
        .owners()
        .iter()
        .find(|v| v.instance.node.as_str() == "group")
        .unwrap();
    assert_eq!(
        coarse_clock(selected),
        coarse_clock(live_at),
        "coarse owner identity deliberately collides"
    );
    assert_ne!(
        clock(selected, reference.plan, |v| v.0),
        clock(live_at, &plan, |v| v.0)
    );
    fixture_json(
        "hidden_resumed_retained_domain_vs_live_root",
        &doc,
        vec![
            case(
                "live_bound_group",
                Parent::Root(&plan),
                AudioSample(1)..AudioSample(9),
                AudioSample(1),
                "group",
                AudioOwnerKind::Node,
            ),
            case(
                "actual_hidden_reference_group",
                Parent::Domain(reference),
                AudioSample(0)..AudioSample(8),
                at,
                "group",
                AudioOwnerKind::Node,
            ),
        ],
        json!({"historical_document":serde_json::to_value(old).unwrap(),
       "same_numerical_window":{"start":3,"end":4},"actual_seeded_query":seeded_json,
       "incorrect_root_query":root_json,"different_literal_results":true,"equal_coarse_owner_identity":true,
       "seeded_owner":clock(selected,reference.plan,|v|v.0),"root_owner":clock(live_at,&plan,|v|v.0),
       "root_substitution":input_result(substitute,0)}),
    )
}
fn source(rate: u32, duration: i64) -> (BeatNode, AssetRecord) {
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, rate).unwrap(),
        },
        SourceTimestamp {
            ticks: i64::from(rate),
            time_base: SourceTimeBase::new(1, rate).unwrap(),
        },
    )
    .unwrap();
    let mut node = tone(duration);
    node.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(duration),
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(SourceAudio {
                asset: AssetId::new("baseline-media").unwrap(),
                span,
            }),
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
            edit_window: None,
        },
    };
    (
        node,
        AssetRecord {
            label: "Planning metadata only".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: None,
        },
    )
}
fn source_document(
    rate: FrameRate,
    children: &[&str],
    nodes: Vec<(&str, BeatNode)>,
    asset: AssetRecord,
) -> ProjectDocument {
    // Build source-bearing wire before validation; no fake provider/PCM authority.
    let base = ProjectDocument::new(
        ProjectId::new("owner-source-baseline").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(base).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(id("root"), sequence(children));
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"]["baseline-media"] = serde_json::to_value(asset).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn point_ceil_shift() -> Value {
    // Existing moved_preserve_input_keeps_its_point_ceil_reference_labels,
    // with an actual Source(44.1kHz clock) and fixed Shift processing variant.
    let (voice, asset) = source(44_100, 2);
    let old = source_document(
        FrameRate::new(32_000, 1).unwrap(),
        &["stage"],
        vec![
            ("prefix", tone(1)),
            ("voice", voice.clone()),
            ("group", sequence(&["prefix", "voice"])),
            (
                "stage",
                retime(
                    "group",
                    6,
                    0..3,
                    PitchPolicy::Shift { semitones: 3 },
                    RetimePurpose::Edit,
                ),
            ),
        ],
        asset.clone(),
    );
    let current = source_document(
        FrameRate::new(32_000, 1).unwrap(),
        &["voice"],
        vec![("voice", voice)],
        asset,
    );
    let doc = bind(
        &current,
        &old,
        "voice",
        AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
        None,
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let clock = owner(&plan, "voice", 0);
    let input = plan
        .audio_owner_input(&clock, AudioSample(0), Default::default())
        .unwrap();
    let retained = input
        .retained_at(AudioSample(0), Default::default())
        .unwrap()
        .unwrap();
    assert!(matches!(retained.domain(), AudioBoundDomain::Point(_)));
    fixture_json(
        "nonunity_shift_fractional_point_ceil_source44100",
        &doc,
        vec![case(
            "retained_point_voice",
            Parent::Root(&plan),
            AudioSample(0)..plan.audio_duration().unwrap(),
            AudioSample(0),
            "voice",
            AudioOwnerKind::Node,
        )],
        json!({"historical_document":serde_json::to_value(old).unwrap(),"source_time_base":{"numerator":1,"denominator":44100},
       "media_opened":false,"qualified_provider_admission_claimed":false}),
    )
}
// audio_reference::source_prefix_behind_partition_keeps_unbound_and_retained_source_phase
// explicitly captures/installs a lattice; its bare Source-Split reference-plan
// test does NOT promise a RenderPlan Bound input. Keep these variants distinct.
fn split_documents(install_lattice: bool) -> (ProjectDocument, ProjectDocument, ProjectDocument) {
    let (voice, asset) = source(48_000, 6);
    let original = source_document(
        FrameRate::new(48_000, 1).unwrap(),
        &["voice"],
        vec![("voice", voice)],
        asset,
    );
    let before = if install_lattice {
        let state = capture_unbound_audio_bindings(
            &original,
            AudioTimingId {
                allocation: revision("baseline-pre-split-lattice"),
                ordinal: 0,
            },
        )
        .unwrap();
        let mut wire = serde_json::to_value(&original).unwrap();
        wire["audio_bindings"] = serde_json::to_value(state).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    } else {
        original.clone()
    };
    let result = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("baseline-split"),
            command: Command::Split {
                node: id("voice"),
                at: frames(2),
                identities: SplitIdentities {
                    nodes: (0..12).map(|n| id(&format!("baseline-cut-{n}"))).collect(),
                },
            },
        },
    )
    .unwrap();
    let split = result.forward.apply(&before).unwrap();
    (original, before, split)
}
fn split_owner_rows(plan: &RenderPlan, at: AudioSample) -> Vec<Value> {
    let observed = plan
        .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
        .unwrap();
    observed.spans()[0]
        .owners()
        .iter()
        .enumerate()
        .map(|(index, member)| {
            let captured = plan
                .audio_owner_input(member, at, Default::default())
                .unwrap();
            json!({
                "member_index":index,
                "observed_owner":clock(member, plan, |v|v.0),
                "captured_input":input(&captured),
                "actual_retained_probe":retained_probe(&captured)
            })
        })
        .collect()
}
fn split_root() -> Value {
    let (original, before, doc) = split_documents(true);
    let plan = RenderPlan::compile(&doc).unwrap();
    let at = AudioSample(3);
    let observed = plan
        .audio_gain_owners(at..AudioSample(4), Default::default())
        .unwrap();
    // Select from ACTUAL old observations/probes, not a root/last/key assumption.
    // The resulting member index is frozen in SnapshotV1 before any new API.
    let mut selected = None;
    for (index, member) in observed.spans()[0].owners().iter().enumerate() {
        let captured = plan
            .audio_owner_input(member, at, Default::default())
            .unwrap();
        let probe = captured.retained_probe_at(at, Default::default()).unwrap();
        if probe
            .input()
            .is_some_and(|value| matches!(value.domain(), AudioBoundDomain::Root(_)))
        {
            selected = Some((index, captured));
            break;
        }
    }
    let (selected_index, captured) =
        selected.expect("installed pre-Split lattice must expose a checked retained Root input");
    let member = &observed.spans()[0].owners()[selected_index];
    let alias = member.instance.node.as_str();
    let rows = split_owner_rows(&plan, at);
    fixture_json(
        "actual_source_split_with_preexisting_lattice_retained_root_round_even_clipped_window",
        &doc,
        vec![
            case(
                "split_verified_retained_owner_clipped",
                Parent::Root(&plan),
                AudioSample(2)..AudioSample(5),
                at,
                alias,
                member.kind,
            ),
            case(
                "split_verified_retained_owner_actual_domain",
                Parent::Domain(captured.context()),
                captured.context().visible_samples(),
                at,
                alias,
                member.kind,
            ),
        ],
        json!({
            "unbound_pre_split_document":serde_json::to_value(original).unwrap(),
            "bound_pre_split_document":serde_json::to_value(before).unwrap(),
            "public_source_split_command":true,
            "explicit_pre_split_lattice_capture":true,
            "selected_verified_old_member_index":selected_index,
            "actual_old_owner_rows":rows,
            "retained_domain_grid":"RootRoundEven",
            "current_variant_label_required":true
        }),
    )
}

#[test]
#[ignore = "bounded legacy fixture diagnosis only; no expected golden"]
fn diagnose_old_split_owner_inputs_v1() {
    let output = std::path::PathBuf::from(
        std::env::var_os("DEADPAN_OWNER_SPLIT_DIAGNOSTIC_OUTPUT")
            .expect("explicit /tmp diagnostic path required"),
    );
    assert!(
        output.is_absolute() && (output.starts_with("/tmp") || output.starts_with("/private/tmp"))
    );
    let variant = std::env::var("DEADPAN_OWNER_SNAPSHOT_VARIANT").expect("variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source =
        std::env::var("DEADPAN_OWNER_SNAPSHOT_SOURCE_SHA256").expect("source authority required");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let rows = [false, true].map(|bound| {
        let (_, before, doc) = split_documents(bound);
        let plan = RenderPlan::compile(&doc).unwrap();
        json!({"explicit_pre_split_lattice":bound,
            "before_document":serde_json::to_value(before).unwrap(),
            "split_document":serde_json::to_value(doc).unwrap(),
            "observed_window":{"start":3,"end":4},
            "old_owner_rows":split_owner_rows(&plan, AudioSample(3))})
    });
    let result = json!({"schema":"LegacySplitOwnerDiagnosticV1","variant":variant,
        "runtime_source_manifest_sha256":source,"variants":rows});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &result).unwrap();
    use std::io::Write;
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
pub(super) fn matrix() -> Vec<Value> {
    vec![
        unbound(),
        repeated(),
        hidden_resumed(),
        point_ceil_shift(),
        split_root(),
    ]
}
