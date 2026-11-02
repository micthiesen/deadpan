//! The checked private projection reaches O after its smaller preceding native floor.
use super::*;

fn trial(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    limits: StageLimits,
    repeat: bool,
    name: &str,
) -> Value {
    COLLECTION.with(|slot| {
        assert!(slot.borrow().as_ref().unwrap().rows.is_empty());
    });
    let action = super::private_action(
        plan,
        PrivateTarget::Projected(projection),
        inner,
        name,
        limits,
        PrivateOptions {
            repeat,
            ..Default::default()
        },
    );
    object([
        ("action", action),
        ("receipts", Value::Array(super::receipts())),
    ])
}

pub(super) fn run(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    // Fixed by retained diagnostic1, not selected from this control or a new search.
    let cap = 2347;
    assert!((1..=StageLimits::default().maximum_resident_frames).contains(&cap));
    let default = trial(
        plan,
        projection,
        reopen()?,
        StageLimits::default(),
        true,
        "O_private_projected_resident_default",
    );
    let lower = trial(
        plan,
        projection,
        reopen()?,
        StageLimits {
            maximum_resident_frames: cap,
            ..Default::default()
        },
        false,
        "O_private_projected_resident_2347",
    );
    let baseline = receipt(&default["action"]["first"], array(&default, "receipts"));
    let failed = receipt(&lower["action"]["first"], array(&lower, "receipts"));
    let before = &baseline["native"];
    let now = &failed["native"];
    assert_eq!(array(&default, "receipts").len(), 2);
    assert_eq!(array(&lower, "receipts").len(), 1);
    assert_eq!(default["action"]["first"]["result"]["status"], "Ok");
    assert_eq!(default["action"]["same_actual_prepared_arc"], true);
    assert_eq!(
        default["action"]["first"]["result"]["prepared"],
        default["action"]["prepared_after_ReadWork_drop"]
    );
    assert_eq!(
        default["action"]["prepared_after_ReadWork_drop"],
        default["action"]["prepared_after_controller_drop"]
    );
    let memo = &array(&default, "receipts")[1];
    assert!(array(&memo["native"], "calls").is_empty());
    assert!(array(&memo["ambient_N"], "calls").is_empty());
    for trace in [before, now] {
        assert_eq!(trace["fault"], "None");
        let calls = array(trace, "calls");
        assert_eq!(calls.len(), 3);
        for (index, call) in calls.iter().enumerate() {
            assert_eq!(call["input"], "PreserveTape");
            assert_eq!(call["start"], index * 256);
            assert_eq!(call["sample_len"], 256);
        }
    }
    assert_eq!(lower["action"]["first"]["result"]["status"], "Error");
    let outer = &lower["action"]["first"]["result"]["error"];
    assert_eq!(outer["kind"], "Limit");
    assert_eq!(outer["cancelled"], false);
    assert_eq!(
        outer["typed_payload"],
        json!({"reason":"resident PCM or stage cache"})
    );
    let failures = array(now, "rows")
        .iter()
        .filter(|row| row["phase"] == "make_room_after" && !row["value"]["error"].is_null())
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    let failure = failures[0];
    assert_eq!(failure["ticket"], 3);
    let value = &failure["value"];
    assert_eq!(value["eligible"], true);
    assert_eq!(value["attempt"], 19);
    assert_eq!(value["layout"], json!([1536, 16]));
    assert_eq!(value["frames"], 192);
    assert_eq!(value["host_work_delta"], json!([1, 0]));
    assert_eq!(
        value["error"],
        json!({"kind":"Limit","reason":"resident PCM or stage cache","cancelled":false})
    );
    let resident = &value["resident"];
    assert_eq!(resident["limit"], cap);
    assert_eq!(n(&resident["sum_before_pending"]), 2156);
    assert_eq!(n(&resident["pending_additional"]), 192);
    assert_eq!(
        n(&resident["sum_before_pending"]) + n(&resident["pending_additional"]),
        2348
    );
    let matching = array(before, "rows")
        .iter()
        .filter(|row| {
            row["phase"] == "make_room_after"
                && row["ticket"] == failure["ticket"]
                && row["value"]["attempt"] == value["attempt"]
        })
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    let control = &matching[0]["value"];
    assert!(control["error"].is_null());
    for key in ["layout", "frames", "host_work_delta"] {
        assert_eq!(control[key], value[key]);
    }
    for key in ["work", "ledger", "lane"] {
        assert_eq!(control["before"][key], value["before"][key]);
    }
    for key in [
        "active",
        "cached",
        "projected",
        "sound_context",
        "owned_suppression",
        "pending_additional",
        "sum_before_pending",
    ] {
        assert_eq!(control["resident"][key], resident[key]);
    }
    let predicates = array(&failed["ambient_N"], "resident_rows");
    let last = predicates.last().unwrap();
    assert_eq!(last["sum_with_pending"], 2348);
    assert_eq!(last["maximum_resident_frames"], cap);
    assert!(
        predicates[..predicates.len() - 1]
            .iter()
            .all(|row| n(&row["sum_with_pending"]) <= u64::from(cap))
    );
    let native_errors = array(&failed["ambient_N"], "native_rows")
        .iter()
        .filter(|row| !row["error"].is_null())
        .collect::<Vec<_>>();
    assert_eq!(native_errors.len(), 2);
    assert_eq!(native_errors[0]["kind"], "MakeRoomAfter");
    assert_eq!(native_errors[1]["kind"], "ReserveResult");
    assert_eq!(native_errors[0]["before"], value["before"]);
    assert_eq!(native_errors[0]["after"], value["after"]);
    let refused = array(now, "rows")
        .iter()
        .find(|row| row["phase"] == "reserve_result" && n(&row["ordinal"]) > n(&failure["ordinal"]))
        .unwrap();
    assert_eq!(refused["ticket"], failure["ticket"]);
    assert_eq!(refused["value"]["attempt"], value["attempt"]);
    assert_eq!(refused["value"]["action"], "None");
    assert_eq!(refused["value"]["actual_injected_refusal"], false);
    assert_eq!(refused["value"]["before"]["ledger"], 105);
    assert_eq!(refused["value"]["after"]["ledger"], 105);
    assert_eq!(
        accepted(before, Some(n(&refused["ordinal"]))),
        accepted(now, None)
    );
    let tail = array(now, "rows")
        .iter()
        .filter(|row| n(&row["ordinal"]) > n(&refused["ordinal"]))
        .collect::<Vec<_>>();
    assert!(tail.iter().all(|row| !matches!(
        row["phase"].as_str(),
        Some(
            "spend_attempt"
                | "reserve_attempt"
                | "spend_result"
                | "reserve_result"
                | "owner_borrow_begin"
                | "writer_push_before"
        )
    )));
    assert!(call_rows(now, &failure["ticket"], "owner_borrow_begin").is_empty());
    let releases = tail
        .iter()
        .filter(|row| row["phase"] == "release_after")
        .collect::<Vec<_>>();
    assert_eq!(releases.len(), 3);
    for (row, (frames, ledger)) in releases.iter().zip([(3, 102), (96, 6), (6, 0)]) {
        assert_eq!(row["value"]["frames_before"], json!([frames, 0]));
        assert_eq!(row["value"]["frames_after"], json!([0, 0]));
        assert_eq!(row["value"]["after"]["ledger"], ledger);
    }
    for action in [&default["action"], &lower["action"]] {
        assert_eq!(
            action["first"]["input_before"],
            action["first"]["input_after"]
        );
        assert_eq!(action["cancelled_at_end"], false);
        assert_eq!(action["first"]["work_after"]["sound_context_frames"], 0);
        assert_eq!(action["controller_before_drop"]["active_frames"], 0);
    }
    let work = &lower["action"]["final_work"];
    assert_eq!(work["projected_active_len"], 0);
    assert_eq!(work["projected_resident_frames"], 0);
    assert!(array(work, "projected_blocks").is_empty());
    assert!(lower["action"]["prepared_after_ReadWork_drop"].is_null());
    assert!(lower["action"]["prepared_after_controller_drop"].is_null());
    assert_eq!(
        lower["action"]["controller_before_drop"]["owned_account"],
        3
    );
    assert_eq!(baseline["provider_entries"], failed["provider_entries"]);
    Ok(object([
        (
            "schema",
            json!("stage-block-policy-private-projected-residency-v1"),
        ),
        (
            "scope",
            json!(
                "real checked private prepare_projected(base); natural selected O refusal and cleanup; no public equivalence or same-cap O recovery"
            ),
        ),
        (
            "cap_authority",
            json!({"raw_sha256":"943dee5febe69d35baee546d10017ce0c43f8964eadef074d1f84851ee9f4160","raw_bytes":2805083,"preceding_peak":2345,"selected_O_demand":2348,"chosen_cap":cap,"rule":"fixed2348-minus-one; no search or outcome-derived cap"}),
        ),
        ("default_control_and_memo", default),
        ("cold_lowered_trial", lower),
    ]))
}
