//! Explicit developer hardware probe. Ordinary tests never open a device.
#[cfg(target_os = "macos")]
mod supported {
    use std::time::{Duration, Instant};

    use deadpan_output::{
        DeviceOutput, DeviceReport, FeedError, Generation, RenderStatus, SAMPLE_RATE,
    };
    use serde_json::{Value, json};

    fn fill(
        output: &mut DeviceOutput,
        generation: Generation,
        end: i64,
        tone: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        while output.feed().next_sample() < end {
            let start = output.feed().next_sample();
            let count = usize::try_from((end - start).min(256))?;
            let mut pcm = [[0.0; 2]; 256];
            for (offset, frame) in pcm[..count].iter_mut().enumerate() {
                // Short -42 dBFS tone with 10 ms ramps, constructed off callback.
                let n = start + i64::try_from(offset)?;
                if tone && (0..9_600).contains(&n) {
                    let ramp = (n as f32 / 480.0)
                        .min((9_599 - n) as f32 / 480.0)
                        .clamp(0.0, 1.0);
                    let sample = (n as f32 * std::f32::consts::TAU * 440.0 / SAMPLE_RATE as f32)
                        .sin()
                        * 0.007_943_282
                        * ramp;
                    *frame = [sample, sample];
                }
            }
            match output.feed().submit(generation, &pcm[..count]) {
                Ok(()) => {}
                Err(FeedError::Full) => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        match output.feed().finish(generation) {
            Ok(()) => Ok(true),
            Err(FeedError::Full) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn drain(
        output: &mut DeviceOutput,
        phase: &str,
        records: &mut Vec<Value>,
        observed: &mut Vec<DeviceReport>,
    ) {
        while let Some(report) = output.pop_report() {
            records.push(json!({
                "phase": phase, "generation": report.render.generation.get(),
                "channel_id": report.render.generation.channel_id(),
                "status": format!("{:?}", report.render.status),
                "first_sample": report.render.first_sample,
                "rendered_frames": report.render.rendered_frames,
                "silent_frames": report.render.silent_frames,
                "discarded_packets": report.render.discarded_packets,
                "callback_ns": report.callback_ns, "playback_ns": report.playback_ns,
                "render_cost_ns": report.render_cost_ns,
            }));
            observed.push(report);
        }
    }

    fn contiguous(reports: &[&DeviceReport], start: i64, end: i64) -> bool {
        let mut next = start;
        for report in reports.iter().filter(|r| r.render.rendered_frames > 0) {
            if report.render.first_sample != Some(next) {
                return false;
            }
            let Ok(count) = i64::try_from(report.render.rendered_frames) else {
                return false;
            };
            let Some(after) = next.checked_add(count) else {
                return false;
            };
            next = after;
        }
        next == end
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if args.iter().any(|arg| arg != "--tone") {
            return Err("usage: qualify_output [--tone]".into());
        }
        let tone = args.iter().any(|arg| arg == "--tone");
        let mut output = DeviceOutput::open_default()?;
        let info = output.info().clone();
        let buffer_frames = output.buffer_frames()?;
        let mut records = Vec::new();
        let mut observed = Vec::new();
        let mut checks = Vec::new();
        let generation = output.feed().restart(0)?;
        let end = i64::from(SAMPLE_RATE) * 2;
        let mut finished = fill(&mut output, generation, end, tone)?;
        output.feed().activate(generation)?;
        output.start_device()?;
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(2_250) {
            if !finished {
                finished = fill(&mut output, generation, end, tone)?;
            }
            drain(&mut output, "steady", &mut records, &mut observed);
            std::thread::sleep(Duration::from_millis(2));
        }
        drain(&mut output, "steady", &mut records, &mut observed);
        let steady: Vec<_> = observed
            .iter()
            .filter(|r| r.render.generation == generation)
            .collect();
        let steady_frames: usize = steady.iter().map(|r| r.render.rendered_frames).sum();
        checks.push(json!({"name":"steady_complete_pcm", "passed":contiguous(&steady, 0, end) && finished, "frames":steady_frames}));
        checks.push(json!({"name":"steady_no_starvation_or_fault", "passed":steady.iter().all(|r| !matches!(r.render.status, RenderStatus::Starved | RenderStatus::Fault))}));
        checks.push(json!({"name":"explicit_end", "passed":steady.iter().any(|r| r.render.status == RenderStatus::Ended)}));

        // Queue a long region, let it partially play, then invalidate queued PCM.
        let old = output.feed().restart(100_000)?;
        fill(&mut output, old, 120_000, false)?;
        output.feed().activate(old)?;
        std::thread::sleep(Duration::from_millis(25));
        drain(&mut output, "before_seek", &mut records, &mut observed);
        let sought = output.feed().restart(900_000)?;
        let mut finished = false;
        let mut activated = false;
        let began = Instant::now();
        while began.elapsed() < Duration::from_millis(150) {
            if !finished {
                finished = fill(&mut output, sought, 904_800, false)?;
            }
            if !activated && output.feed().next_sample() >= 904_096 {
                output.feed().activate(sought)?;
                activated = true;
            }
            drain(&mut output, "seek", &mut records, &mut observed);
            std::thread::sleep(Duration::from_millis(2));
        }
        drain(&mut output, "seek", &mut records, &mut observed);
        let seek: Vec<_> = observed
            .iter()
            .filter(|r| r.render.generation == sought && r.render.rendered_frames > 0)
            .collect();
        checks.push(
            json!({"name":"seek_new_coordinate", "passed":contiguous(&seek, 900_000, 904_800)}),
        );

        let starved = output.feed().restart(1_000_000)?;
        output.feed().submit(starved, &[[0.0; 2]; 256])?;
        output.feed().activate(starved)?;
        std::thread::sleep(Duration::from_millis(40));
        drain(&mut output, "starve", &mut records, &mut observed);
        let late_begin = observed.len();
        output.feed().submit(starved, &[[0.0; 2]; 256])?;
        std::thread::sleep(Duration::from_millis(40));
        drain(&mut output, "late_refill", &mut records, &mut observed);
        let late: Vec<_> = observed[late_begin..]
            .iter()
            .filter(|r| r.render.generation == starved)
            .collect();
        checks.push(json!({"name":"starvation_latches_silence", "passed":!late.is_empty() && late.iter().all(|r| r.render.rendered_frames == 0 && r.render.status == RenderStatus::Starved)}));

        // Fill the starved queue completely before native pause. A paused
        // callback cannot release these slots for the replacement generation.
        let mut full = false;
        for _ in 0..=deadpan_output::QUEUE_PACKETS {
            match output.feed().submit(starved, &[[0.0; 2]; 256]) {
                Ok(()) => {}
                Err(FeedError::Full) => {
                    full = true;
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        if !full {
            return Err("expected a completely full starved queue".into());
        }

        // Run muted callbacks first so they can drain the full stale queue.
        // New content starts only after prefill and explicit activation.
        output.pause_device()?;
        drain(&mut output, "pause", &mut records, &mut observed);
        let resumed = output.feed().restart(2_000_000)?;
        output.start_device()?;
        let began = Instant::now();
        while !fill(&mut output, resumed, 2_002_400, false)? {
            if began.elapsed() >= Duration::from_secs(1) {
                return Err("stale queue did not drain after native resume".into());
            }
            drain(&mut output, "resume_prefill", &mut records, &mut observed);
            std::thread::sleep(Duration::from_millis(2));
        }
        output.feed().activate(resumed)?;
        std::thread::sleep(Duration::from_millis(100));
        drain(&mut output, "resume", &mut records, &mut observed);
        let resume: Vec<_> = observed
            .iter()
            .filter(|r| r.render.generation == resumed)
            .collect();
        checks.push(json!({"name":"pause_resume_full_stale_queue", "passed":contiguous(&resume, 2_000_000, 2_002_400)}));
        output.check_route()?;
        // Inject our permanent fault while the actual stream is still running.
        // This is controller fault-path coverage, not a physical disconnection.
        output.feed().fault_signal().raise();
        checks.push(json!({"name":"injected_fault_visibility", "passed":matches!(output.check_route(), Err(deadpan_output::DeviceError::Faulted))}));
        std::thread::sleep(Duration::from_millis(40));
        let fault_begin = observed.len();
        drain(&mut output, "injected_fault", &mut records, &mut observed);
        checks.push(json!({"name":"fault_silences_callbacks", "passed":observed[fault_begin..].iter().any(|r| r.render.status == RenderStatus::Fault) && observed[fault_begin..].iter().filter(|r| r.render.status == RenderStatus::Fault).all(|r| r.render.rendered_frames == 0)}));
        let paused_fault = matches!(
            output.pause_device(),
            Err(deadpan_output::DeviceError::Feed(FeedError::Fault))
        );
        drain(&mut output, "stopped", &mut records, &mut observed);
        let stopped_count = observed.len();
        std::thread::sleep(Duration::from_millis(40));
        drain(&mut output, "after_stop", &mut records, &mut observed);
        checks.push(json!({"name":"pause_fault_stops_callbacks", "passed":paused_fault && observed.len() == stopped_count}));
        checks.push(json!({"name":"no_reported_backend_errors", "passed":output.error_flags() == 0,"flags":output.error_flags()}));
        checks.push(json!({"name":"telemetry_complete", "passed":output.dropped_reports() == 0,"dropped":output.dropped_reports()}));
        checks.push(json!({"name":"timestamps_monotonic", "passed":observed.windows(2).all(|w| w[1].callback_ns >= w[0].callback_ns && w[1].playback_ns >= w[0].playback_ns)}));
        checks.push(json!({"name":"measured_render_body_within_buffer_duration", "passed":observed.iter().all(|r| u128::from(r.render_cost_ns)*u128::from(SAMPLE_RATE) < (r.render.rendered_frames+r.render.silent_frames) as u128*1_000_000_000)}));
        let passed = checks.iter().all(|c| c["passed"] == true);
        let peak = observed.iter().map(|r| r.render_cost_ns).max();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version":1,"engine":deadpan_output::ENGINE_ID,"cpal":"0.18.2","rtrb":"0.4.0",
                "device":{"id":info.id,"name":info.name,"sample_rate":info.sample_rate,"channels":info.channels,"sample_format":info.sample_format,"buffer_range":info.buffer_range,"buffer_frames":buffer_frames},
                "tone":tone,"status":if passed {"passed"} else {"failed"},
                "checks":checks,"callback_count":observed.len(),"max_render_cost_ns":peak,"records":records,
                "limits":["No acoustic or loopback measurement","No physical disconnect/rate switch/suspend or inference stress","Render timing excludes telemetry publication and CPAL/driver work outside our closure","CPAL exceptional error path may allocate; release realtime contract remains unqualified"]
            }))?
        );
        if !passed {
            return Err("output qualification assertions failed".into());
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        supported::run()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("output qualification requires macOS".into())
    }
}
