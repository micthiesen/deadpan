/// Transfer interpretation for normalized source RGB, separate from primaries.
/// `Rec709` is the inverse BT.709 OETF, not a display gamma approximation.
/// `Pq` (SMPTE ST 2084) and `Hlg` (ARIB STD-B67) are BT.2100 HDR transfers;
/// they decode to display light placed with working 1.0 at
/// [`HDR_REFERENCE_WHITE_NITS`]. HLG applies its whole-RGB OOTF on Rec.2020
/// scene light with a [`HLG_NOMINAL_PEAK_NITS`] display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Srgb,
    Rec709,
    Linear,
    Pq,
    Hlg,
}

impl Transfer {
    /// True for the BT.2100 HDR transfers.
    pub const fn is_hdr(self) -> bool {
        matches!(self, Self::Pq | Self::Hlg)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primaries {
    Rec709,
    Rec2020,
    DisplayP3D65,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceColor {
    pub transfer: Transfer,
    pub primaries: Primaries,
}

pub(crate) type Matrix = [[f64; 3]; 3];

// RGB-to-XYZ matrices from the named chromaticities and D65 (x=.3127,y=.3290).
// Transform definitions: https://www.w3.org/TR/css-color-4/#color-conversion-code
// BT.709 transfer: https://www.itu.int/rec/R-REC-BT.709
fn to_xyz(primaries: Primaries) -> Matrix {
    match primaries {
        Primaries::Rec709 => [
            [0.4123907992659595, 0.35758433938387796, 0.1804807884018343],
            [0.21263900587151036, 0.7151686787677559, 0.07219231536073371],
            [0.01933081871559185, 0.11919477979462599, 0.9505321522496607],
        ],
        Primaries::Rec2020 => [
            [0.6369580483012914, 0.14461690358620832, 0.1688809751641721],
            [0.2627002120112671, 0.6779980715188708, 0.05930171646986196],
            [0.0, 0.028072693049087428, 1.060985057710791],
        ],
        Primaries::DisplayP3D65 => [
            [0.4865709486482162, 0.26566769316909306, 0.1982172852343625],
            [0.2289745640697488, 0.6917385218365064, 0.079286914093745],
            [0.0, 0.04511338185890264, 1.043944368900976],
        ],
    }
}

fn inverse(m: Matrix) -> Matrix {
    let [[a, b, c], [d, e, f], [g, h, i]] = m;
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ]
    .map(|row| row.map(|value| value / determinant))
}

pub(crate) fn multiply(m: Matrix, v: [f64; 3]) -> [f64; 3] {
    m.map(|row| row.into_iter().zip(v).map(|(a, b)| a * b).sum())
}

pub(crate) fn conversion(from: Primaries, to: Primaries) -> Matrix {
    let a = inverse(to_xyz(to));
    let b = to_xyz(from);
    std::array::from_fn(|row| {
        std::array::from_fn(|column| (0..3).map(|k| a[row][k] * b[k][column]).sum())
    })
}

pub(crate) fn decode(value: f64, transfer: Transfer) -> f64 {
    match transfer {
        Transfer::Srgb if value <= 0.04045 => value / 12.92,
        Transfer::Srgb => ((value + 0.055) / 1.055).powf(2.4),
        Transfer::Rec709 if value < 0.081 => value / 4.5,
        Transfer::Rec709 => ((value + 0.099) / 1.099).powf(1.0 / 0.45),
        Transfer::Linear => value,
        Transfer::Pq => pq_eotf(value) / HDR_REFERENCE_WHITE_NITS,
        // Scene light; the OOTF follows the primaries transform.
        Transfer::Hlg => hlg_inverse_oetf(value),
    }
}

/// BT.709 OETF after the final SDR linear-light gamut/reference-white clip.
/// Apply the primaries transform before this function; clipping the Rec.2020
/// working channels first would change colors. This is not HDR tone mapping.
pub(crate) fn encode_rec709(value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    if value < 0.018 {
        4.5 * value
    } else {
        1.099 * value.powf(0.45) - 0.099
    }
}

/// CPU reference source interpretation, using two f64 XYZ matrix operations.
/// No clipping occurs here, including negative or above-reference working values.
/// PQ decodes to display light / 203 cd/m^2. HLG decodes to scene light,
/// converts it to Rec.2020, then applies the BT.2100 OOTF (1000 cd/m^2, gamma
/// 1.2) on Rec.2020 luminance and divides by 203 cd/m^2.
pub fn source_to_working(rgb: [f64; 3], color: SourceColor) -> [f64; 3] {
    let linear = rgb.map(|value| decode(value, color.transfer));
    let working = multiply(
        inverse(to_xyz(Primaries::Rec2020)),
        multiply(to_xyz(color.primaries), linear),
    );
    if color.transfer == Transfer::Hlg {
        hlg_ootf(working).map(|nits| nits / HDR_REFERENCE_WHITE_NITS)
    } else {
        working
    }
}

/// BT.2408 HDR reference (graphics) white: working 1.0 in cd/m^2.
pub const HDR_REFERENCE_WHITE_NITS: f64 = 203.0;
/// PQ absolute peak luminance in cd/m^2.
pub const PQ_PEAK_NITS: f64 = 10_000.0;
/// Nominal HLG display peak (BT.2100 reference display) in cd/m^2.
pub const HLG_NOMINAL_PEAK_NITS: f64 = 1000.0;
/// HLG system gamma for the nominal 1000 cd/m^2 display.
pub const HLG_SYSTEM_GAMMA: f64 = 1.2;

/// Rec.2020 (BT.2100) luminance weights.
pub(crate) const REC2020_LUMA: [f64; 3] = [0.2627, 0.6780, 0.0593];

const PQ_M1: f64 = 2610.0 / 16384.0;
const PQ_M2: f64 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f64 = 3424.0 / 4096.0;
const PQ_C2: f64 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f64 = 2392.0 / 4096.0 * 32.0;

const HLG_A: f64 = 0.178_832_77;
const HLG_B: f64 = 1.0 - 4.0 * HLG_A;
// c = 0.5 - a ln(4a) = 0.5599107295, evaluated in f64 rather than BT.2100's
// rounded 0.55991073. The difference (4.7e-10) is intentional: the WGSL shader
// uses the published constant (identical in f32), and the independent
// qualification reference uses it so that it shares no derivation with this
// production reference.
fn hlg_c() -> f64 {
    0.5 - HLG_A * (4.0 * HLG_A).ln()
}

/// SMPTE ST 2084 EOTF: nonlinear signal E' in [0, 1] to cd/m^2 in [0, 10000].
/// Inputs outside [0, 1] are clamped first.
pub fn pq_eotf(signal: f64) -> f64 {
    let power = signal.clamp(0.0, 1.0).powf(1.0 / PQ_M2);
    ((power - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * power)).powf(1.0 / PQ_M1) * PQ_PEAK_NITS
}

/// SMPTE ST 2084 inverse EOTF: cd/m^2, clamped to [0, 10000], to E' in [0, 1].
pub fn pq_inverse_eotf(nits: f64) -> f64 {
    let power = (nits.clamp(0.0, PQ_PEAK_NITS) / PQ_PEAK_NITS).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * power) / (1.0 + PQ_C3 * power)).powf(PQ_M2)
}

/// Verification helper: a limited-range 10-bit code (64 black, 940 nominal
/// peak) interpreted as a PQ signal, returned in cd/m^2. Codes outside the
/// nominal range clamp to it.
pub fn pq_code_to_nits(code: u16) -> f64 {
    pq_eotf((f64::from(code) - 64.0) / 876.0)
}

/// BT.2100 HLG OETF: normalized scene light E in [0, 1] to E' in [0, 1].
/// Inputs are clamped to [0, 1].
pub fn hlg_oetf(scene: f64) -> f64 {
    let scene = scene.clamp(0.0, 1.0);
    if scene <= 1.0 / 12.0 {
        (3.0 * scene).sqrt()
    } else {
        HLG_A * (12.0 * scene - HLG_B).ln() + hlg_c()
    }
}

/// BT.2100 HLG inverse OETF: E' in [0, 1] (clamped) to scene light in [0, 1].
pub fn hlg_inverse_oetf(signal: f64) -> f64 {
    let signal = signal.clamp(0.0, 1.0);
    if signal <= 0.5 {
        signal * signal / 3.0
    } else {
        (((signal - hlg_c()) / HLG_A).exp() + HLG_B) / 12.0
    }
}

/// BT.2100 HLG OOTF for the nominal display (Lw 1000 cd/m^2, black 0, gamma
/// 1.2): Rec.2020 scene RGB to display light in cd/m^2. Scene luminance below
/// zero (possible only for out-of-gamut signed input) is treated as zero.
pub fn hlg_ootf(scene: [f64; 3]) -> [f64; 3] {
    let luminance = dot(REC2020_LUMA, scene).max(0.0);
    let gain = HLG_NOMINAL_PEAK_NITS * luminance.powf(HLG_SYSTEM_GAMMA - 1.0);
    scene.map(|value| gain * value)
}

/// Inverse of [`hlg_ootf`]: Rec.2020 display light in cd/m^2 to scene RGB.
/// Zero (or negative) display luminance maps to zero scene light.
pub fn hlg_inverse_ootf(display: [f64; 3]) -> [f64; 3] {
    let luminance = dot(REC2020_LUMA, display) / HLG_NOMINAL_PEAK_NITS;
    if luminance <= 0.0 {
        return [0.0; 3];
    }
    let gain = luminance.powf((1.0 - HLG_SYSTEM_GAMMA) / HLG_SYSTEM_GAMMA) / HLG_NOMINAL_PEAK_NITS;
    display.map(|value| gain * value)
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// SDR display transform: linear Rec.2020 to linear Rec.709, clip to display
/// gamut/reference white, then encode sRGB. This is not an HDR tone mapper.
pub fn working_to_display(rgb: [f64; 3]) -> [f64; 3] {
    let display_linear = multiply(
        inverse(to_xyz(Primaries::Rec709)),
        multiply(to_xyz(Primaries::Rec2020), rgb),
    );
    display_linear.map(|value| {
        let value = value.clamp(0.0, 1.0);
        if value <= 0.0031308 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: [f64; 3], expected: [f64; 3], epsilon: f64) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < epsilon,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn named_primaries_match_independent_known_red_and_white() {
        close(
            source_to_working(
                [1.0, 0.0, 0.0],
                SourceColor {
                    transfer: Transfer::Linear,
                    primaries: Primaries::Rec709,
                },
            ),
            [0.627403896, 0.069097289, 0.016391439],
            1e-8,
        );
        for primaries in [
            Primaries::Rec709,
            Primaries::Rec2020,
            Primaries::DisplayP3D65,
        ] {
            close(
                source_to_working(
                    [1.0; 3],
                    SourceColor {
                        transfer: Transfer::Linear,
                        primaries,
                    },
                ),
                [1.0; 3],
                1e-12,
            );
        }
    }

    #[test]
    fn transfer_functions_are_distinct_and_round_trip_srgb() {
        assert!((decode(0.5, Transfer::Srgb) - 0.21404114048223255).abs() < 1e-12);
        assert!((decode(0.5, Transfer::Rec709) - 0.25958940050628576).abs() < 1e-12);
        assert_eq!(decode(0.5, Transfer::Linear), 0.5);
        assert_eq!(decode(0.045, Transfer::Rec709), 0.01);
        for value in [0.0, 0.01, 0.04, 0.08, 0.5, 1.0] {
            close(
                working_to_display(source_to_working(
                    [value; 3],
                    SourceColor {
                        transfer: Transfer::Srgb,
                        primaries: Primaries::Rec709,
                    },
                )),
                [value; 3],
                1e-12,
            );
        }
    }

    #[test]
    fn encoder_oetf_uses_the_rec709_threshold_and_final_sdr_clip() {
        assert_eq!(encode_rec709(-0.25), 0.0);
        assert_eq!(encode_rec709(2.0), 1.0);
        assert!((encode_rec709(0.017999) - 0.0809955).abs() < 1e-12);
        assert!((encode_rec709(0.018) - 0.08124794403514049).abs() < 1e-12);
        assert!((encode_rec709(0.5) - 0.7055150899221212).abs() < 1e-12);
    }

    #[test]
    fn pq_matches_published_bt2100_anchors_and_round_trips() {
        // BT.2100 / BT.2408 published PQ signal levels.
        for (signal, nits, tolerance) in [
            (0.0, 0.0, 1e-9),
            (0.5081, 100.0, 0.05),
            (0.58069, 203.0, 0.02),
            (0.7518, 1000.0, 0.5),
            (1.0, 10_000.0, 1e-9),
        ] {
            let actual = pq_eotf(signal);
            assert!((actual - nits).abs() < tolerance, "{signal}: {actual}");
        }
        assert!((pq_inverse_eotf(203.0) - 0.58069).abs() < 5e-5);
        assert!((pq_inverse_eotf(1000.0) - 0.75183).abs() < 5e-5);
        assert!((pq_inverse_eotf(10_000.0) - 1.0).abs() < 1e-12);
        // Zero light is the ST 2084 offset c1^m2, not exactly zero.
        assert!(pq_inverse_eotf(0.0) > 0.0 && pq_inverse_eotf(0.0) < 1e-6);
        for nits in [0.005, 0.1, 1.0, 100.0, 203.0, 1000.0, 4000.0, 10_000.0] {
            let round_trip = pq_eotf(pq_inverse_eotf(nits));
            assert!((round_trip - nits).abs() < 1e-9 * nits.max(1.0), "{nits}");
        }
        assert_eq!(pq_inverse_eotf(20_000.0), pq_inverse_eotf(10_000.0));
        assert_eq!(pq_code_to_nits(64), pq_eotf(0.0));
        assert!((pq_code_to_nits(940) - 10_000.0).abs() < 1e-9);
        // Limited 10-bit code for 203 cd/m^2: 64 + 876 * 0.58069 = 572.7.
        assert!(pq_code_to_nits(573) > 203.0 && pq_code_to_nits(572) < 203.0);
        let working = source_to_working(
            [0.58069; 3],
            SourceColor {
                transfer: Transfer::Pq,
                primaries: Primaries::Rec2020,
            },
        );
        close(working, [1.0; 3], 2e-4);
    }

    #[test]
    fn hlg_matches_published_bt2100_anchors_and_round_trips() {
        assert_eq!(hlg_oetf(0.0), 0.0);
        assert!((hlg_oetf(1.0 / 12.0) - 0.5).abs() < 1e-12);
        assert!((hlg_oetf(1.0) - 1.0).abs() < 1e-7);
        assert!((hlg_inverse_oetf(0.5) - 1.0 / 12.0).abs() < 1e-12);
        assert!((hlg_inverse_oetf(1.0) - 1.0).abs() < 1e-7);
        for scene in [0.0, 0.001, 1.0 / 12.0, 0.2, 0.5, 0.9, 1.0] {
            assert!((hlg_inverse_oetf(hlg_oetf(scene)) - scene).abs() < 1e-12);
        }
        // BT.2408: 75% HLG is ~203 cd/m^2 on the nominal 1000 cd/m^2 display.
        let reference = hlg_ootf([hlg_inverse_oetf(0.75); 3]);
        close(reference, [203.0; 3], 0.5);
        // Full signal is the nominal display peak; the OOTF is whole-RGB.
        close(hlg_ootf([1.0; 3]), [HLG_NOMINAL_PEAK_NITS; 3], 1e-9);
        let blue = hlg_ootf([0.0, 0.0, 1.0]);
        assert!((blue[2] - 1000.0 * 0.0593_f64.powf(0.2)).abs() < 1e-9);
        for scene in [[0.2, 0.5, 0.1], [1.0, 0.0, 0.0], [0.01, 0.01, 0.02]] {
            close(hlg_inverse_ootf(hlg_ootf(scene)), scene, 1e-12);
        }
        assert_eq!(hlg_inverse_ootf([0.0; 3]), [0.0; 3]);
        let working = source_to_working(
            [0.75; 3],
            SourceColor {
                transfer: Transfer::Hlg,
                primaries: Primaries::Rec2020,
            },
        );
        close(working, [1.0; 3], 3e-3);
        assert!(Transfer::Hlg.is_hdr() && Transfer::Pq.is_hdr() && !Transfer::Srgb.is_hdr());
    }

    #[test]
    fn working_values_are_not_clipped_to_a_normalized_texture() {
        let red = source_to_working(
            [1.0, 0.0, 0.0],
            SourceColor {
                transfer: Transfer::Linear,
                primaries: Primaries::DisplayP3D65,
            },
        );
        assert!(red[2] < -0.001);
        close(
            source_to_working(
                [4.0, -0.25, 2.0],
                SourceColor {
                    transfer: Transfer::Linear,
                    primaries: Primaries::Rec2020,
                },
            ),
            [4.0, -0.25, 2.0],
            1e-12,
        );
        close(working_to_display([2.0; 3]), [1.0; 3], 1e-12);
    }
}
