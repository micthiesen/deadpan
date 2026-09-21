//! Worker-side access to verified, original-rate PCM. This adapter owns no
//! authored state and does not admit a source merely because its hash matches.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::AudioSample;
use deadpan_media::audio_index::{AudioChannelLayout, AudioIndexSnapshot};
use deadpan_media::audio_session::{AudioSession, SourceAudioSample};

use crate::{
    PcmWindow, PreparationError, ResampleRecipe, Resampler, StereoBlock, StereoMatrix, check_cancel,
};

/// A private PCM session whose complete reopened index matches retained source
/// qualification. Construct and use it on a preparation worker, never a device
/// callback. The immutable index remains available as preparation provenance.
pub struct PreparedSource {
    session: AudioSession,
    matrix: StereoMatrix,
}

impl PreparedSource {
    pub fn new(
        session: AudioSession,
        expected: &AudioIndexSnapshot,
        cancelled: &AtomicBool,
    ) -> Result<Self, PreparationError> {
        let layout = session.index().stream().channel_layout;
        Self::with_layout(session, expected, layout, cancelled)
    }

    /// Interpret unspecified source channels using an explicit host choice.
    /// A native source layout cannot be overridden and channel counts must
    /// agree. This does not persist an authored override or provide its UI.
    /// Cache provenance must include this chosen layout and [`crate::MATRIX_ID`]
    /// as well as the complete source index and resampling recipe.
    pub fn with_layout(
        session: AudioSession,
        expected: &AudioIndexSnapshot,
        layout: AudioChannelLayout,
        cancelled: &AtomicBool,
    ) -> Result<Self, PreparationError> {
        verify_index(session.index(), expected, || check_cancel(cancelled))?;
        let source_layout = session.index().stream().channel_layout;
        if source_layout.channels() != layout.channels()
            || matches!(source_layout, AudioChannelLayout::Native { .. } if source_layout != layout)
        {
            return Err(PreparationError::UnsupportedLayout);
        }
        let matrix = StereoMatrix::new(layout)?;
        check_cancel(cancelled)?;
        Ok(Self { session, matrix })
    }

    pub fn index(&self) -> &AudioIndexSnapshot {
        self.session.index()
    }

    pub fn matrix_layout(&self) -> AudioChannelLayout {
        self.matrix.layout()
    }

    /// Prepare one bounded output block using an exact origin-based recipe.
    /// Kernel context is read once from the private original-rate cache. Only
    /// context outside the authored trim is zero-extended by the resampler;
    /// unavailable coverage inside the trim remains a media error.
    pub fn prepare(
        &self,
        recipe: ResampleRecipe,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<StereoBlock, PreparationError> {
        check_cancel(cancelled)?;
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(PreparationError::InvalidRecipe("audio read time budget"));
        }
        let resampler = Resampler::new(recipe, self.matrix.clone());
        let window = resampler
            .required_source_range(start, frames)?
            .map(|range| {
                let frame_count = range
                    .end
                    .checked_sub(range.start)
                    .and_then(|count| u32::try_from(count).ok())
                    .ok_or(PreparationError::InvalidSamples)?;
                let block = self.session.read_samples(
                    SourceAudioSample(range.start),
                    frame_count,
                    timeout,
                    cancelled,
                )?;
                check_cancel(cancelled)?;
                let stream = self.index().stream();
                let expected_samples = usize::try_from(frame_count)
                    .ok()
                    .and_then(|count| {
                        count.checked_mul(usize::try_from(stream.channel_layout.channels()).ok()?)
                    })
                    .ok_or(PreparationError::InvalidSamples)?;
                if block.start.0 != range.start
                    || block.sample_rate != stream.sample_rate
                    || block.channel_layout != stream.channel_layout
                    || block.samples.len() != expected_samples
                {
                    return Err(PreparationError::IndexMismatch);
                }
                Ok(PcmWindow {
                    start: range.start,
                    samples: block.samples,
                })
            })
            .transpose()?;
        resampler.render(start, frames, window, cancelled)
    }
}

fn verify_index(
    actual: &AudioIndexSnapshot,
    expected: &AudioIndexSnapshot,
    mut check: impl FnMut() -> Result<(), PreparationError>,
) -> Result<(), PreparationError> {
    // Public construction/deserialization already enforces the sole supported
    // schema and decoder contract. Compare every remaining retained field,
    // including derived offsets rather than relying on content identity alone.
    check()?;
    if actual.content() != expected.content()
        || actual.stream() != expected.stream()
        || actual.decoded_samples() != expected.decoded_samples()
        || actual.valid_samples() != expected.valid_samples()
        || actual.observations().len() != expected.observations().len()
        || actual.frames().len() != expected.frames().len()
    {
        return Err(PreparationError::IndexMismatch);
    }
    for (actual, expected) in actual
        .observations()
        .chunks(1024)
        .zip(expected.observations().chunks(1024))
    {
        check()?;
        if actual != expected {
            return Err(PreparationError::IndexMismatch);
        }
    }
    for (actual, expected) in actual
        .frames()
        .chunks(1024)
        .zip(expected.frames().chunks(1024))
    {
        check()?;
        if actual != expected {
            return Err(PreparationError::IndexMismatch);
        }
    }
    check()
}

#[cfg(test)]
mod tests {
    use deadpan_core::SourceTimeBase;
    use deadpan_media::audio_index::{
        AudioChannelLayout, AudioFrameObservation, AudioStreamDescriptor,
    };
    use deadpan_media::source_index::SourceContentIdentity;

    use super::*;

    #[test]
    fn index_comparison_checks_cancellation_through_raw_and_derived_chunks() {
        let index = AudioIndexSnapshot::new(
            SourceContentIdentity::new([7; 32], 100).unwrap(),
            AudioStreamDescriptor {
                stream_index: 0,
                codec: "pcm_s16le".into(),
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                sample_rate: 48_000,
                channel_layout: AudioChannelLayout::Native {
                    channels: 2,
                    mask: 3,
                },
                stream_start: Some(0),
                stream_duration: Some(2049),
                initial_padding: 0,
                trailing_padding: 0,
                seek_preroll: 0,
            },
            (0..2049)
                .map(|pts| AudioFrameObservation {
                    pts,
                    discard: false,
                    decode_timestamp: Some(pts),
                    reported_duration: Some(1),
                    sample_count: 1,
                    sample_format: "s16".into(),
                    skip_samples: None,
                })
                .collect(),
        )
        .unwrap();
        // Entry, three observation chunks, three derived chunks, final check.
        for stop_after in 1..=8 {
            let mut checks = 0;
            assert!(matches!(
                verify_index(&index, &index, || {
                    checks += 1;
                    if checks == stop_after {
                        Err(PreparationError::Cancelled)
                    } else {
                        Ok(())
                    }
                }),
                Err(PreparationError::Cancelled)
            ));
            assert_eq!(checks, stop_after);
        }
        verify_index(&index, &index, || Ok(())).unwrap();
    }
}
