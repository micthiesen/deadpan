//! Deterministic inputs for a bounded encoder capability probe.
//!
//! This fixture is separate from authored media and from the archived developer
//! fixtures. It grants no encoder or publication authority. The host must run
//! encoding in its supervised process and independently inspect the complete
//! emitted file, including these known picture and audio values.

use serde::Serialize;

use crate::{
    AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE, BFramePolicy, ContentLight, EncodeContract,
    EncodeError, EncoderMode, HdrSignal, HdrTransfer, MasteringDisplay,
};

pub const PROBE_VERSION: u32 = 1;
pub const MAX_PROBE_FRAMES: u64 = 121;
pub const MAX_PROBE_SECONDS: u64 = 8;

const COLORS: [[u8; 3]; 5] = [
    [16, 128, 128],
    [235, 128, 128],
    [63, 102, 240],
    [173, 42, 26],
    [32, 240, 118],
];
const MOVING_COLOR: [u8; 3] = [210, 70, 200];

/// Derived observations, not a deserializable admission token. The audio origin
/// is zero and its endpoint is the one origin-based ties-to-even boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeConfig {
    pub version: u32,
    pub raster: [u32; 2],
    pub frame_rate: [u32; 2],
    pub video_frames: u64,
    pub audio_samples: u64,
    pub gop_frames: u32,
    pub picture_bytes: u64,
}

/// One known event in each channel. Indices and amplitudes are in left/right
/// order. The different sample coordinates detect channel exchange as well as
/// delay; a verifier must never realign decoded PCM to these events.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AudioMarker {
    pub samples: [u64; 2],
    pub amplitudes: [f32; 2],
}

/// Owns only small immutable configuration. Filling a picture or audio block
/// allocates nothing and never opens a path, descriptor, codec or device.
#[derive(Debug, Clone)]
pub struct EncoderProbe {
    config: ProbeConfig,
    markers: [AudioMarker; 3],
    raster: [usize; 2],
    terminal_ordinal: usize,
}

impl EncoderProbe {
    pub fn new(raster: [u32; 2], frame_rate: [u32; 2]) -> Result<Self, EncodeError> {
        // Reuse native geometry/rate admission and its GOP calculation. No
        // encoder is opened and the requested mode does not affect this clock.
        let one = EncodeContract::new(
            raster,
            frame_rate,
            1,
            sample_boundary(1, frame_rate)?,
            EncoderMode::Hardware,
            BFramePolicy::None,
        )?;
        let video_frames = u64::from(one.policy().gop_frames)
            .checked_mul(3)
            .and_then(|frames| frames.checked_add(1))
            .filter(|frames| *frames <= MAX_PROBE_FRAMES)
            .ok_or(EncodeError::Configuration(
                "probe frame count exceeds bound",
            ))?;
        if u128::from(video_frames) * u128::from(frame_rate[1])
            > u128::from(MAX_PROBE_SECONDS) * u128::from(frame_rate[0])
        {
            return Err(EncodeError::Configuration(
                "probe requires more than eight seconds at this frame rate",
            ));
        }
        let audio_samples = sample_boundary(video_frames, frame_rate)?;
        let contract = EncodeContract::new(
            raster,
            frame_rate,
            video_frames,
            audio_samples,
            EncoderMode::Hardware,
            BFramePolicy::None,
        )?;
        let stored_raster = [
            usize::try_from(raster[0])
                .map_err(|_| EncodeError::Configuration("probe raster exceeds address space"))?,
            usize::try_from(raster[1])
                .map_err(|_| EncodeError::Configuration("probe raster exceeds address space"))?,
        ];
        let terminal_ordinal = usize::try_from(video_frames - 1)
            .map_err(|_| EncodeError::Configuration("probe ordinal exceeds address space"))?;
        // Admitted probes are at least one second long, keeping the three
        // marker windows disjoint even with the 4096-sample diagnostic radius.
        let markers = [
            AudioMarker {
                samples: [100, 137],
                amplitudes: [0.75, -0.625],
            },
            AudioMarker {
                samples: [audio_samples / 2, audio_samples / 2 + 37],
                amplitudes: [0.625, -0.75],
            },
            AudioMarker {
                samples: [audio_samples - 200, audio_samples - 163],
                amplitudes: [0.6875, -0.5625],
            },
        ];
        Ok(Self {
            config: ProbeConfig {
                version: PROBE_VERSION,
                raster,
                frame_rate,
                video_frames,
                audio_samples,
                gop_frames: contract.policy().gop_frames,
                picture_bytes: contract.picture_bytes(),
            },
            markers,
            raster: stored_raster,
            terminal_ordinal,
        })
    }

    pub fn config(&self) -> &ProbeConfig {
        &self.config
    }

    pub fn markers(&self) -> &[AudioMarker; 3] {
        &self.markers
    }

    pub fn contract(
        &self,
        mode: EncoderMode,
        b_frames: BFramePolicy,
    ) -> Result<EncodeContract, EncodeError> {
        EncodeContract::new(
            self.config.raster,
            self.config.frame_rate,
            self.config.video_frames,
            self.config.audio_samples,
            mode,
            b_frames,
        )
    }

    /// Fill one tight limited-range Rec.709 I420 frame. Every shape occupies
    /// whole 2x2 luma cells, so its Cb/Cr ownership is unambiguous. Ordinal bars
    /// change each picture and the colored block traverses the middle band.
    pub fn fill_picture(&self, ordinal: u64, output: &mut [u8]) -> Result<(), EncodeError> {
        let ordinal = self.check_ordinal(ordinal)?;
        if u64::try_from(output.len()).ok() != Some(self.config.picture_bytes) {
            return Err(EncodeError::Input(
                "probe picture buffer has the wrong length",
            ));
        }
        let [width, height] = self.raster;
        let pixels = width * height;
        let (y_plane, chroma) = output.split_at_mut(pixels);
        let (u_plane, v_plane) = chroma.split_at_mut(pixels / 4);
        for row in 0..height / 2 {
            for column in 0..width / 2 {
                let [y, u, v] = self.cell(ordinal, column, row);
                let position = row * 2 * width + column * 2;
                y_plane[position..position + 2].fill(y);
                y_plane[position + width..position + width + 2].fill(y);
                let position = row * (width / 2) + column;
                u_plane[position] = u;
                v_plane[position] = v;
            }
        }
        Ok(())
    }

    /// Return the exact Y/Cb/Cr values of the cell containing a luma position.
    /// This is an input reference, not an assertion about lossy decoded values.
    pub fn pixel(&self, ordinal: u64, position: [u32; 2]) -> Result<[u8; 3], EncodeError> {
        let ordinal = self.check_ordinal(ordinal)?;
        if position[0] >= self.config.raster[0] || position[1] >= self.config.raster[1] {
            return Err(EncodeError::Input("probe pixel is outside its raster"));
        }
        let column = usize::try_from(position[0] / 2)
            .map_err(|_| EncodeError::Input("probe pixel exceeds address space"))?;
        let row = usize::try_from(position[1] / 2)
            .map_err(|_| EncodeError::Input("probe pixel exceeds address space"))?;
        Ok(self.cell(ordinal, column, row))
    }

    /// Fill one consecutive native AAC input block on the probe's absolute
    /// sample clock. All non-marker samples are digital zero. Arbitrary valid
    /// block boundaries produce the same values as one linear traversal.
    pub fn fill_audio(
        &self,
        first_sample: u64,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), EncodeError> {
        let count = u64::try_from(left.len())
            .map_err(|_| EncodeError::Input("probe audio buffer exceeds address space"))?;
        if left.len() != right.len()
            || count == 0
            || count > u64::from(AUDIO_FRAME_SAMPLES)
            || first_sample
                .checked_add(count)
                .is_none_or(|end| end > self.config.audio_samples)
        {
            return Err(EncodeError::Input(
                "probe audio block exceeds its exact interval",
            ));
        }
        left.fill(0.0);
        right.fill(0.0);
        for marker in &self.markers {
            for (channel, output) in [&mut *left, &mut *right].into_iter().enumerate() {
                if let Some(offset) = marker.samples[channel].checked_sub(first_sample)
                    && offset < count
                {
                    output[usize::try_from(offset).expect("bounded AAC input block")] =
                        marker.amplitudes[channel];
                }
            }
        }
        Ok(())
    }

    fn check_ordinal(&self, ordinal: u64) -> Result<usize, EncodeError> {
        if ordinal >= self.config.video_frames {
            Err(EncodeError::Input(
                "probe picture ordinal exceeds its interval",
            ))
        } else {
            usize::try_from(ordinal)
                .map_err(|_| EncodeError::Input("probe ordinal exceeds address space"))
        }
    }

    fn cell(&self, ordinal: usize, column: usize, row: usize) -> [u8; 3] {
        let width = self.raster[0] / 2;
        let height = self.raster[1] / 2;
        if row < (height / 8).max(1) {
            // Seven low-to-high bits suffice for every bounded probe ordinal.
            let bit = column * 7 / width;
            return [if ordinal & (1 << bit) == 0 { 32 } else { 224 }, 128, 128];
        }
        if row >= height * 3 / 4 {
            return COLORS[column * COLORS.len() / width];
        }
        let box_width = (width / 8).max(1);
        let box_height = (height / 6).max(1);
        let left = ordinal * (width - box_width) / self.terminal_ordinal;
        let top = height / 3;
        if (left..left + box_width).contains(&column) && (top..top + box_height).contains(&row) {
            return MOVING_COLOR;
        }
        [
            40 + u8::try_from(column % 16).expect("four-bit gradient"),
            128,
            128,
        ]
    }
}

fn sample_boundary(frame: u64, rate: [u32; 2]) -> Result<u64, EncodeError> {
    if rate[0] == 0 || rate[1] == 0 {
        return Err(EncodeError::Configuration(
            "probe frame rate must be positive",
        ));
    }
    let numerator = u128::from(frame) * u128::from(AUDIO_SAMPLE_RATE) * u128::from(rate[1]);
    let denominator = u128::from(rate[0]);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = quotient
        + u128::from(
            remainder * 2 > denominator
                || remainder * 2 == denominator && !quotient.is_multiple_of(2),
        );
    u64::try_from(rounded)
        .map_err(|_| EncodeError::Configuration("probe sample boundary exceeds range"))
}

pub const HDR_PROBE_VERSION: u32 = 1;

/// BT.2020 primaries, D65 white, 1000 cd/m² peak and 0.005 cd/m² black, in
/// the contract units. Declared probe metadata, not a measured display.
pub const HDR_PROBE_MASTERING: MasteringDisplay = MasteringDisplay {
    primaries: [[35_400, 14_600], [8_500, 39_850], [6_550, 2_300]],
    white_point: [15_635, 16_450],
    max_luminance: 10_000_000,
    min_luminance: 50,
};

/// Declared PQ probe content light for the `clli` round trip. These are fixed
/// probe metadata values, not a measurement of the synthetic pictures.
pub const HDR_PROBE_CONTENT_LIGHT: ContentLight = ContentLight {
    max_cll: 1_000,
    max_fall: 203,
};

/// Known 10-bit limited-range codes for one transfer. Neutral codes are
/// achromatic (Cb=Cr=512); colored codes are BT.2020 NCL R'G'B' primaries and
/// secondaries at the reference-white signal level, rounded half up once.
struct HdrCodes {
    black: u16,
    near_black: u16,
    ten_nits: u16,
    reference_white: u16,
    thousand_nits: u16,
    colors: [[u16; 3]; 3],
    moving: [u16; 3],
}

/// PQ: 0.1, 10, 203 and 1000 cd/m² inverse-EOTF codes. Colors use E'=PQ(203).
const PQ_CODES: HdrCodes = HdrCodes {
    black: 64,
    near_black: 119,
    ten_nits: 327,
    reference_white: 573,
    thousand_nits: 723,
    colors: [[198, 439, 772], [409, 325, 273], [94, 772, 491]],
    moving: [543, 252, 533],
};

/// HLG (Lw 1000, gamma 1.2) display-referred 0.1, 10, 203 and 1000 cd/m²
/// achromatic codes. Colors use E'=0.75 (75% HLG reference white).
const HLG_CODES: HdrCodes = HdrCodes {
    black: 64,
    near_black: 97,
    ten_nits: 287,
    reference_white: 721,
    thousand_nits: 940,
    colors: [[237, 418, 848], [509, 270, 203], [103, 848, 485]],
    moving: [682, 176, 539],
};

const NEUTRAL: u16 = 512;

/// Deterministic HDR probe analogous to `EncoderProbe`: same clocks, GOP-based
/// length and audio markers, with planar 10-bit little-endian pictures. Like
/// the SDR probe it grants no authority and opens nothing.
#[derive(Debug, Clone)]
pub struct HdrEncoderProbe {
    base: EncoderProbe,
    config: ProbeConfig,
    transfer: HdrTransfer,
}

impl HdrEncoderProbe {
    pub fn new(
        raster: [u32; 2],
        frame_rate: [u32; 2],
        transfer: HdrTransfer,
    ) -> Result<Self, EncodeError> {
        let base = EncoderProbe::new(raster, frame_rate)?;
        let mut config = base.config().clone();
        config.version = HDR_PROBE_VERSION;
        config.picture_bytes *= 2;
        let probe = Self {
            base,
            config,
            transfer,
        };
        // Admit the exact HDR contract once, independent of mode/B frames.
        let contract = probe.contract(EncoderMode::Hardware, BFramePolicy::None)?;
        debug_assert_eq!(contract.picture_bytes(), probe.config.picture_bytes);
        Ok(probe)
    }

    pub fn config(&self) -> &ProbeConfig {
        &self.config
    }
    pub const fn transfer(&self) -> HdrTransfer {
        self.transfer
    }
    pub fn markers(&self) -> &[AudioMarker; 3] {
        self.base.markers()
    }

    /// The probe's HDR signal: PQ carries `HDR_PROBE_MASTERING`; HLG none.
    pub const fn signal(&self) -> HdrSignal {
        HdrSignal {
            transfer: self.transfer,
            mastering: match self.transfer {
                HdrTransfer::Pq => Some(HDR_PROBE_MASTERING),
                HdrTransfer::Hlg => None,
            },
        }
    }

    /// Content light to pass at finish: `Some` for PQ, `None` for HLG.
    pub const fn content_light(&self) -> Option<ContentLight> {
        match self.transfer {
            HdrTransfer::Pq => Some(HDR_PROBE_CONTENT_LIGHT),
            HdrTransfer::Hlg => None,
        }
    }

    pub fn contract(
        &self,
        mode: EncoderMode,
        b_frames: BFramePolicy,
    ) -> Result<EncodeContract, EncodeError> {
        EncodeContract::new_hdr_v1(
            self.config.raster,
            self.config.frame_rate,
            self.config.video_frames,
            self.config.audio_samples,
            mode,
            b_frames,
            self.signal(),
        )
    }

    /// Fill one planar 10-bit little-endian Y/Cb/Cr frame. Shapes occupy whole
    /// 2x2 luma cells exactly as in the SDR probe.
    pub fn fill_picture(&self, ordinal: u64, output: &mut [u8]) -> Result<(), EncodeError> {
        let ordinal = self.base.check_ordinal(ordinal)?;
        if u64::try_from(output.len()).ok() != Some(self.config.picture_bytes) {
            return Err(EncodeError::Input(
                "probe picture buffer has the wrong length",
            ));
        }
        let [width, height] = self.base.raster;
        let pixels = width * height;
        let (y_plane, chroma) = output.split_at_mut(pixels * 2);
        let (u_plane, v_plane) = chroma.split_at_mut(pixels / 2);
        for row in 0..height / 2 {
            for column in 0..width / 2 {
                let [y, u, v] = self.cell(ordinal, column, row).map(u16::to_le_bytes);
                for line in [row * 2, row * 2 + 1] {
                    let position = (line * width + column * 2) * 2;
                    y_plane[position..position + 2].copy_from_slice(&y);
                    y_plane[position + 2..position + 4].copy_from_slice(&y);
                }
                let position = (row * (width / 2) + column) * 2;
                u_plane[position..position + 2].copy_from_slice(&u);
                v_plane[position..position + 2].copy_from_slice(&v);
            }
        }
        Ok(())
    }

    /// Exact 10-bit Y/Cb/Cr input codes of the cell containing a luma position.
    pub fn pixel(&self, ordinal: u64, position: [u32; 2]) -> Result<[u16; 3], EncodeError> {
        let ordinal = self.base.check_ordinal(ordinal)?;
        if position[0] >= self.config.raster[0] || position[1] >= self.config.raster[1] {
            return Err(EncodeError::Input("probe pixel is outside its raster"));
        }
        let column = usize::try_from(position[0] / 2)
            .map_err(|_| EncodeError::Input("probe pixel exceeds address space"))?;
        let row = usize::try_from(position[1] / 2)
            .map_err(|_| EncodeError::Input("probe pixel exceeds address space"))?;
        Ok(self.cell(ordinal, column, row))
    }

    pub fn fill_audio(
        &self,
        first_sample: u64,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), EncodeError> {
        self.base.fill_audio(first_sample, left, right)
    }

    fn codes(&self) -> &'static HdrCodes {
        match self.transfer {
            HdrTransfer::Pq => &PQ_CODES,
            HdrTransfer::Hlg => &HLG_CODES,
        }
    }

    fn cell(&self, ordinal: usize, column: usize, row: usize) -> [u16; 3] {
        let codes = self.codes();
        let width = self.base.raster[0] / 2;
        let height = self.base.raster[1] / 2;
        let neutral = |code: u16| [code, NEUTRAL, NEUTRAL];
        if row < (height / 8).max(1) {
            let bit = column * 7 / width;
            return neutral(if ordinal & (1 << bit) == 0 {
                codes.black
            } else {
                codes.reference_white
            });
        }
        if row >= height * 3 / 4 {
            let patches = [
                neutral(codes.black),
                neutral(codes.near_black),
                neutral(codes.ten_nits),
                neutral(codes.reference_white),
                neutral(codes.thousand_nits),
                codes.colors[0],
                codes.colors[1],
                codes.colors[2],
            ];
            return patches[column * patches.len() / width];
        }
        let box_width = (width / 8).max(1);
        let box_height = (height / 6).max(1);
        let left = ordinal * (width - box_width) / self.base.terminal_ordinal;
        let top = height / 3;
        if (left..left + box_width).contains(&column) && (top..top + box_height).contains(&row) {
            return codes.moving;
        }
        // Luma ramp from black to the 1000 cd/m² (PQ) or peak (HLG) code.
        let span = usize::from(codes.thousand_nits - codes.black);
        let step = column * span / (width - 1).max(1);
        neutral(codes.black + u16::try_from(step).expect("ramp is bounded by its span"))
    }
}
