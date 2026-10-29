//! Ignored producer for a frozen legacy caller-side Description oracle.
//!
//! This intentionally snapshots only public checked observations available to
//! deadpan-audio. It is not an oracle for private Plan provider internals.
use super::*;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

fn checked_output(path: &std::path::Path) -> PathBuf {
    assert!(path.is_absolute(), "oracle output path must be absolute");
    let parent = path
        .parent()
        .expect("output parent")
        .canonicalize()
        .unwrap();
    let scratch = std::path::Path::new("/tmp").canonicalize().unwrap();
    assert!(
        parent.starts_with(scratch),
        "oracle output must stay under canonical /tmp"
    );
    parent.join(path.file_name().expect("output filename"))
}

fn legacy_description_snapshot() -> Value {
    description_fixture_snapshot::<false>()
}

fn description_fixture_snapshot<const M: bool>() -> Value {
    let mut cases = Vec::new();

    // The current Root and actual checked Domain fixture share one established
    // plan/owner but each invokes the inherited describe path exactly once.
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let at = AudioSample(128);
    let stage = fixture::stage(&owner);
    let input = plan
        .audio_owner_input(&owner, at, Default::default())
        .unwrap();
    for (name, original) in [
        ("root", Context::Root),
        (
            "actual_domain",
            Context::Domain(Arc::new(input.context().clone())),
        ),
    ] {
        let incoming = Contexts {
            original,
            sounds: Sounds::Root,
        };
        cases.push(snapshot_case::<M>(
            name,
            &plan,
            request(&incoming, &owner, at, Temporal::Equalizer(stage), false),
            None,
        ));
    }

    // This is the established incoming retained fixture. The borrowed owner is
    // selected from the real one-sample query; describe itself is called once.
    let retained_plan = super::retained_plan();
    let initial = retained_plan
        .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
        .unwrap();
    let tone = initial.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "tone")
        .expect("retained fixture has tone owner");
    let retained_input = retained_plan
        .audio_owner_input(tone, at, Default::default())
        .unwrap()
        .retained_at(at, Default::default())
        .unwrap()
        .expect("retained fixture reaches Bound")
        .into_shared();
    let observed = retained_input
        .owners(at..AudioSample(at.0 + 1), Default::default())
        .unwrap();
    let retained_owner = observed.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "tone")
        .expect("retained fixture query has tone owner");
    let incoming = Contexts {
        original: Context::Retained(retained_input.clone()),
        sounds: Sounds::None,
    };
    cases.push(snapshot_case::<M>(
        "incoming_retained",
        &retained_plan,
        request(
            &incoming,
            retained_owner,
            at,
            Temporal::Wet(TailEffect::Reverb),
            true,
        ),
        Some(&retained_input),
    ));

    // A frozen, ordinary ProjectDocument gives the repeat's real default-gap
    // owner and its concrete play path. This is bounded to one sample.
    let repeat_snapshot: Value = serde_json::from_str(include_str!(
        "../../../deadpan-plan/tests/fixtures/owner-query-baseline-v1/snapshot.json"
    ))
    .expect("frozen owner query snapshot parses");
    let repeat_fixture = repeat_snapshot["fixtures"]
        .as_array()
        .expect("fixture list")
        .iter()
        .find(|entry| entry["fixture"] == "stable_repeat_plays_default_gap_and_overrides")
        .expect("repeat/gap fixture exists");
    let repeat_document = ProjectDocument::from_json(&repeat_fixture["document"].to_string())
        .expect("repeat fixture is a normal checked document");
    let repeat_plan =
        Arc::new(RenderPlan::compile(&repeat_document).expect("repeat plan compiles"));
    let gap_at = AudioSample(8);
    let gap_query = repeat_plan
        .audio_gain_owners(gap_at..AudioSample(gap_at.0 + 1), Default::default())
        .expect("one-sample repeat gap query");
    let gap_owner = gap_query.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.kind() == deadpan_plan::AudioOwnerKind::DefaultGap)
        .expect("repeat fixture has a default gap owner");
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::Root,
    };
    cases.push(snapshot_case::<M>(
        "normal_document_repeat_default_gap",
        &repeat_plan,
        request(
            &incoming,
            gap_owner,
            gap_at,
            Temporal::Wet(TailEffect::Delay),
            true,
        ),
        None,
    ));

    // Inspect the first repeated voice in that same checked document. This
    // freezes the concrete, nonempty RepeatInstance path of a normal owner.
    let voice_at = AudioSample(0);
    let voice_query = repeat_plan
        .audio_gain_owners(voice_at..AudioSample(voice_at.0 + 1), Default::default())
        .expect("one-sample first repeat voice query");
    let voice_owner = voice_query.spans()[0]
        .owners()
        .iter()
        .find(|clock| {
            clock.instance().node.as_str() == "voice" && !clock.instance().repeats.is_empty()
        })
        .expect("repeat fixture has a first-play voice with a concrete instance path");
    assert!(!voice_owner.instance().repeats.is_empty());
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::Root,
    };
    cases.push(snapshot_case::<M>(
        "normal_document_first_repeat_voice",
        &repeat_plan,
        request(
            &incoming,
            voice_owner,
            voice_at,
            Temporal::Wet(TailEffect::Delay),
            true,
        ),
        None,
    ));

    json!({
        "schema":"deadpan.audio.legacy-caller-description.v1",
        "snapshot_scope":{
            "source":"actual legacy describe return values and public checked getters",
            "private_plan_provider_state":"not exposed by the Audio dependency test module; not claimed",
            "repeat_branch":"exercises one actual DefaultGap owner and one first-play voice with a nonempty RepeatInstance path",
            "retained_origin_repeat":"not exercised",
            "work_receipts":"recorded separately as actual controller work; immutable owner-query receipt fixtures are untouched",
            "observation":"no raw addresses; retained identity uses same-Arc ptr_eq attestation",
            "retained_owner_query":"separate post-return inspection of the actual retained context; not the acceptance-query receipt or allocator evidence",
        },
        "cases":cases,
    })
}

// Each fixture invokes exactly one Description constructor. False preserves
// the frozen producer's legacy path; true uses the actual request admission.
fn snapshot_case<'plan, const M: bool>(
    name: &str,
    plan: &'plan Arc<RenderPlan>,
    request: node_description::Request<'_, 'plan>,
    expected_retained: Option<&deadpan_plan::SharedAudioOwnerRetainedInput<'plan>>,
) -> Value {
    let owner = request.owner;
    let at = request.at;
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let snapshot = if M {
        let before_work = work.borrow().plan_work;
        let mut renderer = StageAudio::new(Arc::clone(plan));
        let description = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            node_description::describe_admitted(plan, request, control, &mut admission)
                .expect("actual admitted description fixture succeeds")
        };
        assert!(work.borrow().sound_context_frames > 0, "{name}");
        assert!(work.borrow().plan_work > before_work, "{name}");
        let snapshot = description_snapshot(plan, &description, owner, at, expected_retained);
        let billed = work.borrow().plan_work;
        drop(description);
        assert_eq!(work.borrow().sound_context_frames, 0, "{name}");
        assert_eq!(work.borrow().plan_work, billed, "{name}: no work refund");
        snapshot
    } else {
        let description =
            describe(plan, request, control).expect("legacy description fixture succeeds");
        description_snapshot(plan, &description, owner, at, expected_retained)
    };
    json!({
        "case":name,
        "description":snapshot,
        "controller_work":controller_work(&work.borrow()),
    })
}

#[test]
fn oracle_output_rejects_parent_escape() {
    let rejected = std::panic::catch_unwind(|| {
        checked_output(std::path::Path::new("/tmp/../caller-oracle.json"))
    });
    assert!(rejected.is_err());
    assert_eq!(
        checked_output(std::path::Path::new("/tmp/caller-oracle.json"))
            .parent()
            .unwrap(),
        std::path::Path::new("/tmp").canonicalize().unwrap()
    );
}

fn ratio(value: ExactRatio) -> Value {
    serde_json::to_value(value).expect("ExactRatio serializes")
}

fn sample(value: AudioSample) -> Value {
    json!(value.0)
}

fn range(value: Range<AudioSample>) -> Value {
    json!({"start":value.start.0,"end":value.end.0})
}

fn temporal(kind: Temporal) -> Value {
    match kind {
        Temporal::Equalizer(stage) => json!({"tag":"Equalizer","stage":stage}),
        Temporal::Wet(effect) => json!({"tag":"Wet","effect":effect}),
    }
}

fn owner_fields(owner: &AudioOwnerClock<'_>, at: AudioSample) -> Value {
    let map = owner.sampling();
    let offsets = [-1_i64, 0, 1];
    let observed = offsets
        .into_iter()
        .map(|offset| {
            let position = AudioSample(at.0 + offset);
            json!({
                "sample": position.0,
                "local": ratio(map.local_at(position).expect("small adjacent map query")),
            })
        })
        .collect::<Vec<_>>();
    let origin = match owner.origin() {
        deadpan_plan::AudioOwnerClockOrigin::Current => json!({"tag":"Current"}),
        deadpan_plan::AudioOwnerClockOrigin::Retained {
            definition,
            instance,
            kind,
            gap_after,
        } => json!({
            "tag":"Retained",
            "definition":definition,
            "instance":instance,
            "kind":format!("{kind:?}"),
            "gap_after":gap_after,
        }),
    };
    json!({
        "instance":owner.instance(),
        "kind":format!("{:?}", owner.kind()),
        "definition":owner.definition(),
        "origin":origin,
        "gap_after":owner.gap_after(),
        "sampling":{
            "anchor":sample(map.anchor()),
            "local_at_anchor":ratio(map.local_at_anchor()),
            "local_frames_per_sample":ratio(map.local_frames_per_sample()),
            "observed_adjacent_boundaries":observed,
        },
    })
}

fn domain_fields(domain: &AudioDomain<'_>) -> Value {
    json!({
        "instance":domain.instance(),
        "root_samples":range(domain.root_samples()),
        "visible_samples":range(domain.visible_samples()),
    })
}

fn description_snapshot<I: Copy + Eq, L>(
    plan: &RenderPlan,
    description: &node_description::Description<'_, I, L>,
    selected_input_owner: &AudioOwnerClock<'_>,
    at: AudioSample,
    expected_retained: Option<&deadpan_plan::SharedAudioOwnerRetainedInput<'_>>,
) -> Value {
    let original = match &description.context.original {
        Context::Root => json!({"tag":"Root"}),
        Context::Domain(domain) => {
            json!({"tag":"Domain","checked":domain.belongs_to(plan),"value":domain_fields(domain)})
        }
        Context::Retained(actual) => {
            let mut reference = Vec::new();
            for offset in [-1_i64, 0, 1] {
                let position = AudioSample(at.0 + offset);
                reference.push(json!({
                    "sample":position.0,
                    "reference":ratio(actual.reference_at(position).expect("small reference query")),
                }));
            }
            let owners = actual
                .owners(at..AudioSample(at.0 + 1), Default::default())
                .expect("one-sample retained owner observation");
            let observed_owners = owners
                .spans()
                .iter()
                .map(|span| json!({
                    "samples":range(span.samples()),
                    "owners":span.owners().iter().map(|owner| owner_fields(owner, at)).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>();
            json!({
                "tag":"Retained",
                "checked":actual.belongs_to(plan),
                "same_arc_as_incoming":expected_retained.is_some_and(|expected| actual.ptr_eq(expected)),
                "reference_samples_per_output_sample":ratio(actual.reference_samples_per_output_sample()),
                "reference_map":reference,
                "one_sample_owner_observations":observed_owners,
            })
        }
    };
    let sounds = match &description.context.sounds {
        Sounds::Root => json!({"tag":"Root"}),
        Sounds::None => json!({"tag":"None"}),
        Sounds::Owner(sound) => json!({
            "tag":"Owner",
            "checked":sound.belongs_to(plan),
            "owner":sound.owner(),
            "samples":range(sound.samples()),
            "processing_domain":domain_fields(sound.processing_domain()),
        }),
    };
    let boundary_owner = description.boundary.owner();
    json!({
        "selected_owner":owner_fields(boundary_owner, at),
        "boundary":{
            "before":description.boundary.before_stage(),
            "matches_selected_input_owner":description.boundary.matches(selected_input_owner),
            "matches_clock_at_selected_input_owner":description.boundary.matches_clock_at(selected_input_owner, at),
        },
        "stop":{
            "before":description.stop.before,
            "owner_key":description.stop.owner.snapshot_fields(),
        },
        "temporal":temporal(description.kind),
        "context":{"original":original,"sounds":sounds},
    })
}

fn controller_work(work: &ReadWork) -> Value {
    json!({
        "plan_work":work.plan_work,
        "cache_plan_work":work.cache_plan_work,
        "cache_key_scopes":work.cache_key_scopes,
        "prepared_stages":work.prepared_stages,
        "prepared_frames":work.prepared_frames,
        "source_checks":work.source_checks,
    })
}

#[test]
fn legacy_description_matches_frozen_public_observation() {
    let mut expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/caller-description-baseline-v1/snapshot.json"
    ))
    .expect("frozen caller description baseline parses");
    expected.as_object_mut().unwrap().remove("variant");
    expected
        .as_object_mut()
        .unwrap()
        .remove("runtime_source_manifest_sha256");
    assert_eq!(legacy_description_snapshot(), expected);
}

#[test]
fn actual_admitted_descriptions_match_all_five_frozen_public_observations() {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/caller-description-baseline-v1/snapshot.json"
    ))
    .expect("immutable caller description baseline parses");
    let actual = description_fixture_snapshot::<true>();
    let expected = frozen["cases"].as_array().expect("frozen cases");
    let observed = actual["cases"].as_array().expect("admitted cases");
    assert_eq!(observed.len(), 5);
    assert_eq!(observed.len(), expected.len());
    for (actual, old) in observed.iter().zip(expected) {
        assert_eq!(actual["case"], old["case"]);
        assert_eq!(
            actual["description"], old["description"],
            "actual metered Description must preserve every frozen field: {}",
            old["case"]
        );
    }
    let incoming = observed
        .iter()
        .find(|case| case["case"] == "incoming_retained")
        .unwrap();
    assert_eq!(
        incoming["description"]["context"]["original"]["same_arc_as_incoming"],
        json!(true),
        "the first matching retained owner keeps the exact incoming Arc"
    );
    let repeated = observed
        .iter()
        .find(|case| case["case"] == "normal_document_first_repeat_voice")
        .unwrap();
    assert!(
        !repeated["description"]["selected_owner"]["instance"]["repeats"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the actual persistent key and boundary have a nonempty RepeatInstance path"
    );
}

#[test]
#[ignore = "parent-owned compiler/runtime pins before freezing the legacy caller oracle"]
fn produce_legacy_description_oracle_once_for_external_freeze() {
    let variant = std::env::var("DEADPAN_CALLER_BASELINE_VARIANT")
        .expect("set DEADPAN_CALLER_BASELINE_VARIANT explicitly");
    assert!(!variant.trim().is_empty(), "variant must be nonempty");
    let source_digest = std::env::var("DEADPAN_CALLER_BASELINE_SOURCE_SHA256")
        .expect("explicit runtime source manifest SHA-256");
    assert!(source_digest.len() == 64 && source_digest.bytes().all(|b| b.is_ascii_hexdigit()));
    let output = PathBuf::from(
        std::env::var_os("DEADPAN_CALLER_BASELINE_OUTPUT")
            .expect("set DEADPAN_CALLER_BASELINE_OUTPUT explicitly"),
    );
    let output = checked_output(&output);

    let mut document = legacy_description_snapshot();
    document["variant"] = json!(variant);
    document["runtime_source_manifest_sha256"] = json!(source_digest);
    let bytes = serde_json::to_vec_pretty(&document).expect("oracle serializes");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("oracle output must be a new path; existing files are never overwritten");
    file.write_all(&bytes).expect("write oracle bytes");
    file.write_all(b"\n").expect("finish oracle line");
}
