//! Run real automatic admission and retain its report plus exact probe movie.
//! Usage: qualify_encoder_admission WORKER WIDTH HEIGHT FPS_NUM FPS_DEN REPORT [sdr|pq|hlg]

use deadpan_cli::{
    encoded_render::admission::{AdmissionLimits, AdmissionRequest, qualify},
    render_worker::{RenderWorkerRuntime, protocol::RenderIdentity},
};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (args, color) = match args.as_slice() {
        [rest @ .., color] if rest.len() == 6 => (rest, color.as_str()),
        rest => (rest, "sdr"),
    };
    let color_policy = match color {
        "sdr" => deadpan_core::ColorPolicy::SdrRec709,
        "pq" => deadpan_core::ColorPolicy::HdrRec2020Pq,
        "hlg" => deadpan_core::ColorPolicy::HdrRec2020Hlg,
        _ => return Err("color must be sdr, pq or hlg".into()),
    };
    let [worker, width, height, num, den, report] = args else {
        return Err("expected WORKER WIDTH HEIGHT FPS_NUM FPS_DEN REPORT [sdr|pq|hlg]".into());
    };
    let worker = fs::canonicalize(worker)?;
    let report = std::path::absolute(report)?;
    let parent = report.parent().ok_or("report parent")?.canonicalize()?;
    if !parent.starts_with(fs::canonicalize("/tmp")?) {
        return Err("qualification output must be in /tmp".into());
    }
    let report = parent.join(report.file_name().ok_or("report filename")?);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&report)?;
    let movie = report.with_extension("probe.mp4");
    let runtime = RenderWorkerRuntime {
        executable: worker,
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    };
    let request = AdmissionRequest {
        identity: RenderIdentity {
            request_id: RequestId::new("native-admission-qualification")?,
            attempt_id: AttemptId::new("fresh-runtime")?,
        },
        cancellation_token: CancellationToken::new("native-admission-cancel")?,
        raster: [width.parse()?, height.parse()?],
        frame_rate: [num.parse()?, den.parse()?],
        color_policy,
    };
    let started = Instant::now();
    let deadline = started + Duration::from_secs(120);
    let cancelled = AtomicBool::new(false);
    let result = qualify(
        &runtime,
        request,
        AdmissionLimits::default(),
        &cancelled,
        deadline,
        |_, _, _| {},
    );
    match result {
        Ok(mut qualified) => {
            let mut retained = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&movie)?;
            let count = qualified.copy_probe_to(&mut retained, &cancelled, deadline)?;
            retained.sync_all()?;
            drop(retained);
            let mut reader = fs::File::open(&movie)?;
            let mut digest = Sha256::new();
            let mut bytes = [0_u8; 64 * 1024];
            loop {
                if Instant::now() >= deadline {
                    return Err("retained movie hash deadline".into());
                }
                let read = reader.read(&mut bytes)?;
                if read == 0 {
                    break;
                }
                digest.update(&bytes[..read]);
            }
            let sha256: String = digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            if sha256
                != qualified
                    .decision()
                    .selected
                    .manifest
                    .movie
                    .sha256()
                    .as_str()
            {
                return Err("retained probe hash differs".into());
            }
            serde_json::to_writer_pretty(
                &mut output,
                &json!({"schema_version":1,"status":"passed","seconds":started.elapsed().as_secs_f64(),"decision":qualified.decision(),"retained_probe":{"path":movie,"bytes":count,"sha256":sha256},"scope":"fresh automatic encoder admission only; no durable job policy, project export or public UI"}),
            )?;
            writeln!(output)?;
            output.sync_all()?;
            Ok(())
        }
        Err(error) => {
            serde_json::to_writer_pretty(
                &mut output,
                &json!({"schema_version":1,"status":"failed","seconds":started.elapsed().as_secs_f64(),"cleanup_confirmed":error.error.cleanup_confirmed(),"error":error.to_string(),"rejected":error.rejected}),
            )?;
            writeln!(output)?;
            output.sync_all()?;
            Err(error.into())
        }
    }
}
