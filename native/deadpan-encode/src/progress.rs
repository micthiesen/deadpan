use serde::Serialize;

use crate::{AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE, EncodeContract, EncodeError};

/// The exact next input in the common output clock. Picture wins equal clocks.
/// Following this sequence bounds the native muxer's interleave queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NextInput {
    Picture {
        ordinal: u64,
        pts: i64,
        duration: i64,
    },
    Audio {
        first_sample: u64,
        samples: u32,
    },
    Finish,
}

#[derive(Default)]
pub(crate) struct Progress {
    pub(crate) pictures: u64,
    pub(crate) audio: u64,
    pub(crate) poisoned: bool,
}

impl Progress {
    pub(crate) fn next(&self, contract: &EncodeContract) -> Result<NextInput, EncodeError> {
        if self.poisoned {
            return Err(EncodeError::Poisoned);
        }
        self.next_unchecked(contract)
    }

    fn next_unchecked(&self, contract: &EncodeContract) -> Result<NextInput, EncodeError> {
        let pictures_left = self.pictures < contract.video_frames();
        let audio_left = self.audio < contract.audio_samples();
        if !pictures_left && !audio_left {
            return Ok(NextInput::Finish);
        }
        let [numerator, denominator] = contract.frame_rate();
        if pictures_left
            && (!audio_left
                || u128::from(self.pictures)
                    * u128::from(denominator)
                    * u128::from(AUDIO_SAMPLE_RATE)
                    <= u128::from(self.audio) * u128::from(numerator))
        {
            let (pts, duration) = contract.picture_timing(self.pictures)?;
            Ok(NextInput::Picture {
                ordinal: self.pictures,
                pts,
                duration,
            })
        } else {
            let samples = u32::try_from(
                (contract.audio_samples() - self.audio).min(u64::from(AUDIO_FRAME_SAMPLES)),
            )
            .expect("bounded audio block");
            Ok(NextInput::Audio {
                first_sample: self.audio,
                samples,
            })
        }
    }

    pub(crate) fn picture(
        &mut self,
        contract: &EncodeContract,
        timing: (u64, i64, i64),
        bytes: &[u8],
        push: impl FnOnce() -> Result<(), EncodeError>,
    ) -> Result<(), EncodeError> {
        self.step(|state| {
            let (ordinal, pts, duration) = timing;
            if state.next_unchecked(contract)?
                != (NextInput::Picture {
                    ordinal,
                    pts,
                    duration,
                })
            {
                return Err(EncodeError::Input(
                    "picture is not the next exact output timestamp",
                ));
            }
            if u64::try_from(bytes.len()).ok() != Some(contract.picture_bytes()) {
                return Err(EncodeError::Input(
                    "I420 byte length differs from the captured raster",
                ));
            }
            let y_length =
                usize::try_from(u64::from(contract.raster()[0]) * u64::from(contract.raster()[1]))
                    .map_err(|_| EncodeError::Input("I420 plane length exceeds address space"))?;
            if bytes[..y_length]
                .iter()
                .any(|code| !(16..=235).contains(code))
                || bytes[y_length..]
                    .iter()
                    .any(|code| !(16..=240).contains(code))
            {
                return Err(EncodeError::Input(
                    "I420 input contains codes outside the limited range",
                ));
            }
            push()?;
            state.pictures += 1;
            Ok(())
        })
    }

    pub(crate) fn audio(
        &mut self,
        contract: &EncodeContract,
        first: u64,
        left: &[f32],
        right: &[f32],
        push: impl FnOnce() -> Result<(), EncodeError>,
    ) -> Result<(), EncodeError> {
        self.step(|state| {
            let count = u32::try_from(left.len())
                .map_err(|_| EncodeError::Input("audio input exceeds block length"))?;
            if left.len() != right.len()
                || state.next_unchecked(contract)?
                    != (NextInput::Audio {
                        first_sample: first,
                        samples: count,
                    })
            {
                return Err(EncodeError::Input(
                    "audio is not the next exact contiguous output block",
                ));
            }
            if left.iter().chain(right).any(|sample| !sample.is_finite()) {
                return Err(EncodeError::Input(
                    "audio input contains a nonfinite sample",
                ));
            }
            push()?;
            state.audio += u64::from(count);
            Ok(())
        })
    }

    pub(crate) fn finish<T>(
        &mut self,
        contract: &EncodeContract,
        finish: impl FnOnce() -> Result<T, EncodeError>,
    ) -> Result<T, EncodeError> {
        let result = self.step(|state| {
            if state.next_unchecked(contract)? != NextInput::Finish {
                return Err(EncodeError::Input(
                    "finish requires every captured picture and audio sample",
                ));
            }
            finish()
        });
        // Finish is terminal even after success; the safe session consumes itself.
        self.poisoned = true;
        result
    }

    fn step<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, EncodeError>,
    ) -> Result<T, EncodeError> {
        if self.poisoned {
            return Err(EncodeError::Poisoned);
        }
        self.poisoned = true;
        let result = operation(self);
        if result.is_ok() {
            self.poisoned = false;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BFramePolicy, EncoderMode};

    fn contract(frames: u64, samples: u64) -> EncodeContract {
        EncodeContract::new(
            [2, 2],
            [30000, 1001],
            frames,
            samples,
            EncoderMode::Hardware,
            BFramePolicy::TargetTwo,
        )
        .unwrap()
    }
    const BLACK: [u8; 6] = [16, 16, 16, 16, 128, 128];

    #[test]
    fn fractional_clock_interleave_preserves_origin_sample_count_and_short_final_input() {
        let contract = contract(1, 1601); // B(2)-B(1), not B(1)=1602.
        let mut state = Progress::default();
        assert_eq!(
            state.next(&contract).unwrap(),
            NextInput::Picture {
                ordinal: 0,
                pts: 0,
                duration: 1001
            }
        );
        state
            .picture(&contract, (0, 0, 1001), &BLACK, || Ok(()))
            .unwrap();
        assert_eq!(
            state.next(&contract).unwrap(),
            NextInput::Audio {
                first_sample: 0,
                samples: 1024
            }
        );
        state
            .audio(&contract, 0, &[0.; 1024], &[0.; 1024], || Ok(()))
            .unwrap();
        assert_eq!(
            state.next(&contract).unwrap(),
            NextInput::Audio {
                first_sample: 1024,
                samples: 577
            }
        );
        state
            .audio(&contract, 1024, &[0.; 577], &[0.; 577], || Ok(()))
            .unwrap();
        assert_eq!(state.next(&contract).unwrap(), NextInput::Finish);
        assert_eq!(state.finish(&contract, || Ok(73)).unwrap(), 73);
        assert!(matches!(
            state.finish(&contract, || Ok(())),
            Err(EncodeError::Poisoned)
        ));
    }

    #[test]
    fn chronological_input_and_tie_precedence_are_exact() {
        let contract = EncodeContract::new(
            [2, 2],
            [375, 16],
            2,
            4096,
            EncoderMode::Hardware,
            BFramePolicy::TargetTwo,
        )
        .unwrap();
        let mut state = Progress::default();
        let mut events = Vec::new();
        loop {
            let next = state.next(&contract).unwrap();
            events.push(next);
            match next {
                NextInput::Picture {
                    ordinal,
                    pts,
                    duration,
                } => state
                    .picture(&contract, (ordinal, pts, duration), &BLACK, || Ok(()))
                    .unwrap(),
                NextInput::Audio {
                    first_sample,
                    samples,
                } => {
                    let samples = vec![0.; usize::try_from(samples).unwrap()];
                    state
                        .audio(&contract, first_sample, &samples, &samples, || Ok(()))
                        .unwrap();
                }
                NextInput::Finish => break,
            }
        }
        assert_eq!(
            events,
            vec![
                NextInput::Picture {
                    ordinal: 0,
                    pts: 0,
                    duration: 16
                },
                NextInput::Audio {
                    first_sample: 0,
                    samples: 1024
                },
                NextInput::Audio {
                    first_sample: 1024,
                    samples: 1024
                },
                NextInput::Picture {
                    ordinal: 1,
                    pts: 16,
                    duration: 16
                },
                NextInput::Audio {
                    first_sample: 2048,
                    samples: 1024
                },
                NextInput::Audio {
                    first_sample: 3072,
                    samples: 1024
                },
                NextInput::Finish
            ]
        );
    }

    #[test]
    fn missing_duplicate_out_of_order_and_wrong_clock_inputs_poison_without_native_work() {
        let contract = contract(2, 3203);
        for bad in [(1, 1001, 1001), (0, 1, 1001), (0, 0, 1000)] {
            let mut state = Progress::default();
            assert!(
                state
                    .picture(&contract, bad, &BLACK, || panic!(
                        "invalid picture reached native boundary"
                    ))
                    .is_err()
            );
            assert!(matches!(state.next(&contract), Err(EncodeError::Poisoned)));
        }
        let mut state = Progress::default();
        assert!(
            state
                .audio(&contract, 0, &[0.; 1024], &[0.; 1024], || panic!(
                    "audio overtook tied video"
                ))
                .is_err()
        );
        let mut state = Progress::default();
        state
            .picture(&contract, (0, 0, 1001), &BLACK, || Ok(()))
            .unwrap();
        assert!(
            state
                .picture(&contract, (0, 0, 1001), &BLACK, || panic!(
                    "duplicate reached native"
                ))
                .is_err()
        );
        let mut state = Progress::default();
        assert!(
            state
                .finish::<()>(&contract, || panic!(
                    "omitted input finalized native output"
                ))
                .is_err()
        );
        assert!(matches!(state.next(&contract), Err(EncodeError::Poisoned)));
    }

    #[test]
    fn invalid_planes_audio_holes_short_blocks_and_nonfinite_pcm_are_rejected() {
        let contract = contract(1, 1602);
        for bad in [
            &[16, 16, 16, 16, 128][..],
            &[0, 16, 16, 16, 128, 128][..],
            &[16, 16, 16, 16, 128, 241][..],
        ] {
            assert!(
                Progress::default()
                    .picture(&contract, (0, 0, 1001), bad, || panic!(
                        "bad I420 reached native"
                    ))
                    .is_err()
            );
        }
        for (first, length, right_length, bad_sample) in [
            (1, 1024, 1024, 0.),
            (0, 1, 1, 0.),
            (0, 0, 0, 0.),
            (0, 1024, 1023, 0.),
            (0, 1024, 1024, f32::NAN),
            (0, 1024, 1024, f32::INFINITY),
        ] {
            let mut state = Progress::default();
            state
                .picture(&contract, (0, 0, 1001), &BLACK, || Ok(()))
                .unwrap();
            assert!(
                state
                    .audio(
                        &contract,
                        first,
                        &vec![bad_sample; length],
                        &vec![0.; right_length],
                        || panic!("bad audio reached native")
                    )
                    .is_err()
            );
            assert!(matches!(state.next(&contract), Err(EncodeError::Poisoned)));
        }
    }

    #[test]
    fn native_or_control_failure_never_advances_counters_or_allows_finish() {
        let contract = contract(1, 1602);
        for error in [
            EncodeError::Cancelled,
            EncodeError::Deadline,
            EncodeError::Native {
                code: "encode_failure".into(),
                message: "injected after partial native acceptance".into(),
            },
        ] {
            let mut state = Progress::default();
            assert!(
                state
                    .picture(&contract, (0, 0, 1001), &BLACK, || Err(error))
                    .is_err()
            );
            assert_eq!(state.pictures, 0);
            assert!(matches!(
                state.finish::<()>(&contract, || panic!("poisoned session finalized")),
                Err(EncodeError::Poisoned)
            ));
        }
    }
}
