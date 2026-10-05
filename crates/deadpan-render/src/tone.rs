//! Output color branch and the highlight tone map shared by the SDR export
//! path (per HDR source, before compositing) and the HDR-output SDR preview
//! (on the composite). Working 1.0 is [`HDR_REFERENCE_WHITE_NITS`].

use crate::color::{HDR_REFERENCE_WHITE_NITS, PQ_PEAK_NITS};
use crate::{RenderError, SourceColor, source_to_working, working_to_display};

/// The two HDR output transfers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdrTransfer {
    Pq,
    Hlg,
}

/// The output branch of one renderer: SDR Rec.709 or HDR Rec.2100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputColor {
    #[default]
    Sdr,
    Hdr(HdrTransfer),
}

/// Reference-white-preserving highlight tone map (BT.2408 style) from the
/// declared source peak to the SDR range, working [0, 1].
///
/// Working light up to [`ToneMap::KNEE`] is unchanged. Above it, the excess
/// `y = (x - k) / (1 - k)` follows the extended-Reinhard shoulder
/// `g(y) = y (1 + y / Y^2) / (1 + y)`, where `Y = (P - k) / (1 - k)` and `P` is
/// the source peak in working units, so `g(Y) = 1` and the output is
/// `k + (1 - k) g(y)`. The curve meets the identity with slope 1 at the knee,
/// is strictly increasing with slope never above 1, and reaches working 1.0
/// exactly at the source peak; inputs above the peak clip to 1.0. HDR reference
/// white (working 1.0, 203 cd/m^2) lands at `(1 + k) / 2 + (1 - k) / (2 Y^2)`,
/// at least 0.95 for every admitted peak above 203 cd/m^2. At a 203 cd/m^2
/// peak `Y = 1` and the curve is the identity with a clip at 1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneMap {
    source_peak_nits: f64,
}

impl ToneMap {
    pub const MIN_SOURCE_PEAK_NITS: f64 = HDR_REFERENCE_WHITE_NITS;
    pub const MAX_SOURCE_PEAK_NITS: f64 = PQ_PEAK_NITS;
    pub const DEFAULT_SOURCE_PEAK_NITS: f64 = 1000.0;
    /// Working light at and below this value (182.7 cd/m^2) is unchanged.
    pub const KNEE: f64 = 0.9;

    /// Source peak luminance in cd/m^2, validated to 203..=10000.
    pub fn new(source_peak_nits: f64) -> Result<Self, RenderError> {
        if !(Self::MIN_SOURCE_PEAK_NITS..=Self::MAX_SOURCE_PEAK_NITS).contains(&source_peak_nits) {
            return Err(RenderError::ToneMapPeak);
        }
        Ok(Self { source_peak_nits })
    }

    pub const fn source_peak_nits(self) -> f64 {
        self.source_peak_nits
    }

    fn curve(self) -> Curve {
        let headroom = 1.0 - Self::KNEE;
        let peak = (self.source_peak_nits / HDR_REFERENCE_WHITE_NITS - Self::KNEE) / headroom;
        Curve {
            knee: Self::KNEE,
            headroom,
            peak,
            inverse_peak_squared: 1.0 / (peak * peak),
        }
    }

    /// Map one working-light value (1.0 = 203 cd/m^2) through the curve.
    /// Values at or above the source peak map to 1.0; values at or below the
    /// knee, including negative values, are unchanged.
    pub fn map_working(self, working: f64) -> f64 {
        let curve = self.curve();
        if working <= curve.knee {
            return working;
        }
        let y = ((working - curve.knee) / curve.headroom).min(curve.peak);
        curve.knee + curve.headroom * y * (1.0 + y * curve.inverse_peak_squared) / (1.0 + y)
    }

    /// [`ToneMap::map_working`] in cd/m^2.
    pub fn map_nits(self, nits: f64) -> f64 {
        self.map_working(nits / HDR_REFERENCE_WHITE_NITS) * HDR_REFERENCE_WHITE_NITS
    }

    /// Shader constants: knee, headroom (1 - knee), normalized peak Y, 1/Y^2.
    pub(crate) fn shader_parameters(self) -> [f32; 4] {
        let curve = self.curve();
        [
            curve.knee as f32,
            curve.headroom as f32,
            curve.peak as f32,
            curve.inverse_peak_squared as f32,
        ]
    }
}

impl Default for ToneMap {
    fn default() -> Self {
        Self {
            source_peak_nits: Self::DEFAULT_SOURCE_PEAK_NITS,
        }
    }
}

struct Curve {
    knee: f64,
    headroom: f64,
    peak: f64,
    inverse_peak_squared: f64,
}

/// The renderer's complete color branch. Default: SDR output, 1000 cd/m^2
/// source peak for tone mapping HDR sources.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorPipeline {
    pub output: OutputColor,
    pub tone_map: ToneMap,
}

impl ColorPipeline {
    pub const fn sdr(tone_map: ToneMap) -> Self {
        Self {
            output: OutputColor::Sdr,
            tone_map,
        }
    }

    pub const fn hdr(transfer: HdrTransfer, tone_map: ToneMap) -> Self {
        Self {
            output: OutputColor::Hdr(transfer),
            tone_map,
        }
    }

    /// Short viewing-condition label for the SDR preview of this branch.
    /// `hdr_source` says whether the shown picture comes from a PQ/HLG source.
    pub const fn preview_label(self, hdr_source: bool) -> &'static str {
        match (self.output, hdr_source) {
            (OutputColor::Hdr(HdrTransfer::Pq), _) => "HDR PQ output, tone-mapped SDR preview",
            (OutputColor::Hdr(HdrTransfer::Hlg), _) => "HDR HLG output, tone-mapped SDR preview",
            (OutputColor::Sdr, true) => "SDR output (HDR source tone-mapped)",
            (OutputColor::Sdr, false) => "SDR output",
        }
    }

    /// True when the interpret pass tone-maps frames of this source color.
    pub const fn tone_maps_source(self, color: SourceColor) -> bool {
        matches!(self.output, OutputColor::Sdr) && color.transfer.is_hdr()
    }
}

/// [`ToneMap`] on max(R, G, B) of linear working light, with hue-preserving
/// ratio scaling of signed RGB. A maximum at or below the knee (including a
/// nonpositive one) is returned unchanged. The result stays in working units,
/// so 1.0 is SDR reference white.
pub fn tone_map_highlights(rgb: [f64; 3], tone_map: ToneMap) -> [f64; 3] {
    let peak = rgb[0].max(rgb[1]).max(rgb[2]);
    if peak <= ToneMap::KNEE {
        return rgb;
    }
    let ratio = tone_map.map_working(peak) / peak;
    rgb.map(|value| value * ratio)
}

/// CPU reference source interpretation under a color branch: the SDR branch
/// tone-maps PQ/HLG sources per pixel before compositing.
pub fn source_to_working_with(
    pipeline: ColorPipeline,
    rgb: [f64; 3],
    color: SourceColor,
) -> [f64; 3] {
    let working = source_to_working(rgb, color);
    if pipeline.tone_maps_source(color) {
        tone_map_highlights(working, pipeline.tone_map)
    } else {
        working
    }
}

/// CPU reference SDR preview transform: the HDR branch tone-maps the
/// composite first; both then use [`working_to_display`].
pub fn working_to_display_with(pipeline: ColorPipeline, rgb: [f64; 3]) -> [f64; 3] {
    match pipeline.output {
        OutputColor::Sdr => working_to_display(rgb),
        OutputColor::Hdr(_) => working_to_display(tone_map_highlights(rgb, pipeline.tone_map)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Primaries, Transfer};

    #[test]
    fn peak_validation_and_default() {
        assert_eq!(ToneMap::default().source_peak_nits(), 1000.0);
        for peak in [202.9, 10_000.1, f64::NAN, f64::INFINITY, -1.0] {
            assert!(matches!(ToneMap::new(peak), Err(RenderError::ToneMapPeak)));
        }
        for peak in [203.0, 1000.0, 4000.0, 10_000.0] {
            assert_eq!(ToneMap::new(peak).unwrap().source_peak_nits(), peak);
        }
    }

    #[test]
    fn curve_is_identity_below_the_knee_monotone_continuous_and_bounded() {
        for peak in [203.0, 250.0, 400.0, 1000.0, 4000.0, 10_000.0] {
            let tone_map = ToneMap::new(peak).unwrap();
            let peak_working = peak / HDR_REFERENCE_WHITE_NITS;
            let span = peak_working * 1.2;
            let step_size = span / 20_000.0;
            let mut previous = tone_map.map_working(0.0);
            for step in 1..=20_000 {
                let x = span * f64::from(step) / 20_000.0;
                let mapped = tone_map.map_working(x);
                assert!(mapped >= previous, "monotone at {x} for {peak}");
                assert!(mapped <= 1.0 + 1e-12, "bounded at {x} for {peak}");
                // Continuity: the slope never exceeds identity, so no step jumps.
                assert!(mapped - previous <= step_size * (1.0 + 1e-9), "{x}");
                if x <= ToneMap::KNEE {
                    assert_eq!(mapped, x);
                }
                if x < peak_working {
                    assert!(mapped < 1.0 || peak == 203.0, "{x} for {peak}");
                }
                previous = mapped;
            }
            // C1 at the knee: the one-sided slope just above it is 1.
            let h = 1e-7;
            let slope = (tone_map.map_working(ToneMap::KNEE + h) - ToneMap::KNEE) / h;
            assert!((slope - 1.0).abs() < 1e-5, "{slope} for {peak}");
            assert!((tone_map.map_working(peak_working) - 1.0).abs() < 1e-12);
            assert_eq!(tone_map.map_working(peak_working * 3.0), 1.0);
            assert_eq!(tone_map.map_working(-0.25), -0.25);
        }
    }

    #[test]
    fn reference_white_lands_near_sdr_white() {
        // At the minimum peak the curve is the identity with a clip at 1.0.
        let minimum = ToneMap::new(203.0).unwrap();
        assert_eq!(minimum.map_working(1.0), 1.0);
        assert_eq!(minimum.map_working(0.97), 0.97);
        // Above it, HDR reference white (working 1.0) lands at
        // (1 + k) / 2 + (1 - k) / (2 Y^2): at least 0.95 and below 1.0.
        for (peak, expected) in [
            (250.0, 0.954_549),
            (400.0, 0.950_436),
            (1000.0, 0.950_031),
            (4000.0, 0.950_001),
            (10_000.0, 0.950_000),
        ] {
            let white = ToneMap::new(peak).unwrap().map_working(1.0);
            assert!((white - expected).abs() < 1e-6, "{peak}: {white}");
            assert!((0.95..1.0).contains(&white), "{peak}: {white}");
        }
        let tone_map = ToneMap::default();
        assert!((tone_map.map_nits(203.0) - 0.950_031 * 203.0).abs() < 1e-3);
        // Diffuse light below the knee (182.7 cd/m^2) is untouched.
        assert_eq!(tone_map.map_nits(100.0), 100.0);
        assert_eq!(tone_map.map_nits(182.0), 182.0);
        // Highlights compress into (0.95, 1.0]: 500 cd/m^2 and the peak.
        let highlight = tone_map.map_nits(500.0) / HDR_REFERENCE_WHITE_NITS;
        assert!((0.99..1.0).contains(&highlight), "{highlight}");
        assert!((tone_map.map_nits(1000.0) - 203.0).abs() < 1e-9);
    }

    #[test]
    fn ratio_scaling_preserves_hue_and_sdr_frames_are_untouched() {
        let tone_map = ToneMap::default();
        let rgb = [4.0, 2.0, -0.5];
        let mapped = tone_map_highlights(rgb, tone_map);
        assert!((mapped[0] - tone_map.map_working(4.0)).abs() < 1e-12);
        assert!((mapped[1] / mapped[0] - 0.5).abs() < 1e-12);
        assert!((mapped[2] / mapped[0] + 0.125).abs() < 1e-12);
        assert!(mapped[0] <= 1.0 + 1e-12);
        assert_eq!(
            tone_map_highlights([-1.0, 0.0, -2.0], tone_map),
            [-1.0, 0.0, -2.0]
        );
        // Below-knee colors, including signed ones, pass through exactly.
        assert_eq!(
            tone_map_highlights([0.9, 0.3, -0.1], tone_map),
            [0.9, 0.3, -0.1]
        );
        let sdr = SourceColor {
            transfer: Transfer::Srgb,
            primaries: Primaries::Rec709,
        };
        let pq = SourceColor {
            transfer: Transfer::Pq,
            primaries: Primaries::Rec2020,
        };
        for output in [OutputColor::Sdr, OutputColor::Hdr(HdrTransfer::Pq)] {
            let pipeline = ColorPipeline { output, tone_map };
            assert_eq!(
                source_to_working_with(pipeline, [0.7, 0.2, 1.0], sdr),
                source_to_working([0.7, 0.2, 1.0], sdr)
            );
        }
        let bright = source_to_working([0.9; 3], pq);
        assert!(bright[0] > 10.0);
        let sdr_branch = source_to_working_with(ColorPipeline::default(), [0.9; 3], pq);
        assert!(sdr_branch[0] <= 1.0 + 1e-12);
        let hdr_branch =
            source_to_working_with(ColorPipeline::hdr(HdrTransfer::Pq, tone_map), [0.9; 3], pq);
        assert_eq!(hdr_branch, bright);
        assert_eq!(
            working_to_display_with(ColorPipeline::default(), bright),
            working_to_display(bright)
        );
        assert_eq!(
            working_to_display_with(ColorPipeline::hdr(HdrTransfer::Hlg, tone_map), bright),
            working_to_display(tone_map_highlights(bright, tone_map))
        );
    }

    #[test]
    fn preview_labels_name_the_viewing_condition() {
        let tone_map = ToneMap::default();
        assert_eq!(ColorPipeline::default().preview_label(false), "SDR output");
        assert_eq!(
            ColorPipeline::default().preview_label(true),
            "SDR output (HDR source tone-mapped)"
        );
        assert_eq!(
            ColorPipeline::hdr(HdrTransfer::Pq, tone_map).preview_label(true),
            "HDR PQ output, tone-mapped SDR preview"
        );
        assert_eq!(
            ColorPipeline::hdr(HdrTransfer::Hlg, tone_map).preview_label(false),
            "HDR HLG output, tone-mapped SDR preview"
        );
    }
}
