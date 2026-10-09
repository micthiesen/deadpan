//! Family-zero Opus in ISO BMFF. Packet timing and edits remain exact samples;
//! a dOps PreSkip alone never substitutes for a presentation edit.

use super::*;
use crate::audio::Mp4OpusFraming;

pub(super) fn configuration(
    r: &mut Reader<'_>,
    span: Span,
    track: &Track,
) -> Result<Mp4OpusFraming> {
    r.fixed(span, 11)?;
    let bytes = r.bytes::<11>(span.start)?;
    let channels = u32::from(bytes[1]);
    require(
        bytes[0] == 0 && bytes[10] == 0 && (1..=2).contains(&channels),
        "MP4 Opus requires version-zero mapping-family-zero mono or stereo dOps",
    )?;
    require(
        track.audio_channels == Some(channels) && track.audio_sample_rate == Some(48_000),
        "MP4 Opus channel/rate declarations disagree",
    )?;
    // InputSampleRate is informational in the Opus mapping. Zero (unknown)
    // and other original input rates do not change the decoder's 48 kHz clock.
    Ok(Mp4OpusFraming {
        channels,
        pre_skip: u32::from(u16::from_be_bytes([bytes[2], bytes[3]])),
        packet_count: 0,
        decoded_sample_count: 0,
        first_sample: 0,
        valid_samples: 0,
    })
}

#[derive(Default)]
pub(super) struct Timing {
    row: u32,
    remaining: u32,
    duration: u32,
}

impl Timing {
    pub(super) fn packet(
        &mut self,
        r: &mut Reader<'_>,
        table: Table,
        last: bool,
        samples: u64,
    ) -> Result<()> {
        if self.remaining == 0 {
            require(self.row < table.rows, "Opus packet exceeds timing table")?;
            let at = table.data.start + u64::from(self.row) * 8;
            self.remaining = r.u32(at)?;
            self.duration = r.u32(at + 4)?;
            self.row += 1;
        }
        require(
            self.remaining > 0
                && self.duration > 0
                && if last {
                    u64::from(self.duration) <= samples
                } else {
                    u64::from(self.duration) == samples
                },
            "Opus packet duration disagrees with physical samples",
        )?;
        self.remaining -= 1;
        Ok(())
    }
}

pub(super) fn validate_timing(
    r: &mut Reader<'_>,
    track: &mut Track,
    movie_scale: u32,
) -> Result<()> {
    let Some(framing) = &mut track.opus else {
        return Ok(());
    };
    require(
        track.media_timescale == 48_000 && track.composition.is_none() && track.sync.is_none(),
        "MP4 Opus requires its exact 48000 Hz clock and synchronous packets",
    )?;
    require(
        track.roll_distance.is_some_and(|distance| distance < 0),
        "MP4 Opus requires negative roll recovery",
    )?;
    let (offset, edit) = match track.edit_entries.as_slice() {
        [edit] => (0, edit),
        [empty, edit] if empty.media_time == -1 => (empty.segment_duration, edit),
        _ => {
            return Err(invalid(
                "MP4 Opus requires one presentation edit with optional initial offset",
            ));
        }
    };
    let samples = |ticks: u64| -> Result<u64> {
        let scaled = u128::from(ticks) * 48_000;
        require(
            scaled.is_multiple_of(u128::from(movie_scale)),
            "MP4 Opus movie edit is not sample-exact",
        )?;
        u64::try_from(scaled / u128::from(movie_scale))
            .map_err(|_| limit("Opus edit overflows sample clock"))
    };
    require(
        edit.media_time == i64::from(framing.pre_skip),
        "MP4 Opus edit start differs from declared pre-skip",
    )?;
    let duration = samples(edit.segment_duration)?;
    require(
        duration > 0
            && duration.checked_add(u64::from(framing.pre_skip)) == Some(track.timing_duration),
        "MP4 Opus presentation edit disagrees with packet timing",
    )?;
    framing.first_sample = i64::try_from(samples(offset)?)
        .map_err(|_| limit("Opus offset overflows sample clock"))?
        .checked_sub(i64::from(framing.pre_skip))
        .ok_or_else(|| limit("Opus offset overflows sample clock"))?;
    framing.valid_samples = duration;
    r.check()
}
