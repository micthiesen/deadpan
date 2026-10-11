//! Private worker-owned PCM. Long outputs are unlinked temporary files, never
//! authored assets or reusable cross-process cache entries. Handles pin reads.

use std::fs::File;
use std::io::{self, Write};
use std::ops::Range;
use std::os::unix::fs::FileExt;
use std::sync::atomic::AtomicBool;

use super::{StageAudioError, WorkControl};

const IO_FRAMES: usize = 2048;
pub(super) const FILE_BUFFER_FRAMES: u64 = IO_FRAMES as u64;

pub(super) enum PreparedPcm {
    Memory(Vec<[f32; 2]>),
    File { file: File, frames: usize },
    Silence(usize),
}

impl PreparedPcm {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Memory(samples) => samples.len(),
            Self::File { frames, .. } | Self::Silence(frames) => *frames,
        }
    }

    pub(super) fn resident_frames(&self) -> u64 {
        match self {
            Self::Memory(samples) => samples.len() as u64,
            Self::File { .. } | Self::Silence(_) => 0,
        }
    }

    pub(super) fn disk_bytes(&self) -> u64 {
        match self {
            Self::File { frames, .. } => *frames as u64 * 8,
            Self::Memory(_) | Self::Silence(_) => 0,
        }
    }
}

/// Both short PCM and cached output feed exactly the same resampler window.
pub(super) trait PcmSamples {
    fn read_window(
        &self,
        range: Range<usize>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<f32>, StageAudioError>;
}

impl PcmSamples for [[f32; 2]] {
    fn read_window(
        &self,
        range: Range<usize>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<f32>, StageAudioError> {
        check_window(&range, self.len(), cancelled)?;
        Ok(self
            .get(range)
            .ok_or(crate::PreparationError::InvalidSamples)?
            .iter()
            .flat_map(|frame| *frame)
            .collect())
    }
}

impl PcmSamples for Vec<[f32; 2]> {
    fn read_window(
        &self,
        range: Range<usize>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<f32>, StageAudioError> {
        self.as_slice().read_window(range, cancelled)
    }
}

impl PcmSamples for PreparedPcm {
    fn read_window(
        &self,
        range: Range<usize>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<f32>, StageAudioError> {
        check_window(&range, self.len(), cancelled)?;
        match self {
            Self::Memory(samples) => samples.read_window(range, cancelled),
            Self::Silence(_) => Ok(vec![0.0; (range.end - range.start) * 2]),
            Self::File { file, .. } => {
                let mut samples = Vec::with_capacity((range.end - range.start) * 2);
                let mut buffer = [0_u8; IO_FRAMES * 8];
                let mut cursor = range.start;
                while cursor < range.end {
                    crate::check_cancel(cancelled)?;
                    let frames = (range.end - cursor).min(IO_FRAMES);
                    file.read_exact_at(&mut buffer[..frames * 8], cursor as u64 * 8)?;
                    deadpan_diagnostics::IO
                        .pcm_cache_file
                        .read(frames as u64 * 8);
                    for bytes in buffer[..frames * 8].chunks_exact(4) {
                        let value = f32::from_le_bytes(bytes.try_into().expect("four-byte chunk"));
                        if !value.is_finite() {
                            return Err(crate::PreparationError::InvalidSamples.into());
                        }
                        samples.push(value);
                    }
                    cursor += frames;
                }
                Ok(samples)
            }
        }
    }
}

fn check_window(
    range: &Range<usize>,
    length: usize,
    cancelled: &AtomicBool,
) -> Result<(), StageAudioError> {
    crate::check_cancel(cancelled)?;
    if range.start > range.end
        || range.end > length
        || range.end - range.start > crate::MAX_SOURCE_FRAMES as usize
    {
        return Err(crate::PreparationError::InvalidSamples.into());
    }
    Ok(())
}

pub(super) struct PcmWriter {
    pcm: PreparedPcm,
    expected: usize,
    written: usize,
    // Own one fixed write buffer; no per-quantum buffering allocation.
    buffer: Vec<u8>,
    buffered: usize,
    failed: bool,
}

impl PcmWriter {
    pub(super) fn new(frames: u32, memory_limit: u32) -> Result<Self, StageAudioError> {
        let disk = frames > memory_limit;
        Ok(Self {
            pcm: if disk {
                PreparedPcm::File {
                    file: tempfile::tempfile()?,
                    frames: frames as usize,
                }
            } else {
                PreparedPcm::Memory(Vec::with_capacity(frames as usize))
            },
            expected: frames as usize,
            written: 0,
            buffer: if disk {
                vec![0; IO_FRAMES * 8]
            } else {
                Vec::new()
            },
            buffered: 0,
            failed: false,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.written
    }

    pub(super) fn push(
        &mut self,
        samples: &[[f32; 2]],
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        if self.failed {
            return Err(io::Error::other("prepared PCM writer has failed").into());
        }
        control.check()?;
        if samples.len() > self.expected - self.written
            || samples.iter().flatten().any(|sample| !sample.is_finite())
        {
            return Err(crate::PreparationError::InvalidSamples.into());
        }
        let result: Result<(), StageAudioError> = (|| {
            match &mut self.pcm {
                PreparedPcm::Memory(output) => output.extend_from_slice(samples),
                PreparedPcm::File { file, .. } => {
                    for frame in samples {
                        for value in frame {
                            self.buffer[self.buffered..self.buffered + 4]
                                .copy_from_slice(&value.to_le_bytes());
                            self.buffered += 4;
                        }
                        if self.buffered == self.buffer.len() {
                            control.check()?;
                            file.write_all(&self.buffer)?;
                            deadpan_diagnostics::IO
                                .pcm_cache_file
                                .write(self.buffered as u64);
                            self.buffered = 0;
                        }
                    }
                }
                PreparedPcm::Silence(_) => unreachable!("writer owns storage"),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.failed = true;
            return Err(error);
        }
        self.written += samples.len();
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        control: WorkControl<'_>,
    ) -> Result<PreparedPcm, StageAudioError> {
        control.check()?;
        if self.failed || self.written != self.expected {
            return Err(crate::PreparationError::InvalidSamples.into());
        }
        if let PreparedPcm::File { file, frames } = &mut self.pcm {
            if self.buffered != 0 {
                file.write_all(&self.buffer[..self.buffered])?;
                deadpan_diagnostics::IO
                    .pcm_cache_file
                    .write(self.buffered as u64);
            }
            if file.metadata()?.len() != *frames as u64 * 8 {
                return Err(io::Error::other("incomplete prepared PCM cache").into());
            }
        }
        control.check()?;
        Ok(self.pcm)
    }
}

#[cfg(test)]
mod tests {
    use super::super::ReadWork;
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    fn with_control(run: impl FnOnce(WorkControl<'_>)) {
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        run(WorkControl {
            work: &work,
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(10),
        });
    }

    fn fixture(frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|at| {
                [
                    if at % 2 == 0 {
                        -0.0
                    } else {
                        at as f32 / 1024.0
                    },
                    -(at as f32) / 8192.0,
                ]
            })
            .collect()
    }

    #[test]
    fn disk_pcm_matches_memory_bits_across_flushes_and_random_windows() {
        with_control(|control| {
            let values = fixture(10_003);
            let mut writer = PcmWriter::new(values.len() as u32, 0).unwrap();
            for chunk in values.chunks(173) {
                writer.push(chunk, control).unwrap();
            }
            let pcm = writer.finish(control).unwrap();
            assert_eq!(pcm.len(), values.len());
            assert_eq!(pcm.disk_bytes(), values.len() as u64 * 8);
            assert_eq!(pcm.resident_frames(), 0);
            let PreparedPcm::File { file, .. } = &pcm else {
                panic!("disk");
            };
            assert_eq!(
                file.metadata().unwrap().nlink(),
                0,
                "no cache pathname survives"
            );
            for range in [9999..10003, 0..1, 2011..6123, 0..10003, 10003..10003] {
                let actual = pcm.read_window(range.clone(), control.cancelled).unwrap();
                let expected = values.read_window(range, control.cancelled).unwrap();
                assert_eq!(
                    actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                );
            }
            let mut short = PcmWriter::new(17, 17).unwrap();
            short.push(&values[..17], control).unwrap();
            let short = short.finish(control).unwrap();
            assert_eq!(short.resident_frames(), 17);
            assert_eq!(short.disk_bytes(), 0);
        });
    }

    #[test]
    fn incomplete_invalid_cancelled_and_failed_writes_never_publish_pcm() {
        with_control(|control| {
            let mut short = PcmWriter::new(2, 0).unwrap();
            short.push(&[[0.0; 2]], control).unwrap();
            assert!(short.finish(control).is_err());
            let mut writer = PcmWriter::new(2, 0).unwrap();
            assert!(writer.push(&[[f32::NAN; 2]], control).is_err());
            assert!(writer.push(&[[0.0; 2]; 3], control).is_err());
            assert_eq!(writer.len(), 0);
            control.cancelled.store(true, Ordering::Relaxed);
            assert!(
                writer
                    .push(&[[0.0; 2]], control)
                    .unwrap_err()
                    .is_cancelled()
            );
            control.cancelled.store(false, Ordering::Relaxed);
            writer.push(&[[0.0; 2]; 2], control).unwrap();
            let named = tempfile::NamedTempFile::new().unwrap();
            writer.pcm = PreparedPcm::File {
                file: File::open(named.path()).unwrap(),
                frames: 2,
            };
            assert!(matches!(
                writer.finish(control),
                Err(StageAudioError::Cache(_))
            ));
            let mut failed = PcmWriter::new(IO_FRAMES as u32, 0).unwrap();
            failed.pcm = PreparedPcm::File {
                file: File::open(named.path()).unwrap(),
                frames: IO_FRAMES,
            };
            assert!(matches!(
                failed.push(&fixture(IO_FRAMES), control),
                Err(StageAudioError::Cache(_))
            ));
            assert!(matches!(
                failed.push(&[[0.0; 2]], control),
                Err(StageAudioError::Cache(_))
            ));
            assert!(failed.finish(control).is_err());
        });
    }

    #[test]
    fn damaged_or_cancelled_disk_reads_reject_instead_of_returning_partial_pcm() {
        with_control(|control| {
            let mut writer = PcmWriter::new(4097, 0).unwrap();
            writer.push(&fixture(4097), control).unwrap();
            let pcm = writer.finish(control).unwrap();
            assert!(pcm.read_window(4096..4098, control.cancelled).is_err());
            control.cancelled.store(true, Ordering::Relaxed);
            assert!(
                pcm.read_window(0..1, control.cancelled)
                    .unwrap_err()
                    .is_cancelled()
            );
            control.cancelled.store(false, Ordering::Relaxed);
            let PreparedPcm::File { file, .. } = &pcm else {
                panic!("disk");
            };
            file.write_all_at(&f32::NAN.to_le_bytes(), 8).unwrap();
            assert!(matches!(
                pcm.read_window(0..3, control.cancelled),
                Err(StageAudioError::Preparation(
                    crate::PreparationError::InvalidSamples
                ))
            ));
            file.set_len(4096 * 8).unwrap();
            assert!(matches!(
                pcm.read_window(4095..4097, control.cancelled),
                Err(StageAudioError::Cache(_))
            ));
        });
    }
}
