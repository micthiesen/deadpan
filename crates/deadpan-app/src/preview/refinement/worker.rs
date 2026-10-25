use super::*;
use deadpan_core::{SourceSpan, SourceTimeBase, SourceTimestamp};
use deadpan_store::{AccessMode, ProjectStore};

pub(super) enum Failure {
    NeedsModel,
    Message(String),
}
fn failure(error: impl std::fmt::Display) -> Failure {
    Failure::Message(error.to_string())
}

pub(super) fn run(
    capture: &Capture,
    cancel: &AtomicBool,
    handle: &crate::jobs::JobHandle,
) -> Result<Prepared, Failure> {
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                if capture.interrupted() {
                    cancel.store(true, Ordering::Release);
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(20));
            }
        });
        struct Finish<'a>(&'a AtomicBool);
        impl Drop for Finish<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _finish = Finish(&done);
        prepare(capture, cancel, handle)
    })
}

fn prepare(
    capture: &Capture,
    cancel: &AtomicBool,
    handle: &crate::jobs::JobHandle,
) -> Result<Prepared, Failure> {
    let deadline = Instant::now() + DEADLINE;
    handle.set_progress("Verifying the local word timing model", None);
    let models =
        deadpan_cli::models::installed_refinement_models(&capture.model_root, cancel, deadline)
            .map_err(failure)?
            .ok_or(Failure::NeedsModel)?;
    if cancel.load(Ordering::Acquire) || capture.interrupted() {
        return Err(failure("Local refinement was cancelled or interrupted"));
    }
    handle.acquire(Some(cancel)).map_err(failure)?;
    handle.set_progress(
        "Reading the Original and proposing approximate word edges",
        None,
    );
    let store =
        ProjectStore::open(&capture.workspace.path, AccessMode::ReadOnly).map_err(failure)?;
    if store.head_revision().map_err(failure)? != *capture.workspace.document.revision_id() {
        return Err(failure("The edit changed before refinement started"));
    }
    let runtime =
        deadpan_cli::refinement::RefinementRuntime::beside_current_executable().map_err(failure)?;
    let result = deadpan_cli::refinement::prepare(
        &store,
        &capture.transcript.key,
        capture.word..capture.word + 1,
        &runtime,
        &models,
        cancel,
        deadline,
    )
    .map_err(failure)?;
    if capture.transcript.selection_guard.as_ref() != Some(&result.selection_guard)
        || result.expected_revision != *capture.workspace.document.revision_id()
        || result.expected_version != capture.corrections.version()
    {
        return Err(failure(
            "The transcript or corrections changed before refinement completed",
        ));
    }
    if cancel.load(Ordering::Acquire) || capture.interrupted() {
        return Err(failure("Local refinement was cancelled or interrupted"));
    }
    let window = &result.window;
    let from = source_boundary(
        window.analysis_origin,
        window.analysis_offset,
        window.source_rate,
        false,
    )
    .map_err(failure)?
    .max(window.source_range.start);
    let end = window
        .analysis_offset
        .checked_add(window.samples.len() as u64)
        .ok_or_else(|| failure("analysis window endpoint overflow"))?;
    let to = source_boundary(window.analysis_origin, end, window.source_rate, true)
        .map_err(failure)?
        .min(window.source_range.end);
    let time_base = SourceTimeBase::new(1, window.source_rate).map_err(failure)?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: from,
            time_base,
        },
        SourceTimestamp {
            ticks: to,
            time_base,
        },
    )
    .map_err(failure)?;
    let audition = deadpan_playback::AudioRange::new(
        capture.workspace.document.presentation_basis().frame_rate,
        capture.source.asset.clone(),
        capture.source.receipt.clone(),
        span,
    )
    .map_err(failure)?;
    let waveform = waveform(&window.samples);
    Ok(Prepared {
        review: Arc::new(result.review),
        waveform,
        audition: Arc::new(audition),
        source_start: from,
        analysis_origin: window.analysis_origin,
        source_rate: window.source_rate,
        #[cfg(feature = "ui-harness")]
        engine: result.runtime.engine,
    })
}

/// Whole source samples enclose the exact global 16 kHz window for audition.
/// This outward enclosure never feeds back into proposed annotation timing.
fn source_boundary(origin: i64, sample: u64, rate: u32, ceil: bool) -> Result<i64, String> {
    let numerator = i128::from(origin)
        .checked_mul(16_000)
        .and_then(|base| {
            i128::from(sample)
                .checked_mul(i128::from(rate))
                .and_then(|offset| base.checked_add(offset))
        })
        .ok_or("audition source coordinate overflow")?;
    let value = if ceil {
        numerator
            .checked_add(15_999)
            .ok_or("audition endpoint overflow")?
            .div_euclid(16_000)
    } else {
        numerator.div_euclid(16_000)
    };
    i64::try_from(value).map_err(|error| error.to_string())
}

fn waveform(samples: &[f32]) -> Vec<(f32, f32)> {
    samples
        .chunks(samples.len().div_ceil(1024).max(1))
        .map(|chunk| {
            chunk
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), sample| {
                    (low.min(*sample), high.max(*sample))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audition_enclosure_keeps_fractional_source_phase_and_negative_origins() {
        assert_eq!(source_boundary(-123, 1, 44_100, false).unwrap(), -121);
        assert_eq!(source_boundary(-123, 1, 44_100, true).unwrap(), -120);
        assert_eq!(
            source_boundary(-123, 16_000, 44_100, false).unwrap(),
            43_977
        );
        assert_eq!(source_boundary(-123, 16_000, 44_100, true).unwrap(), 43_977);
    }
    #[test]
    fn waveform_is_measured_bounded_and_does_not_normalize_the_input() {
        let samples = vec![0.25; 480_000];
        let measured = waveform(&samples);
        assert!(measured.len() <= 1024);
        assert!(measured.iter().all(|pair| *pair == (0.25, 0.25)));
        assert_eq!(waveform(&[-0.5, 1.2]), vec![(-0.5, -0.5), (1.2, 1.2)]);
    }
}
