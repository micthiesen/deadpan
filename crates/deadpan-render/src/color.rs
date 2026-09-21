/// Transfer interpretation for normalized source RGB, separate from primaries.
/// `Rec709` is the inverse BT.709 OETF, not a display gamma approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Srgb,
    Rec709,
    Linear,
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
    }
}

/// CPU reference source interpretation, using two f64 XYZ matrix operations.
/// No clipping occurs here, including negative or above-reference working values.
pub fn source_to_working(rgb: [f64; 3], color: SourceColor) -> [f64; 3] {
    let linear = rgb.map(|value| decode(value, color.transfer));
    multiply(
        inverse(to_xyz(Primaries::Rec2020)),
        multiply(to_xyz(color.primaries), linear),
    )
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
