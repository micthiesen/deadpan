//! Sanity lower bounds on CTA-861.3 content light from decoded PQ pictures.
//!
//! This is not CTA-861.3 verification: a 4:2:0 file cannot reproduce the
//! host's per-pixel statistics, so the meter only derives values that a
//! truthful declaration should not fall below (up to quantization and coding
//! error). Overstated declarations, and peaks carried by fewer than 1 % of
//! chroma sites, are not detected.
//!
//! The encoder host measures MaxCLL/MaxFALL per pixel from clipped linear
//! light before chroma subsampling and lossy coding. A per-pixel maximum of a
//! reconstructed 4:2:0 picture is not comparable with it: at saturated chroma
//! edges every reconstruction (nearest or bilinear) combines one pixel's luma
//! with its neighbours' chroma and reaches out-of-gamut R'G'B', measured at up
//! to 10000 cd/m² for 1000 cd/m² content. This meter therefore evaluates each
//! chroma site instead. The decoded luma is filtered with exactly the encoder
//! boundary's left-sited chroma filter ([1,2,1]/4 on columns x-1, x, x+1 with
//! edge replication, [1,1]/2 on rows y, y+1) and combined with that site's
//! decoded Cb/Cr. The BT.2020 NCL matrix is linear, so before quantization and
//! coding this site R'G'B' is a convex combination of the host's clipped pixel
//! R'G'B' (the replicated edge taps still sum to one).
//!
//! - MaxCLL: the brightest component of a convex combination (clamped to
//!   [0, 1], then the monotone PQ EOTF) cannot exceed the host's per-pixel
//!   maximum. Lossy coding still overshoots at dense saturated edges, so the
//!   bound is each frame's 99th percentile of site light (from an 8192-bin
//!   histogram of the nonlinear signal, taking the bin's lower edge), which
//!   ignores sparse coding outliers. Measured alternatives (per-pixel and
//!   site maxima) are recorded in docs/FINISHED_FILE_VERIFICATION.md.
//! - MaxFALL: the EOTF is convex, so a site's light is at most the
//!   filter-weighted sum of its pixels' light. The site mean equals the frame
//!   mean only if every pixel carries the same total weight. Interior pixels
//!   carry 1/4 (relative to the site count), but edge replication gives
//!   column 0 weight 3/8 and the last column 1/8, so an unweighted site mean
//!   can exceed the host mean when the left edge is brighter. Sites in chroma
//!   column 0 are therefore weighted 2/3, which brings column 0 to 1/4 and
//!   column 1 to 5/24. The weighted mean cannot exceed the host frame mean; it
//!   gives up at most a third of one site column (under 0.04 % of the mean at
//!   1920 pixels wide).

use super::{Result, require};

const LUMA_RED: f64 = 0.2627;
const LUMA_BLUE: f64 = 0.0593;
const LUMA_GREEN: f64 = 1.0 - LUMA_RED - LUMA_BLUE;
const LUT_SEGMENTS: usize = 1 << 16;

/// Decoded chroma-site content light lower bounds in cd/m²: `max_cll` is
/// the largest per-frame `SITE_QUANTILE` site value, `max_fall` the largest
/// per-frame edge-weighted site mean. `max_fall` may exceed `max_cll` when
/// fewer than 1 % of sites carry most of a frame's light.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct DecodedLight {
    pub max_cll: f64,
    pub max_fall: f64,
}

/// Brightest nonlinear BT.2020 NCL R'G'B' component of normalized Y'CbCr.
pub(super) fn brightest(luma: f64, cb: f64, cr: f64) -> f64 {
    let red = luma + 2.0 * (1.0 - LUMA_RED) * cr;
    let blue = luma + 2.0 * (1.0 - LUMA_BLUE) * cb;
    let green = (luma - LUMA_RED * red - LUMA_BLUE * blue) / LUMA_GREEN;
    red.max(green).max(blue)
}

pub(super) fn luma(code: u16) -> f64 {
    (f64::from(code) - 64.0) / 876.0
}

pub(super) fn chroma(code: u16) -> f64 {
    (f64::from(code) - 512.0) / 896.0
}

/// Histogram resolution over the nonlinear PQ signal [0, 1]: 1/8192 is about
/// 0.11 limited-range 10-bit code values.
const BINS: usize = 8192;

/// Fraction of chroma sites per frame at or below the MaxCLL bound. Sparse
/// lossy-coding overshoot at saturated edges stays above it; see
/// docs/FINISHED_FILE_VERIFICATION.md for the measurement.
pub(super) const SITE_QUANTILE: f64 = 0.99;

pub(super) struct LightMeter {
    eotf: Vec<f64>,
    light: DecodedLight,
    columns: usize,
    quantiles: Vec<f64>,
    /// Per quantile, the largest per-frame lower quantile bound in cd/m².
    quantile_light: Vec<f64>,
    histogram: Vec<u64>,
}

impl LightMeter {
    pub(super) fn new(width: u32) -> Result<Self> {
        Self::with_quantiles(width, &[SITE_QUANTILE])
    }

    /// Track the per-frame `quantiles` (fractions of sites at or below the
    /// value, in (0, 1]) of site light as well as the maximum and mean.
    pub(super) fn with_quantiles(width: u32, quantiles: &[f64]) -> Result<Self> {
        let mut histogram = Vec::new();
        histogram
            .try_reserve_exact(BINS)
            .map_err(|e| e.to_string())?;
        histogram.resize(BINS, 0);
        let mut eotf = Vec::new();
        eotf.try_reserve_exact(LUT_SEGMENTS + 1)
            .map_err(|e| e.to_string())?;
        eotf.extend(
            (0..=LUT_SEGMENTS)
                .map(|index| deadpan_render::pq_eotf(index as f64 / LUT_SEGMENTS as f64)),
        );
        Ok(Self {
            eotf,
            light: DecodedLight::default(),
            columns: usize::try_from(width / 2).map_err(|_| "chroma width overflow")?,
            quantiles: quantiles.to_vec(),
            quantile_light: vec![0.0; quantiles.len()],
            histogram,
        })
    }

    /// Largest per-frame lower bound of each configured site-light quantile.
    #[cfg(test)]
    pub(super) fn quantile_light(&self) -> &[f64] {
        &self.quantile_light
    }

    pub(super) fn light(&self) -> DecodedLight {
        self.light
    }

    /// PQ EOTF in cd/m² of a nonlinear signal clamped to [0, 1].
    pub(super) fn nits(&self, signal: f64) -> f64 {
        let position = signal.clamp(0.0, 1.0) * LUT_SEGMENTS as f64;
        // Truncation selects the lower interpolation knot of a finite value in range.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = (position as usize).min(LUT_SEGMENTS - 1);
        let fraction = position - index as f64;
        self.eotf[index] + (self.eotf[index + 1] - self.eotf[index]) * fraction
    }

    /// Accumulate one tight Y, Cb, Cr picture of `width`x`height` samples.
    pub(super) fn add(&mut self, width: u32, height: u32, samples: &[u16]) -> Result<()> {
        let width = usize::try_from(width).map_err(|_| "picture width overflow")?;
        let height = usize::try_from(height).map_err(|_| "picture height overflow")?;
        let columns = width / 2;
        let rows = height / 2;
        let pixels = width * height;
        require(
            width % 2 == 0
                && height % 2 == 0
                && width > 0
                && height > 0
                && columns == self.columns
                && samples.len() == pixels + 2 * columns * rows,
            "decoded HDR picture layout differs from the light meter raster",
        )?;
        let (y_plane, chroma_planes) = samples.split_at(pixels);
        let (cb_plane, cr_plane) = chroma_planes.split_at(columns * rows);
        let mut total = 0.0_f64;
        let mut maximum = 0.0_f64;
        for row in 0..rows {
            let lines = [row * 2 * width, (row * 2 + 1) * width];
            for column in 0..columns {
                let x = column * 2;
                let taps = [
                    (x.saturating_sub(1), 0.25),
                    (x, 0.5),
                    ((x + 1).min(width - 1), 0.25),
                ];
                let filtered: f64 = lines
                    .iter()
                    .flat_map(|line| taps.iter().map(move |(tap, weight)| (line + tap, weight)))
                    .map(|(index, weight)| weight * 0.5 * luma(y_plane[index]))
                    .sum();
                let site = row * columns + column;
                let signal = brightest(filtered, chroma(cb_plane[site]), chroma(cr_plane[site]));
                let nits = self.nits(signal);
                maximum = maximum.max(nits);
                // Column 0 sites see pixel column 0 three times (replicated
                // left tap); 2/3 restores its per-pixel weight to 1/4.
                total += if column == 0 { nits * 2.0 / 3.0 } else { nits };
                // Truncation bins a finite signal clamped to [0, 1].
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let bin = ((signal.clamp(0.0, 1.0) * BINS as f64) as usize).min(BINS - 1);
                self.histogram[bin] += 1;
            }
        }
        let sites = (columns * rows) as f64;
        for index in 0..self.quantiles.len() {
            let quantile = self.quantiles[index];
            // Smallest bin whose cumulative count reaches the quantile; its
            // lower edge never exceeds the true quantile value.
            let target = (quantile * sites).ceil().max(1.0);
            let mut cumulative = 0.0;
            let mut bin = BINS - 1;
            for (candidate, count) in self.histogram.iter().enumerate() {
                cumulative += *count as f64;
                if cumulative >= target {
                    bin = candidate;
                    break;
                }
            }
            let value = self.nits(bin as f64 / BINS as f64);
            self.quantile_light[index] = self.quantile_light[index].max(value);
        }
        self.histogram.fill(0);
        self.light.max_cll = self.quantile_light.first().copied().unwrap_or(maximum);
        self.light.max_fall = self.light.max_fall.max(total / (columns * rows) as f64);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_matches_the_exact_pq_eotf() {
        let meter = LightMeter::new(2).unwrap();
        let mut worst = 0.0_f64;
        for step in 0..=100_000 {
            let signal = f64::from(step) / 100_000.0;
            let exact = deadpan_render::pq_eotf(signal);
            let error = (meter.nits(signal) - exact).abs() / exact.max(1e-3);
            worst = worst.max(error);
        }
        // Interpolation error is a few parts per million, far below coding error.
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn sites_filter_luma_like_the_encoder_chroma_filter() {
        // 4x2 neutral picture: columns 573, 723, 573, 573 on both rows.
        // Site 0 taps columns 0, 0, 1 -> (0.75 * 573 + 0.25 * 723) codes;
        // site 1 taps columns 1, 2, 3 -> (0.25 * 723 + 0.75 * 573).
        let mut meter = LightMeter::new(4).unwrap();
        let y = [573, 723, 573, 573];
        let mut samples = Vec::from(y);
        samples.extend(y);
        samples.extend([512; 4]);
        meter.add(4, 2, &samples).unwrap();
        let signal = luma(573) * 0.75 + luma(723) * 0.25;
        let expected = deadpan_render::pq_eotf(signal);
        let light = meter.light();
        // The MaxCLL percentile is the lower edge of its 1/8192 histogram bin.
        assert!(light.max_cll <= expected * (1.0 + 1e-6));
        assert!(light.max_cll >= deadpan_render::pq_eotf(signal - 1.0 / 8192.0));
        // Site 0 (chroma column 0) has weight 2/3 in the mean.
        let mean = expected * (2.0 / 3.0 + 1.0) / 2.0;
        assert!((light.max_fall - mean).abs() < 1e-6 * mean);
        // The site bound never exceeds the per-pixel maximum (1000 cd/m²).
        assert!(light.max_cll < deadpan_render::pq_code_to_nits(723));
        meter.add(4, 2, &[64; 11]).unwrap_err();
        meter.add(2, 2, &[64; 6]).unwrap_err();
    }

    #[test]
    fn saturated_edges_do_not_inflate_the_site_bound() {
        // Host pictures from the renderer boundary: alternating 1000 cd/m²
        // red and blue columns. Per-pixel reconstruction overshoots; sites
        // stay at or below the host per-pixel maximum.
        let (width, height) = (8_u32, 2_u32);
        let mut bytes = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                let value = 1_000.0 / 203.0;
                let rgb = if x % 2 == 0 {
                    [value, 0.0, 0.0]
                } else {
                    [0.0, 0.0, value]
                };
                for channel in [rgb[0], rgb[1], rgb[2], 1.0_f32] {
                    bytes.extend_from_slice(&f16(channel));
                }
            }
        }
        let working =
            deadpan_render::WorkingRgba16Frame::new(width, height, width * 8, bytes).unwrap();
        let (frame, host) = deadpan_render::Rec2100Yuv420P10Frame::from_working(
            &working,
            deadpan_render::HdrTransfer::Pq,
        )
        .unwrap();
        let samples: Vec<u16> = frame
            .bytes()
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let mut meter = LightMeter::new(width).unwrap();
        meter.add(width, height, &samples).unwrap();
        let light = meter.light();
        assert!(light.max_cll <= host.max_nits * 1.01, "{light:?} {host:?}");
        assert!(
            light.max_fall <= host.mean_nits * 1.01,
            "{light:?} {host:?}"
        );
    }

    /// Renderer-boundary picture and its host CTA-861.3 statistics.
    fn host_picture(
        width: u32,
        height: u32,
        pixel: impl Fn(u32) -> [f32; 3],
    ) -> (Vec<u16>, deadpan_render::FrameLight) {
        let mut bytes = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                let rgb = pixel(x);
                for channel in [rgb[0], rgb[1], rgb[2], 1.0_f32] {
                    bytes.extend_from_slice(&f16(channel));
                }
            }
        }
        let working =
            deadpan_render::WorkingRgba16Frame::new(width, height, width * 8, bytes).unwrap();
        let (frame, host) = deadpan_render::Rec2100Yuv420P10Frame::from_working(
            &working,
            deadpan_render::HdrTransfer::Pq,
        )
        .unwrap();
        let samples = frame
            .bytes()
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        (samples, host)
    }

    #[test]
    fn a_brighter_left_edge_does_not_raise_the_mean_above_the_host() {
        // Gray 300 cd/m² in column 0, 100 cd/m² elsewhere. The replicated
        // left tap gives pixel column 0 weight 3/8 instead of 1/4, so an
        // unweighted site mean exceeds the host frame mean by about 10 %.
        let (width, height) = (4_u32, 2_u32);
        let (samples, host) = host_picture(width, height, |x| {
            [if x == 0 { 300.0 } else { 100.0 } / 203.0; 3]
        });
        let mut meter = LightMeter::new(width).unwrap();
        meter.add(width, height, &samples).unwrap();
        let sites: Vec<f64> = [[0, 0, 1], [1, 2, 3]]
            .iter()
            .map(|taps| {
                let filtered = (luma(samples[taps[0]])
                    + 2.0 * luma(samples[taps[1]])
                    + luma(samples[taps[2]]))
                    / 4.0;
                deadpan_render::pq_eotf(filtered)
            })
            .collect();
        let unweighted = (sites[0] + sites[1]) / 2.0;
        assert!(unweighted > host.mean_nits * 1.05, "{unweighted} {host:?}");
        let light = meter.light();
        assert!(
            light.max_fall <= host.mean_nits * 1.001,
            "{light:?} {host:?}"
        );
        assert!(light.max_cll <= host.max_nits * 1.001, "{light:?} {host:?}");
    }

    #[test]
    fn sparse_highlights_can_put_the_mean_bound_above_the_percentile_bound() {
        // 400 chroma sites, two lit by a 2x2 white 10000 cd/m² highlight: the
        // 99th percentile stays black while the mean does not. Evidence with
        // decoded MaxFALL above decoded MaxCLL and a consistent declaration
        // is admitted; only the declared pair must satisfy FALL <= CLL.
        let (width, height) = (800_u32, 2_u32);
        let (samples, host) = host_picture(width, height, |x| {
            [if (100..102).contains(&x) {
                10_000.0 / 203.0
            } else {
                0.0
            }; 3]
        });
        let mut meter = LightMeter::new(width).unwrap();
        meter.add(width, height, &samples).unwrap();
        let decoded = meter.light();
        assert!(decoded.max_fall > decoded.max_cll + 1.0, "{decoded:?}");
        assert!(
            decoded.max_fall <= host.mean_nits * 1.001,
            "{decoded:?} {host:?}"
        );
        let declared =
            |max_cll: u16, max_fall: u16| deadpan_source::ContentLight { max_cll, max_fall };
        let whole = |nits: f64| u16::try_from(nits.ceil() as u32).unwrap();
        let evidence = crate::encoded_render::verification::ContentLightEvidence::new(
            declared(whole(host.max_nits), whole(host.mean_nits)),
            decoded,
        )
        .unwrap();
        assert!(
            evidence.decoded_bound_max_fall_millinits > evidence.decoded_bound_max_cll_millinits
        );
        evidence.validate().unwrap();
        let mut inverted = evidence;
        inverted.declared_max_fall = inverted.declared_max_cll + 1;
        assert!(inverted.validate().is_err());
    }

    /// Binary16 for the small finite nonnegative working values above.
    fn f16(value: f32) -> [u8; 2] {
        let bits = value.to_bits();
        let exponent = i32::try_from((bits >> 23) & 0xff).unwrap() - 127 + 15;
        let half = if value == 0.0 || exponent <= 0 {
            0_u16
        } else {
            let mantissa = bits & 0x7f_ffff;
            u16::try_from((u32::try_from(exponent).unwrap() << 10) + ((mantissa + 0x1000) >> 13))
                .unwrap()
        };
        half.to_le_bytes()
    }
}
