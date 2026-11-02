//! Optional complete residency observations written before the existing assertions.
use serde::Serialize;
use serde_json::Value;
use std::io::{BufWriter, Write};

#[derive(Serialize)]
pub(super) struct Snapshot<'value> {
    pub(super) predeclared_legal_cap_domain: [u32; 2],
    pub(super) maximum_binary_trials: u32,
    pub(super) dropped_frontier: u32,
    pub(super) dropped_trials: &'value [Value],
    pub(super) held_trial: &'value Value,
    pub(super) matched_drop_control: &'value Value,
    pub(super) actual_reserve_threshold_trials: &'value [Value],
    pub(super) reserve_frontier_batch_manifest: &'value Value,
    #[serde(rename = "selected_O_natural_resident_refusals")]
    pub(super) selected_o_natural_resident_refusals: &'value [Value],
    #[serde(rename = "separating_held_N_drop_retry")]
    pub(super) separating_held_n_drop_retry: bool,
}
#[derive(Serialize)]
struct Envelope<'value> {
    schema: &'static str,
    qualification_status: &'static str,
    scope: &'static str,
    assertion_order: [&'static str; 2],
    snapshot: Snapshot<'value>,
}
pub(super) fn write(snapshot: Snapshot<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let Some(output) = std::env::var_os("DEADPAN_STAGE_BLOCK_POLICY_RESIDENT_DIAGNOSTIC_OUTPUT")
    else {
        return Ok(());
    };
    let output = std::path::Path::new(&output);
    if !output.is_absolute() || !output.starts_with("/tmp") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "resident diagnostic output must be an absolute /tmp path",
        )
        .into());
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(
        &mut writer,
        &Envelope {
            schema: "stage-block-policy-resident-preassert-diagnostic-v1",
            qualification_status: "unadjudicated diagnostic; original assertions still execute",
            scope: "complete current trials, including errors/retry/drop; source/binary authority is the guarded command receipt; no inferred witness",
            assertion_order: [
                "actual held N output must separate a cold O target and recover after its drop",
                "actual legal cap must reach selected O resident refusal",
            ],
            snapshot,
        },
    )?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}
