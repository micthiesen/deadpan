//! Actual old numerical helper work receipts, independently frozen before query admission.
use super::*;
use serde_json::{Value, json};

fn plan_error(value: &PlanError) -> Value {
    match value {
        PlanError::InvalidPlan(message) => json!({"variant":"InvalidPlan","message":message}),
        PlanError::AudioQueryLimit(budget) => json!({"variant":"AudioQueryLimit","budget":budget}),
        PlanError::InvalidAudioLimits => json!({"variant":"InvalidAudioLimits"}),
        PlanError::AudioRangeOutOfRange => json!({"variant":"AudioRangeOutOfRange"}),
        PlanError::Document(value) => {
            json!({"variant":"Document","code":value.code,"message":value.message})
        }
        _ => panic!("add the explicit legacy receipt Plan error serializer: {value}"),
    }
}
fn stage_error(value: &StageAudioError) -> Value {
    match value {
        StageAudioError::Plan(value) => json!({"variant":"Plan","error":plan_error(value)}),
        StageAudioError::Limit(budget) => json!({"variant":"Limit","budget":budget}),
        StageAudioError::Timeout => json!({"variant":"Timeout"}),
        _ => panic!("add the explicit legacy receipt Stage error serializer: {value}"),
    }
}
fn receipt<'plan>(
    plan: &'plan RenderPlan,
    incoming: &Context<'plan>,
    key: &OwnerKey,
    at: AudioSample,
    headroom: usize,
) -> Value {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let initial = MAX_PLAN_WORK_PER_READ - headroom;
    work.borrow_mut().plan_work = initial;
    let result = original_context(
        plan,
        incoming,
        key,
        at..AudioSample(at.0 + 1),
        at,
        fixture::control(&cancelled, &work),
    );
    let outcome = match result {
        Ok(Some(Context::Domain(_))) => json!({"result":"Some","kind":"Domain"}),
        Ok(Some(Context::Retained(_))) => json!({"result":"Some","kind":"Retained"}),
        Ok(Some(Context::Root)) => panic!("helper never returns Root"),
        Ok(None) => json!({"result":"None"}),
        Err(error) => json!({"result":"Error","error":stage_error(&error)}),
    };
    let ledger = work.borrow();
    json!({"headroom":headroom,"initial_plan_work":initial,"final_plan_work":ledger.plan_work,
      "new_billed_work":ledger.plan_work-initial,"outcome":outcome,
      "prepared_frames":ledger.prepared_frames,"metadata_frames":ledger.sound_context_frames,
      "source_checks":ledger.source_checks})
}
fn one<'plan>(
    plan: &'plan RenderPlan,
    incoming: &Context<'plan>,
    owner: &AudioOwnerClock<'plan>,
    name: &str,
) -> Value {
    let at = AudioSample(128);
    let key = OwnerKey::new(owner);
    let query = match incoming {
        Context::Root => plan
            .audio_gain_owners(at..AudioSample(129), Default::default())
            .unwrap(),
        Context::Domain(domain) => domain
            .gain_owners(at..AudioSample(129), Default::default())
            .unwrap(),
        Context::Retained(_) => panic!("Root/Domain baseline only"),
    };
    let member = query.spans()[0]
        .owners()
        .iter()
        .position(|v| OwnerKey::new(v) == key)
        .unwrap();
    let expected_input = match incoming {
        Context::Root => plan
            .audio_owner_input(&query.spans()[0].owners()[member], at, Default::default())
            .unwrap(),
        Context::Domain(domain) => domain
            .owner_input(&query.spans()[0].owners()[member], at, Default::default())
            .unwrap(),
        Context::Retained(_) => unreachable!(),
    };
    let q = query.work();
    let c = expected_input.work() - q;
    let probe = expected_input
        .retained_probe_at(at, Default::default())
        .unwrap();
    let retained_owner_work = probe.input().map_or(0, |v| {
        v.owners(at..AudioSample(129), Default::default())
            .unwrap()
            .work()
    });
    let total = q + c + probe.work() + retained_owner_work;
    let ample = receipt(plan, incoming, &key, at, MAX_PLAN_WORK_PER_READ);
    assert_eq!(ample["new_billed_work"], json!(total));
    json!({"case":name,"at":128,"selected_member_index":member,"legacy_q":q,"legacy_c":c,
      "retained_probe_work":probe.work(),"retained_result":if probe.input().is_some(){"Some"}else{"None"},
      "retained_owner_query_work":retained_owner_work,"expected_success_receipt_total":total,
      "ample":ample,"exact_total_headroom":receipt(plan,incoming,&key,at,total),
      "one_short_total_headroom":receipt(plan,incoming,&key,at,total-1),
      "one_short_query_headroom":receipt(plan,incoming,&key,at,q-1),
      "one_short_capture_headroom":receipt(plan,incoming,&key,at,q+c-1)})
}
#[test]
#[ignore = "explicit frozen old WorkControl receipt capture; parent owns build/runtime pins"]
fn capture_legacy_query_workcontrol_receipts_v1() {
    let output = std::path::PathBuf::from(
        std::env::var_os("DEADPAN_OWNER_WORK_OUTPUT").expect("explicit /tmp output path required"),
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
        std::env::var("DEADPAN_OWNER_SNAPSHOT_SOURCE_SHA256").expect("source manifest required");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    let domain = Context::Domain(Arc::new(input.context().clone()));
    let retained = retained_plan();
    let query = retained
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let tone = query.spans()[0]
        .owners()
        .iter()
        .find(|v| v.instance().node.as_str() == "tone")
        .unwrap();
    let result = json!({"schema":"OwnerQueryWorkControlReceiptsV1","producer":"legacy_pre_query_admission",
       "variant":variant,"runtime_source_manifest_sha256":source,"numeric_helper":"original_context (actual legacy Q/C/probe/body)",
       "cases":[one(&plan,&Context::Root,&owner,"unbound_eq_root"),
          one(&plan,&domain,&owner,"unbound_eq_actual_domain"),
          one(&retained,&Context::Root,tone,"accepted_retained_tone")]});
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
