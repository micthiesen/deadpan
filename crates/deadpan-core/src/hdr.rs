//! Static HDR output metadata carried through render contracts.
//!
//! These are exact integer values in the units of SMPTE ST 2086 and
//! CTA-861.3 as coded in HEVC SEI and the MP4 `mdcv`/`clli` boxes. They are
//! pure values: admitting them from a source and emitting them in a file are
//! host responsibilities. Content light is never copied from a source; it is
//! measured from the emitted pictures.

use serde::{Deserialize, Serialize};

/// Chromaticity units: 1/50000. Luminance units: 1/10000 cd/m².
pub const MASTERING_CHROMATICITY_DENOMINATOR: u32 = 50_000;
pub const MASTERING_LUMINANCE_DENOMINATOR: u32 = 10_000;

/// SMPTE ST 2086 mastering display colour volume. Primaries are in R, G, B
/// order (FFmpeg's order), not the G, B, R order of the coded SEI/box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasteringDisplay {
    pub primaries: [[u16; 2]; 3],
    pub white_point: [u16; 2],
    pub max_luminance: u32,
    pub min_luminance: u32,
}

/// The single static-metadata rule set shared by source admission, render
/// contracts and the encoder: every chromaticity is a positive CIE 1931 point
/// (x + y <= 1), the primaries form a nondegenerate counter-clockwise R, G, B
/// triangle strictly containing the white point, the peak lies in
/// 50..=10,000 cd/m², black is at most 50 cd/m² and below the peak.
pub const MASTERING_MIN_PEAK: u32 = 50 * MASTERING_LUMINANCE_DENOMINATOR;
pub const MASTERING_MAX_PEAK: u32 = 10_000 * MASTERING_LUMINANCE_DENOMINATOR;
pub const MASTERING_MAX_BLACK: u32 = 50 * MASTERING_LUMINANCE_DENOMINATOR;
/// CTA-861.3 content light bound in cd/m².
pub const CONTENT_LIGHT_MAX: u16 = 10_000;

/// Which mastering-display rule a volume fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasteringDisplayError {
    Chromaticity,
    Primaries,
    Luminance,
}

impl MasteringDisplay {
    /// Checks the shared rule set documented on [`MASTERING_MIN_PEAK`].
    pub fn check(&self) -> Result<(), MasteringDisplayError> {
        let limit = MASTERING_CHROMATICITY_DENOMINATOR;
        let points = [
            self.primaries[0],
            self.primaries[1],
            self.primaries[2],
            self.white_point,
        ];
        if points
            .iter()
            .any(|[x, y]| *x == 0 || *y == 0 || u32::from(*x) + u32::from(*y) > limit)
        {
            return Err(MasteringDisplayError::Chromaticity);
        }
        let cross = |a: [u16; 2], b: [u16; 2], c: [u16; 2]| {
            let [ax, ay] = a.map(i64::from);
            let [bx, by] = b.map(i64::from);
            let [cx, cy] = c.map(i64::from);
            (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        };
        let [red, green, blue] = self.primaries;
        let white = self.white_point;
        if cross(red, green, blue) <= 0
            || cross(red, green, white) <= 0
            || cross(green, blue, white) <= 0
            || cross(blue, red, white) <= 0
        {
            return Err(MasteringDisplayError::Primaries);
        }
        if !(MASTERING_MIN_PEAK..=MASTERING_MAX_PEAK).contains(&self.max_luminance)
            || self.min_luminance > MASTERING_MAX_BLACK
            || self.min_luminance >= self.max_luminance
        {
            return Err(MasteringDisplayError::Luminance);
        }
        Ok(())
    }

    /// True when [`Self::check`] passes.
    pub fn is_valid(&self) -> bool {
        self.check().is_ok()
    }

    /// Peak luminance in cd/m², rounded down.
    pub const fn peak_nits(&self) -> u32 {
        self.max_luminance / MASTERING_LUMINANCE_DENOMINATOR
    }
}

/// CTA-861.3 content light level, in cd/m².
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentLight {
    pub max_cll: u16,
    pub max_fall: u16,
}

impl ContentLight {
    /// MaxCLL at most 10,000 cd/m² and MaxFALL at most MaxCLL; zero means
    /// unknown and is valid.
    pub const fn is_valid(&self) -> bool {
        self.max_cll <= CONTENT_LIGHT_MAX && self.max_fall <= self.max_cll
    }
}

/// Shared boundary cases for the mastering rule, so crates that wrap this
/// validator can prove they apply exactly the same decisions.
#[doc(hidden)]
pub fn mastering_rule_cases() -> Vec<(MasteringDisplay, Result<(), MasteringDisplayError>)> {
    use MasteringDisplayError::{Chromaticity, Luminance, Primaries};
    let p3 = MasteringDisplay {
        primaries: [[34_000, 16_000], [13_250, 34_500], [7_500, 3_000]],
        white_point: [15_635, 16_450],
        max_luminance: 10_000_000,
        min_luminance: 1,
    };
    let with = |edit: fn(&mut MasteringDisplay)| {
        let mut value = p3;
        edit(&mut value);
        value
    };
    vec![
        (p3, Ok(())),
        (with(|m| m.min_luminance = 0), Ok(())),
        (with(|m| m.max_luminance = MASTERING_MIN_PEAK), Ok(())),
        (with(|m| m.max_luminance = MASTERING_MAX_PEAK), Ok(())),
        (with(|m| m.min_luminance = MASTERING_MAX_BLACK), Ok(())),
        (with(|m| m.white_point = [50_001, 0]), Err(Chromaticity)),
        (
            with(|m| m.white_point = [25_000, 25_001]),
            Err(Chromaticity),
        ),
        (with(|m| m.primaries[2] = [0, 3_000]), Err(Chromaticity)),
        (with(|m| m.primaries[0][1] = 0), Err(Chromaticity)),
        // G, B, R order is clockwise in R, G, B terms.
        (
            with(|m| m.primaries = [m.primaries[1], m.primaries[0], m.primaries[2]]),
            Err(Primaries),
        ),
        (with(|m| m.primaries[2] = m.primaries[0]), Err(Primaries)),
        (with(|m| m.white_point = [40_000, 9_000]), Err(Primaries)),
        (with(|m| m.max_luminance = 0), Err(Luminance)),
        (
            with(|m| m.max_luminance = MASTERING_MIN_PEAK - 1),
            Err(Luminance),
        ),
        (
            with(|m| m.max_luminance = MASTERING_MAX_PEAK + 1),
            Err(Luminance),
        ),
        (
            with(|m| m.min_luminance = MASTERING_MAX_BLACK + 1),
            Err(Luminance),
        ),
        (
            with(|m| {
                m.max_luminance = MASTERING_MIN_PEAK;
                m.min_luminance = MASTERING_MIN_PEAK;
            }),
            Err(Luminance),
        ),
    ]
}

/// Shared boundary cases for the content-light rule.
#[doc(hidden)]
pub fn content_light_rule_cases() -> Vec<(ContentLight, bool)> {
    let light = |max_cll, max_fall| ContentLight { max_cll, max_fall };
    vec![
        (light(0, 0), true),
        (light(1_000, 400), true),
        (light(10_000, 10_000), true),
        (light(10_001, 0), false),
        (light(100, 400), false),
        (light(0, 1), false),
        (light(u16::MAX, u16::MAX), false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mastering_volume_bounds() {
        for (index, (volume, expected)) in mastering_rule_cases().into_iter().enumerate() {
            assert_eq!(volume.check(), expected, "case {index}");
            assert_eq!(volume.is_valid(), expected.is_ok(), "case {index}");
        }
        assert_eq!(mastering_rule_cases()[0].0.peak_nits(), 1_000);
        for (index, (light, expected)) in content_light_rule_cases().into_iter().enumerate() {
            assert_eq!(light.is_valid(), expected, "case {index}");
        }
    }
}
