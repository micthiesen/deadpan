use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use deadpan_core::ProjectFrame;
use deadpan_plan::RenderPlan;
use deadpan_source::{DecodeLimits, Mp4TrackKind, SourceDecoder, inspect_mp4};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use super::{
    CUT_FRAME, EDITS, FINAL_FRAMES, FINAL_SAMPLES,
    edits::{Edited, check_picture},
    fixture::Fixture,
    support::{Result, Run, json_file, sha256, text},
};

pub fn check(run: &Run, source: &Fixture, edited: &Edited) -> Result<Value> {
    let store = ProjectStore::open(&source.package, AccessMode::ReadOnly)?;
    let plan = RenderPlan::compile(&store.snapshot_at(&edited.revision)?)?;
    for frame in 0..FINAL_FRAMES {
        check_picture(&plan, source, EDITS, frame)?;
    }
    assert!(plan.picture(ProjectFrame(FINAL_FRAMES)).is_err());
    drop(store);
    let output = run.root.join("render");
    fs::create_dir(&output)?;
    let render_log = run.cli(
        "render",
        &[
            "render",
            text(&source.package)?,
            "--output",
            text(&output)?,
            "--name",
            "fractional.mp4",
            "--expected",
            edited.revision.as_str(),
        ],
    )?;
    let events = fs::read_to_string(&render_log)?;
    let finished: Value = serde_json::from_str(
        events
            .lines()
            .last()
            .ok_or("Render emitted no final event")?,
    )?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published");
    assert_eq!(
        finished["status"]["captured_revision"],
        edited.revision.as_str()
    );
    let movie = PathBuf::from(
        finished["status"]["receipt"]["movie"]
            .as_str()
            .ok_or("Render published movie path")?,
    );
    let timing = check_emitted_timing(run, &movie)?;
    let windows: Vec<(i64, i64)> = (0..FINAL_SAMPLES)
        .step_by(480_000)
        .map(|start| (start, (start + 480_000).min(FINAL_SAMPLES)))
        .collect();
    assert_eq!(windows.len(), 34);
    let selection = windows
        .iter()
        .map(|(a, b)| format!("{a}:{b}"))
        .collect::<Vec<_>>()
        .join(",");
    let report_path = run.cli(
        "verify-export",
        &[
            "verify-export",
            text(&source.package)?,
            "--movie",
            text(&movie)?,
            "--revision",
            edited.revision.as_str(),
            "--every",
            "1",
            "--samples",
            &selection,
        ],
    )?;
    let report = json_file(&report_path)?;
    assert_eq!(report["passed"], true);
    assert_eq!(report["revision_id"], edited.revision.as_str());
    assert_eq!(report["frame_rate"], json!([30000, 1001]));
    assert_eq!(report["range"], json!([0, FINAL_FRAMES]));
    assert_eq!(report["raster"], json!([320, 180]));
    assert_eq!(report["movie"]["video_frames"], FINAL_FRAMES);
    assert_eq!(report["movie"]["expected_video_frames"], FINAL_FRAMES);
    assert_eq!(report["movie"]["expected_audio_samples"], FINAL_SAMPLES);
    assert_eq!(
        report["movie"]["audio_edits"]["presented_media_ticks"],
        FINAL_SAMPLES
    );
    assert_eq!(report["movie"]["audio"]["presented_end"], FINAL_SAMPLES);
    assert_eq!(report["movie"]["picture_anomalies"], json!([]));
    assert_eq!(report["movie"]["audio"]["gaps"], json!([]));
    assert_eq!(report["summary"]["pictures_checked"], FINAL_FRAMES);
    assert_eq!(report["summary"]["audio_windows_checked"], windows.len());
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    assert!(
        report["summary"]["signal_windows"]
            .as_u64()
            .ok_or("signal_windows")?
            >= 2,
        "encoded verification must actually compare prefix and suffix signal"
    );
    let pictures = report["pictures"].as_array().ok_or("pictures")?;
    assert_eq!(pictures.len(), FINAL_FRAMES as usize);
    for (frame, picture) in pictures.iter().enumerate() {
        let frame = frame as i64;
        assert_eq!(picture["output_frame"], frame);
        assert_eq!(picture["project_frame"], frame);
        assert_eq!(picture["passed"], true);
        let provenance = &picture["provenance"];
        if (CUT_FRAME..CUT_FRAME + EDITS).contains(&frame) {
            assert_eq!(provenance["kind"], "background");
        } else {
            let expected = if frame < CUT_FRAME {
                frame
            } else {
                frame - EDITS
            };
            assert_eq!(provenance["kind"], "original");
            assert_eq!(provenance["asset"], source.asset.as_str());
            assert_eq!(provenance["source_frame"], expected);
            assert_eq!(provenance["source_pts"], expected * 1001);
        }
    }
    let audio = report["audio"].as_array().ok_or("audio windows")?;
    assert_eq!(audio.len(), windows.len());
    for (check, (start, end)) in audio.iter().zip(&windows) {
        assert_eq!(check["window"], json!([start, end]));
        assert_eq!(check["project_samples"], json!([start, end]));
        assert_eq!(check["passed"], true);
    }
    for index in [0, windows.len() - 1] {
        assert_eq!(audio[index]["kind"], "signal");
        assert_eq!(audio[index]["offset_status"], "verified_zero");
        assert_eq!(audio[index]["measured_offset_samples"], 0);
    }
    run.check_storage()?;
    Ok(json!({"timing": timing, "movie_sha256": sha256(&movie)?,
        "movie_bytes": fs::metadata(&movie)?.len(), "verification_report": report_path,
        "summary": report["summary"]}))
}

/// Independent integer comparison of every emitted PTS and duration. This
/// reads no project plan and uses no production frame/sample clock conversion.
fn check_emitted_timing(run: &Run, movie: &Path) -> Result<Value> {
    let limits = DecodeLimits {
        max_input_bytes: 1024 * 1024 * 1024,
        max_frames: 10_102,
        max_packets: 100_000,
        max_pixels: 320 * 192,
        max_dimension: 320,
        ..DecodeLimits::default()
    };
    let file = File::open(movie)?;
    let inspection = inspect_mp4(&file, limits, run.control()?)?;
    for kind in [Mp4TrackKind::Video, Mp4TrackKind::Audio] {
        let track = inspection
            .tracks
            .iter()
            .find(|track| track.kind == kind)
            .ok_or("missing emitted track")?;
        assert_eq!(
            track.edits.len(),
            1,
            "one real presentation edit per stream"
        );
        assert!(track.edits[0].media_time >= 0);
        let actual = i128::from(track.edits[0].segment_duration) * 30_000;
        let expected = i128::from(FINAL_FRAMES) * 1001 * i128::from(inspection.movie_timescale);
        assert_eq!(actual, expected, "emitted presentation duration {kind:?}");
        if kind == Mp4TrackKind::Audio {
            assert_eq!(track.media_timescale, 48_000);
            assert_eq!(
                i128::from(track.edits[0].segment_duration) * 48_000,
                i128::from(FINAL_SAMPLES) * i128::from(inspection.movie_timescale)
            );
        }
    }
    let mut decoder = SourceDecoder::open(File::open(movie)?, limits, run.control()?)?;
    let numerator = i128::from(decoder.info().time_base_num);
    let denominator = i128::from(decoder.info().time_base_den);
    let mut count = 0_i64;
    let mut terminal = None;
    while let Some(frame) = decoder.next_metadata(run.control()?)? {
        assert!(count < FINAL_FRAMES, "extra emitted picture");
        assert_eq!(
            i128::from(frame.pts) * numerator * 30_000,
            i128::from(count) * 1001 * denominator,
            "emitted picture PTS {count}"
        );
        let duration = frame
            .reported_duration
            .ok_or("emitted picture has no reported duration")?;
        assert_eq!(
            i128::from(duration) * numerator * 30_000,
            1001 * denominator,
            "emitted picture duration {count}"
        );
        terminal = Some(frame.pts + duration);
        count += 1;
    }
    assert_eq!(count, FINAL_FRAMES);
    assert_eq!(
        i128::from(terminal.ok_or("empty emitted video")?) * numerator * 30_000,
        i128::from(FINAL_FRAMES) * 1001 * denominator
    );
    Ok(
        json!({"pictures_checked": count, "time_base": [numerator.to_string(), denominator.to_string()],
        "terminal_pts": terminal, "audio_presented_samples": FINAL_SAMPLES}),
    )
}
