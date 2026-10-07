//! Measured boundary-picture evidence carried by a version-2 bridge context.
//!
//! Conditioning decodes the project picture on each side of a Hold through the
//! shared picture path. The source decoder applies the stream's declared matrix
//! and range to produce full-range RGB, leaving transfer and primaries
//! unchanged. These types record what that decoder measured for the picture
//! actually used (or that the neighbour was authored black), and the single
//! model-input conversion Deadpan states for it. They are evidence of what the
//! host fed the model, not a claim about the model's own colour handling.

use deadpan_core::{
    AssetId, GeneratedObjectRef, SourceFrameId, SourceQualificationId, SourceTimestamp,
};
use serde::{Deserialize, Serialize};

const MAXIMUM_LABEL_BYTES: usize = 64;
const MAXIMUM_DIMENSION: u32 = 8192;

/// A stream's declared transfer characteristic, as the decoder admitted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeTransfer {
    Bt709,
    Srgb,
    Linear,
    Pq,
    Hlg,
}

/// A stream's declared colour primaries, as the decoder admitted them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgePrimaries {
    Bt709,
    Bt2020,
    DisplayP3,
}

/// A stream's declared YUV-to-RGB matrix (`rgb` for RGB-coded streams).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeMatrix {
    Rgb,
    Bt709,
    Bt601,
    Bt2020Ncl,
}

/// A stream's declared code range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeRange {
    Limited,
    Full,
}

/// One complete colour description from a closed vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeColor {
    pub transfer: BridgeTransfer,
    pub primaries: BridgePrimaries,
    pub matrix: BridgeMatrix,
    pub range: BridgeRange,
}

/// Full-range RGB with the sRGB transfer and BT.709 primaries.
///
/// This is the bridge model's declared input/output space and the exact tag
/// set the canonical FFV1 converter writes and verifies on both masters
/// (`native/deadpan-media-worker/src/converter.c`); candidate and accepted
/// master decoders re-check it before showing a picture.
pub const CANONICAL_BRIDGE_COLOR: BridgeColor = BridgeColor {
    transfer: BridgeTransfer::Srgb,
    primaries: BridgePrimaries::Bt709,
    matrix: BridgeMatrix::Rgb,
    range: BridgeRange::Full,
};

impl BridgeColor {
    /// Human-readable name used in refusals and qualification failures.
    pub fn describe(self) -> String {
        format!(
            "{} transfer, {} primaries, {} matrix, {} range",
            label(self.transfer),
            label(self.primaries),
            label(self.matrix),
            label(self.range)
        )
    }
}

fn label(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The decoder's stream description for the picture actually decoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredStream {
    pub codec: String,
    pub pixel_format: String,
    pub width: u32,
    pub height: u32,
    /// Sample aspect ratio `[numerator, denominator]`.
    pub sample_aspect: [u32; 2],
    pub rotation_quarter_turns: u8,
    /// Bits per channel of the decoder's RGB output: 8, or 16 for HDR.
    pub decoded_sample_bits: u8,
    pub color: BridgeColor,
}

impl MeasuredStream {
    fn validate_shape(&self) -> Result<(), &'static str> {
        for text in [&self.codec, &self.pixel_format] {
            if text.is_empty()
                || text.len() > MAXIMUM_LABEL_BYTES
                || !text.bytes().all(|byte| byte.is_ascii_graphic())
            {
                return Err("invalid measured codec or pixel format");
            }
        }
        if !(1..=MAXIMUM_DIMENSION).contains(&self.width)
            || !(1..=MAXIMUM_DIMENSION).contains(&self.height)
            || self.sample_aspect.contains(&0)
            || self.rotation_quarter_turns > 3
            || !matches!(self.decoded_sample_bits, 8 | 16)
        {
            return Err("invalid measured stream geometry or depth");
        }
        Ok(())
    }
}

/// How a decoded picture's RGB codes became the model's input.
///
/// The decoder has already applied the declared matrix and range. Current
/// conditioning converts BT.709 transfer codes to sRGB; older retained inputs
/// may instead carry the explicitly recorded approximation below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInputConversion {
    /// sRGB-transfer BT.709 codes, which are already the model's input space.
    SrgbCodesUnchanged,
    /// BT.709-transfer BT.709 codes, interpreted as sRGB. This is a stated
    /// approximation: the BT.709 OETF and the sRGB curve differ.
    Rec709CodesAsSrgb,
    /// Inverse BT.709 OETF followed by the sRGB encoding curve, rounded once
    /// to full-range RGB8 before fitting the picture to the model raster.
    Rec709ToSrgb,
}

/// Why a measured picture cannot condition the bridge model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditioningColorRefusal {
    Rotated,
    Hdr,
    DeepSamples,
    LinearTransfer,
    WideGamut(BridgePrimaries),
}

impl std::fmt::Display for ConditioningColorRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rotated => formatter.write_str("rotated pictures cannot condition an AI pause yet"),
            Self::Hdr => formatter.write_str(
                "HDR (PQ/HLG) pictures cannot condition an AI pause yet; the bridge model takes SDR sRGB and no qualified tone mapping exists",
            ),
            Self::DeepSamples => formatter.write_str(
                "only eight-bit decoded SDR pictures can condition an AI pause",
            ),
            Self::LinearTransfer => formatter.write_str(
                "linear-light pictures cannot condition an AI pause: their codes are not sRGB-encoded and no transfer conversion is applied",
            ),
            Self::WideGamut(primaries) => write!(
                formatter,
                "{} primaries cannot condition an AI pause: no gamut conversion to the model's BT.709 sRGB is applied",
                label(primaries)
            ),
        }
    }
}

/// The conversion that feeds `stream`'s decoded RGB8 to the sRGB bridge model,
/// or the reason no stated conversion covers it.
///
/// Every matrix and range the source decoder admits is covered, because the
/// decoder applies it before RGB output. Only sRGB or BT.709 transfer with
/// BT.709 primaries qualifies; BT.709 transfer is converted to sRGB.
pub fn model_input_conversion(
    stream: &MeasuredStream,
) -> Result<ModelInputConversion, ConditioningColorRefusal> {
    let color = stream.color;
    if stream.rotation_quarter_turns != 0 {
        return Err(ConditioningColorRefusal::Rotated);
    }
    if matches!(color.transfer, BridgeTransfer::Pq | BridgeTransfer::Hlg) {
        return Err(ConditioningColorRefusal::Hdr);
    }
    if stream.decoded_sample_bits != 8 {
        return Err(ConditioningColorRefusal::DeepSamples);
    }
    if color.primaries != BridgePrimaries::Bt709 {
        return Err(ConditioningColorRefusal::WideGamut(color.primaries));
    }
    match color.transfer {
        BridgeTransfer::Srgb => Ok(ModelInputConversion::SrgbCodesUnchanged),
        BridgeTransfer::Bt709 => Ok(ModelInputConversion::Rec709ToSrgb),
        BridgeTransfer::Linear => Err(ConditioningColorRefusal::LinearTransfer),
        BridgeTransfer::Pq | BridgeTransfer::Hlg => Err(ConditioningColorRefusal::Hdr),
    }
}

/// One decoded picture: its exact frame identity, PTS and measured stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecodedBoundary {
    /// The measured index identity (original ordinal) of the decoded frame.
    pub source_frame: SourceFrameId,
    /// Its exact source presentation timestamp.
    pub pts: SourceTimestamp,
    pub stream: MeasuredStream,
    pub model_input: ModelInputConversion,
}

/// What the project showed on one side of the Hold at the origin revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundaryPicture {
    /// A frame of the Original, admitted against its revision's receipt.
    Original {
        project_frame: i64,
        asset: AssetId,
        qualification: SourceQualificationId,
        picture: DecodedBoundary,
    },
    /// A frame of an accepted generated Hold's sampled master.
    Generated {
        project_frame: i64,
        sampled_asset: AssetId,
        sampled_object: GeneratedObjectRef,
        provenance: GeneratedObjectRef,
        picture: DecodedBoundary,
    },
    /// Authored Background/Blank: black RGB, nothing decoded.
    AuthoredBlack { project_frame: i64 },
}

impl BoundaryPicture {
    pub const fn project_frame(&self) -> i64 {
        match self {
            Self::Original { project_frame, .. }
            | Self::Generated { project_frame, .. }
            | Self::AuthoredBlack { project_frame } => *project_frame,
        }
    }

    pub const fn decoded(&self) -> Option<&DecodedBoundary> {
        match self {
            Self::Original { picture, .. } | Self::Generated { picture, .. } => Some(picture),
            Self::AuthoredBlack { .. } => None,
        }
    }

    pub(crate) fn validate_shape(&self) -> Result<(), &'static str> {
        if self.project_frame() < 0 {
            return Err("negative boundary project frame");
        }
        if let Some(picture) = self.decoded() {
            picture.stream.validate_shape()?;
            let current = model_input_conversion(&picture.stream);
            let retained_approximation = current == Ok(ModelInputConversion::Rec709ToSrgb)
                && picture.model_input == ModelInputConversion::Rec709CodesAsSrgb;
            if current != Ok(picture.model_input) && !retained_approximation {
                return Err("boundary model-input conversion contradicts its measured colour");
            }
        }
        if let Self::Generated { picture, .. } = self
            && picture.stream.color != CANONICAL_BRIDGE_COLOR
        {
            return Err("generated boundary is not canonical sRGB");
        }
        Ok(())
    }
}

/// The measured pictures on both sides of the Hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeBoundaries {
    pub left: BoundaryPicture,
    pub right: BoundaryPicture,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(color: BridgeColor) -> MeasuredStream {
        MeasuredStream {
            codec: "h264".into(),
            pixel_format: "yuv420p".into(),
            width: 320,
            height: 180,
            sample_aspect: [1, 1],
            rotation_quarter_turns: 0,
            decoded_sample_bits: 8,
            color,
        }
    }

    fn color(transfer: BridgeTransfer, primaries: BridgePrimaries) -> BridgeColor {
        BridgeColor {
            transfer,
            primaries,
            matrix: BridgeMatrix::Bt709,
            range: BridgeRange::Limited,
        }
    }

    #[test]
    fn only_sdr_bt709_primaries_with_srgb_or_bt709_transfer_condition() {
        use BridgePrimaries as P;
        use BridgeTransfer as T;
        assert_eq!(
            model_input_conversion(&stream(color(T::Bt709, P::Bt709))),
            Ok(ModelInputConversion::Rec709ToSrgb)
        );
        assert_eq!(
            model_input_conversion(&stream(CANONICAL_BRIDGE_COLOR)),
            Ok(ModelInputConversion::SrgbCodesUnchanged)
        );
        // Every decoder-admitted matrix and range is applied before RGB.
        for matrix in [
            BridgeMatrix::Rgb,
            BridgeMatrix::Bt601,
            BridgeMatrix::Bt709,
            BridgeMatrix::Bt2020Ncl,
        ] {
            for range in [BridgeRange::Full, BridgeRange::Limited] {
                let mut measured = stream(color(T::Bt709, P::Bt709));
                measured.color.matrix = matrix;
                measured.color.range = range;
                assert!(model_input_conversion(&measured).is_ok());
            }
        }
        for (transfer, primaries, refusal) in [
            (
                T::Linear,
                P::Bt709,
                ConditioningColorRefusal::LinearTransfer,
            ),
            (
                T::Bt709,
                P::Bt2020,
                ConditioningColorRefusal::WideGamut(P::Bt2020),
            ),
            (
                T::Srgb,
                P::DisplayP3,
                ConditioningColorRefusal::WideGamut(P::DisplayP3),
            ),
            (T::Pq, P::Bt2020, ConditioningColorRefusal::Hdr),
            (T::Hlg, P::Bt2020, ConditioningColorRefusal::Hdr),
        ] {
            assert_eq!(
                model_input_conversion(&stream(color(transfer, primaries))),
                Err(refusal)
            );
        }
        let mut deep = stream(color(T::Bt709, P::Bt709));
        deep.decoded_sample_bits = 16;
        assert_eq!(
            model_input_conversion(&deep),
            Err(ConditioningColorRefusal::DeepSamples)
        );
        let mut rotated = stream(color(T::Bt709, P::Bt709));
        rotated.rotation_quarter_turns = 1;
        assert_eq!(
            model_input_conversion(&rotated),
            Err(ConditioningColorRefusal::Rotated)
        );
        assert!(
            ConditioningColorRefusal::WideGamut(P::DisplayP3)
                .to_string()
                .contains("display_p3")
        );
    }

    #[test]
    fn boundary_shape_rejects_contradictions_and_unknown_fields() {
        let decoded = DecodedBoundary {
            source_frame: SourceFrameId(26),
            pts: SourceTimestamp {
                ticks: 26_026,
                time_base: deadpan_core::SourceTimeBase::new(1, 30_000).unwrap(),
            },
            stream: stream(color(BridgeTransfer::Bt709, BridgePrimaries::Bt709)),
            model_input: ModelInputConversion::Rec709CodesAsSrgb,
        };
        let original = BoundaryPicture::Original {
            project_frame: 14,
            asset: AssetId::new("asset").unwrap(),
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            picture: decoded.clone(),
        };
        original.validate_shape().unwrap();
        let mut converted = original.clone();
        let BoundaryPicture::Original { picture, .. } = &mut converted else {
            unreachable!()
        };
        picture.model_input = ModelInputConversion::Rec709ToSrgb;
        converted.validate_shape().unwrap();
        let wire = serde_json::to_value(&original).unwrap();
        assert_eq!(
            wire["original"]["picture"]["stream"]["color"]["transfer"],
            "bt709"
        );
        assert_eq!(
            serde_json::from_value::<BoundaryPicture>(wire.clone()).unwrap(),
            original
        );
        let mut extra = wire.clone();
        extra["original"]["guess"] = serde_json::json!(true);
        assert!(serde_json::from_value::<BoundaryPicture>(extra).is_err());
        let mut nested = wire;
        nested["original"]["picture"]["stream"]["color"]["gamma"] = serde_json::json!(2.2);
        assert!(serde_json::from_value::<BoundaryPicture>(nested).is_err());

        let mut lying = decoded.clone();
        lying.model_input = ModelInputConversion::SrgbCodesUnchanged;
        let contradiction = BoundaryPicture::Original {
            project_frame: 14,
            asset: AssetId::new("asset").unwrap(),
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            picture: lying,
        };
        assert!(contradiction.validate_shape().is_err());
        let mut wide = decoded;
        wide.stream.color.primaries = BridgePrimaries::DisplayP3;
        let unsupported = BoundaryPicture::Original {
            project_frame: 14,
            asset: AssetId::new("asset").unwrap(),
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            picture: wide,
        };
        assert!(unsupported.validate_shape().is_err());
        assert!(
            BoundaryPicture::AuthoredBlack { project_frame: -1 }
                .validate_shape()
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(BoundaryPicture::AuthoredBlack { project_frame: 3 }).unwrap(),
            serde_json::json!({"authored_black": {"project_frame": 3}})
        );
    }
}
