//! Two fresh projected trials: default control and one previously measured lower cap.
use super::*;
use std::io::{BufWriter, Write};

#[derive(serde::Serialize)]
struct BoundarySource {
    schema: &'static str,
    raw_sha256: &'static str,
    raw_bytes: u64,
    preceding_non_o_peak: u32,
    earlier_o_peak: u32,
    selected_o_demand: u32,
    chosen_legal_cap: u32,
    rule: &'static str,
}
#[derive(serde::Serialize)]
struct Trial<'value> {
    requested_resident_cap: u32,
    repeat_if_first_succeeds: bool,
    action: &'value Value,
    receipts: &'value [Value],
}
#[derive(serde::Serialize)]
struct Envelope<'value> {
    schema: &'static str,
    qualification_status: &'static str,
    operation_scope: &'static str,
    configuration: &'static str,
    provenance_scope: &'static str,
    boundary_source: BoundarySource,
    default_trial: Trial<'value>,
    lowered_trial: Trial<'value>,
    original_assertions: &'static str,
}
pub(super) fn run_if_requested(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<()> {
    let Some(output) =
        std::env::var_os("DEADPAN_STAGE_BLOCK_POLICY_PROJECTED_BOUNDARY_DIAGNOSTIC_OUTPUT")
    else {
        return Ok(());
    };
    let output = Path::new(&output);
    if !output.is_absolute() || !output.starts_with("/tmp") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "projected boundary diagnostic output must be an absolute /tmp path",
        )
        .into());
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut trial =
        |limits: StageLimits, repeat: bool, name: &str| -> SetupResult<(Value, Vec<Value>)> {
            COLLECTION.with(|slot| {
                assert!(
                    slot.borrow().as_ref().unwrap().rows.is_empty(),
                    "projected diagnostic cannot consume prior action receipts"
                );
            });
            let action = super::private_action(
                plan,
                PrivateTarget::Projected(projection),
                reopen()?,
                name,
                limits,
                PrivateOptions {
                    repeat,
                    ..Default::default()
                },
            );
            let rows = super::receipts();
            Ok((action, rows))
        };
    let defaults = StageLimits::default();
    let default_cap = defaults.maximum_resident_frames;
    let (default_action, default_rows) = trial(defaults, true, "O_projected_boundary_default")?;
    let lower_cap = 2347;
    assert!((1..=default_cap).contains(&lower_cap));
    let (lowered_action, lowered_rows) = trial(
        StageLimits {
            maximum_resident_frames: lower_cap,
            ..Default::default()
        },
        false,
        "O_projected_boundary_2347",
    )?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(
        &mut writer,
        &Envelope {
            schema: "stage-block-policy-projected-boundary-diagnostic-v2",
            qualification_status: "unadjudicated private boundary experiment; no outcome or public equivalence assumed",
            operation_scope: "same checked prepare_projected(base), two independent fresh renderer/default Work trials; real verified provider reopen per trial",
            configuration: "default control with conditional same-Work memo; one cold lower-cap trial at fixed measured2347; no search/holder/fault/source-mode/work-price changes",
            provenance_scope: "complete real action/result/cleanup/receipt values; source/binary authority is guarded Root command receipt",
            boundary_source: BoundarySource {
                schema: "stage-block-policy-projected-default-diagnostic-v1",
                raw_sha256: "943dee5febe69d35baee546d10017ce0c43f8964eadef074d1f84851ee9f4160",
                raw_bytes: 2805083,
                preceding_non_o_peak: 2345,
                earlier_o_peak: 2188,
                selected_o_demand: 2348,
                chosen_legal_cap: lower_cap,
                rule: "fixed2348-minus-one from retained first diagnostic; not recomputed from these trial outcomes",
            },
            default_trial: Trial {
                requested_resident_cap: default_cap,
                repeat_if_first_succeeds: true,
                action: &default_action,
                receipts: &default_rows,
            },
            lowered_trial: Trial {
                requested_resident_cap: lower_cap,
                repeat_if_first_succeeds: false,
                action: &lowered_action,
                receipts: &lowered_rows,
            },
            original_assertions: "the unchanged following public resident trials/assertions still execute; any failed test remains failed",
        },
    )?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}
