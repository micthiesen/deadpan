use deadpan_cli::audio::{OfflineAudioError, OfflineAudioSession, ProjectAudioSession};
use deadpan_core::{AudioSample, FrameRange, ProjectFrame, RevisionId};
use serde_json::{Value, json};

use super::{
    CUT_FRAME, CUT_SAMPLE, EDITS, FINAL_FRAMES, FINAL_SAMPLES, SOURCE_FRAMES, boundary,
    edits::Edited,
    fixture::Fixture,
    support::{Result, Run},
};

// Well below one source PCM16 step. The nonperiodic, channel-distinct waveform
// exposes a one-sample shift without demanding identical floating point DSP.
const PCM_TOLERANCE: f32 = 0.000_002;

pub fn check(run: &Run, source: &Fixture, edited: &Edited) -> Result<Value> {
    let mut residues = [false; 5];
    let mut fractional_max_error = 0_f32;
    let mut one_sample_shift_errors = [0_f32; 5];
    for (inserted, revision) in &edited.checkpoints {
        residues[(inserted % 5) as usize] = true;
        let result = checkpoint(run, source, *inserted, revision)?;
        fractional_max_error = fractional_max_error.max(result.maximum_error);
        let residue = (inserted % 5) as usize;
        one_sample_shift_errors[residue] =
            one_sample_shift_errors[residue].max(result.one_sample_shift_error);
    }
    assert!(
        residues.into_iter().all(|seen| seen),
        "actual PCM must cover every fractional residue"
    );
    assert!(
        one_sample_shift_errors
            .into_iter()
            .all(|error| error > 100.0 * PCM_TOLERANCE),
        "every residue must distinguish a one-sample phase error: {one_sample_shift_errors:?}"
    );

    let mut audition = ProjectAudioSession::open_revision(&source.package, &edited.revision)?;
    let mut offline = open_offline(run, source, &edited.revision)?;
    assert_eq!(
        offline.sample_range(),
        AudioSample(0)..AudioSample(FINAL_SAMPLES)
    );
    assert_eq!(offline.sample_count(), FINAL_SAMPLES as u64);
    assert_eq!(offline.range().end(), ProjectFrame(FINAL_FRAMES));
    let mut cursor = 0_i64;
    let mut audition_max_error = 0_f32;
    let mut before_peak = 0_f32;
    let mut after_peak = 0_f32;
    let mut channel_difference = 0_f32;
    while cursor < FINAL_SAMPLES {
        run.check("complete audition PCM")?;
        // This public audition inspection API caps reads at 256. Variation is
        // exercised above; use its maximum for complete sample coverage.
        let count = 256_u32.min((FINAL_SAMPLES - cursor) as u32);
        let block = audition.read_limited(AudioSample(cursor), count, &run.cancelled)?;
        assert_eq!(block.revision_id, edited.revision);
        assert_eq!(block.start, AudioSample(cursor));
        assert_eq!(block.samples.len(), count as usize);
        audition_max_error =
            audition_max_error.max(compare(source, EDITS, cursor, &block.samples)?);
        for (offset, pair) in block.samples.iter().enumerate() {
            let sample = cursor + offset as i64;
            let peak = pair[0].abs().max(pair[1].abs());
            if sample < CUT_SAMPLE {
                before_peak = before_peak.max(peak);
            }
            if sample >= 16_096_080 {
                after_peak = after_peak.max(peak);
            }
            channel_difference = channel_difference.max((pair[0] - pair[1]).abs());
        }
        cursor += i64::from(count);
    }
    assert!(
        before_peak > 0.05 && after_peak > 0.05 && channel_difference > 0.05,
        "shared output must contain real, channel-distinct signal before and after pause: {before_peak}, {after_peak}, {channel_difference}"
    );
    drop(audition);
    eprintln!(
        "fractional edits: all {cursor} audition samples match independent source/position oracle"
    );

    cursor = 0;
    let mut turn = 0_usize;
    let mut offline_max_error = 0_f32;
    while cursor < FINAL_SAMPLES {
        run.check("complete offline PCM")?;
        let count = [8192, 1001, 4093, 257][turn % 4].min((FINAL_SAMPLES - cursor) as u32);
        let block = offline.read(AudioSample(cursor), count, &run.cancelled)?;
        assert_eq!(block.revision_id, edited.revision);
        assert_eq!(block.start, AudioSample(cursor));
        assert_eq!(block.samples.len(), count as usize);
        offline_max_error = offline_max_error.max(compare(source, EDITS, cursor, &block.samples)?);
        cursor += i64::from(count);
        turn += 1;
    }
    assert!(
        matches!(
            offline.read(AudioSample(FINAL_SAMPLES), 1, &run.cancelled),
            Err(OfflineAudioError::Range)
        ),
        "offline output must expose no extra terminal sample"
    );
    drop(offline);
    // Repeat nonmonotonic seam requests in a fresh offline session.
    let mut cold = open_offline(run, source, &edited.revision)?;
    for start in [16_096_081, 79_952, 16_095_952, 20_000, FINAL_SAMPLES - 256] {
        let block = cold.read(AudioSample(start), 256, &run.cancelled)?;
        compare(source, EDITS, start, &block.samples)?;
    }
    run.check_storage()?;
    Ok(
        json!({"audition_samples_checked": FINAL_SAMPLES, "offline_samples_checked": FINAL_SAMPLES,
        "audition_max_absolute_error": audition_max_error, "offline_max_absolute_error": offline_max_error,
        "fractional_checkpoint_max_absolute_error": fractional_max_error,
        "one_sample_shift_max_errors_by_residue": one_sample_shift_errors,
        "pcm_reference": "direct independently decoded Original PCM at 80080+n-B(50+i); authored retained resume; no interpolation or alignment",
        "flattened_structural_start_phase_fifths_by_residue": [0,2,-1,1,-2],
        "retained_pcm_start_samples_by_residue": [80080,80080,80080,80080,80080],
        "early_pcm_checkpoint_edits": [1,2,3,4,5],
        "tolerance": PCM_TOLERANCE, "prefix_peak": before_peak, "suffix_peak": after_peak,
        "channel_difference": channel_difference, "phase_residues": [0,1,2,3,4]}),
    )
}

struct Checkpoint {
    maximum_error: f32,
    one_sample_shift_error: f32,
}

/// Validate all five allocation residues before paying for the remaining edits.
/// The final pass still reopens these durable revisions and repeats coverage.
pub fn check_early_checkpoint(
    run: &Run,
    source: &Fixture,
    inserted: i64,
    revision: &RevisionId,
) -> Result {
    assert!((1..=5).contains(&inserted));
    let result = checkpoint(run, source, inserted, revision)?;
    run.write_json(
        &format!("early-pcm-{inserted}.json"),
        &json!({"scope": "early retained-resume PCM checkpoint", "edit": inserted,
            "revision": revision.as_str(), "allocation_residue": inserted % 5,
            "maximum_error": result.maximum_error,
            "one_sample_shift_error": result.one_sample_shift_error,
            "tolerance": PCM_TOLERANCE}),
    )?;
    eprintln!("fractional edits: early PCM checkpoint {inserted}/5 passed");
    Ok(())
}

fn checkpoint(
    run: &Run,
    source: &Fixture,
    inserted: i64,
    revision: &RevisionId,
) -> Result<Checkpoint> {
    run.check("intermediate PCM")?;
    let mut session = ProjectAudioSession::open_revision(&source.package, revision)?;
    let resume = boundary(CUT_FRAME + inserted);
    let total = boundary(SOURCE_FRAMES + inserted);
    assert_eq!(session.plan().audio_duration()?.0, total);
    let mut result = Checkpoint {
        maximum_error: 0.0,
        one_sample_shift_error: 0.0,
    };
    // Read the suffix first in a fresh session so a warm prefix cannot conceal
    // a wrong retained resume. Both sides of the seam are included.
    for (start, count) in [
        (resume + 601, 251),
        (resume - 128, 256),
        (resume, 127),
        (20_000, 17),
        (total - 256, 256),
    ] {
        let block = session.read_limited(AudioSample(start), count, &run.cancelled)?;
        assert_eq!(&block.revision_id, revision);
        assert_eq!(block.start, AudioSample(start));
        assert_eq!(block.samples.len(), count as usize);
        result.maximum_error =
            result
                .maximum_error
                .max(compare(source, inserted, start, &block.samples)?);
        if start == resume + 601 {
            for (offset, actual) in block.samples.iter().enumerate() {
                let wrong = source.pcm[(CUT_SAMPLE + start + offset as i64 - resume + 1) as usize];
                for channel in 0..2 {
                    result.one_sample_shift_error = result
                        .one_sample_shift_error
                        .max((actual[channel] - wrong[channel]).abs());
                }
            }
        }
    }
    assert!(
        result.one_sample_shift_error > 100.0 * PCM_TOLERANCE,
        "edit {inserted} must expose a one-sample phase error: {}",
        result.one_sample_shift_error
    );
    Ok(result)
}

fn open_offline(run: &Run, source: &Fixture, revision: &RevisionId) -> Result<OfflineAudioSession> {
    Ok(OfflineAudioSession::open_revision(
        &source.package,
        revision,
        FrameRange::new(ProjectFrame(0), ProjectFrame(FINAL_FRAMES))?,
        &run.cancelled,
        run.deadline,
    )?)
}

fn compare(source: &Fixture, inserted: i64, start: i64, samples: &[[f32; 2]]) -> Result<f32> {
    let resume = boundary(CUT_FRAME + inserted);
    let mut maximum = 0_f32;
    for (offset, actual) in samples.iter().enumerate() {
        let at = start + offset as i64;
        let expected = if at < CUT_SAMPLE {
            source.pcm[at as usize]
        } else if at < resume {
            [0.; 2]
        } else {
            // InsertTime retains the Original sample at B(50). Hold-duration
            // edits move the rounded allocation boundary without resetting the
            // owned resume. The flattened structural phase is a separate query.
            source.pcm[(CUT_SAMPLE + at - resume) as usize]
        };
        for channel in 0..2 {
            let error = (actual[channel] - expected[channel]).abs();
            if !actual[channel].is_finite() || error > PCM_TOLERANCE {
                return Err(format!("edit {inserted} PCM at output {at}, channel {channel}: actual {}, expected {}, error {error}; suffix allocation starts at {resume}, retained suffix source coordinate is {}", actual[channel], expected[channel], CUT_SAMPLE + at - resume).into());
            }
            if (CUT_SAMPLE..resume).contains(&at) && actual[channel] != 0.0 {
                return Err(format!(
                    "silent Hold contains nonzero PCM at {at}: {}",
                    actual[channel]
                )
                .into());
            }
            maximum = maximum.max(error);
        }
    }
    Ok(maximum)
}
