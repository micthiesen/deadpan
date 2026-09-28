//! Closed, capacity-bounded wire records. Validation runs for both typed
//! constructors and serde callers; public values cannot hold invalid recipes.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::*;

impl<'de> Deserialize<'de> for GainCurve {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        // An internally tagged serde enum buffers arbitrary values before its
        // variant validates them. This map retains only one tag and two typed
        // finite controls, rejecting unknown/duplicate keys before their values.
        decoder.deserialize_map(CurveVisitor)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CurveTag {
    Step,
    Linear,
    Smoothstep,
    Cubic,
}

impl<'de> Deserialize<'de> for CurveTag {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct TagVisitor;
        impl de::Visitor<'_> for TagVisitor {
            type Value = CurveTag;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a gain curve type string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<CurveTag, E> {
                match value {
                    "step" => Ok(CurveTag::Step),
                    "linear" => Ok(CurveTag::Linear),
                    "smoothstep" => Ok(CurveTag::Smoothstep),
                    "cubic" => Ok(CurveTag::Cubic),
                    _ => Err(E::unknown_variant(
                        value,
                        &["step", "linear", "smoothstep", "cubic"],
                    )),
                }
            }
        }
        decoder.deserialize_str(TagVisitor)
    }
}

#[derive(Clone, Copy)]
enum CurveField {
    Type,
    Control1,
    Control2,
}

impl<'de> Deserialize<'de> for CurveField {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct FieldVisitor;
        impl de::Visitor<'_> for FieldVisitor {
            type Value = CurveField;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a gain curve field")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<CurveField, E> {
                match value {
                    "type" => Ok(CurveField::Type),
                    "control1" => Ok(CurveField::Control1),
                    "control2" => Ok(CurveField::Control2),
                    _ => Err(E::unknown_field(value, &["type", "control1", "control2"])),
                }
            }
        }
        decoder.deserialize_identifier(FieldVisitor)
    }
}

struct CurveVisitor;
impl<'de> de::Visitor<'de> for CurveVisitor {
    type Value = GainCurve;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a flat gain curve record")
    }
    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<GainCurve, A::Error> {
        let mut tag = None;
        let mut control1 = None;
        let mut control2 = None;
        while let Some(field) = map.next_key()? {
            match field {
                CurveField::Type => {
                    if tag.is_some() {
                        return Err(de::Error::duplicate_field("type"));
                    }
                    let value = map.next_value::<CurveTag>()?;
                    if value != CurveTag::Cubic && (control1.is_some() || control2.is_some()) {
                        return Err(de::Error::custom(
                            "only a cubic gain curve accepts controls",
                        ));
                    }
                    tag = Some(value);
                }
                CurveField::Control1 | CurveField::Control2 => {
                    let (slot, name) = if matches!(field, CurveField::Control1) {
                        (&mut control1, "control1")
                    } else {
                        (&mut control2, "control2")
                    };
                    if slot.is_some() {
                        return Err(de::Error::duplicate_field(name));
                    }
                    if tag.is_some_and(|tag| tag != CurveTag::Cubic) {
                        return Err(de::Error::custom(
                            "only a cubic gain curve accepts controls",
                        ));
                    }
                    *slot = Some(map.next_value::<GainDb>()?);
                }
            }
        }
        match tag.ok_or_else(|| de::Error::missing_field("type"))? {
            CurveTag::Step => Ok(GainCurve::Step),
            CurveTag::Linear => Ok(GainCurve::Linear),
            CurveTag::Smoothstep => Ok(GainCurve::Smoothstep),
            CurveTag::Cubic => Ok(GainCurve::Cubic {
                control1: control1.ok_or_else(|| de::Error::missing_field("control1"))?,
                control2: control2.ok_or_else(|| de::Error::missing_field("control2"))?,
            }),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Range {
    start: ExactRatio,
    end: ExactRatio,
}
impl TryFrom<Range> for GainRange {
    type Error = GainError;
    fn try_from(value: Range) -> Result<Self, Self::Error> {
        Self::new(value.start, value.end)
    }
}
impl From<GainRange> for Range {
    fn from(value: GainRange) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Segment {
    end: ExactRatio,
    value: GainDb,
    curve: GainCurve,
}
impl TryFrom<Segment> for GainSegment {
    type Error = GainError;
    fn try_from(value: Segment) -> Result<Self, Self::Error> {
        Self::new(value.end, value.value, value.curve)
    }
}
impl From<GainSegment> for Segment {
    fn from(value: GainSegment) -> Self {
        Self {
            end: value.end,
            value: value.value,
            curve: value.curve,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Envelope {
    clock: GainClock,
    range: GainRange,
    initial: GainDb,
    #[serde(deserialize_with = "segments")]
    segments: Vec<GainSegment>,
}
impl TryFrom<Envelope> for GainEnvelope {
    type Error = GainError;
    fn try_from(value: Envelope) -> Result<Self, Self::Error> {
        Self::new(value.clock, value.range, value.initial, value.segments)
    }
}
impl From<GainEnvelope> for Envelope {
    fn from(value: GainEnvelope) -> Self {
        Self {
            clock: value.clock,
            range: value.range,
            initial: value.initial,
            segments: value.segments,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Clip {
    trim: GainDb,
    muted: bool,
    #[serde(deserialize_with = "envelopes")]
    envelopes: Vec<GainEnvelope>,
    #[serde(deserialize_with = "mute_ranges")]
    mute_ranges: Vec<GainRange>,
}
impl TryFrom<Clip> for ClipGain {
    type Error = GainError;
    fn try_from(value: Clip) -> Result<Self, Self::Error> {
        Self::new(value.trim, value.muted, value.envelopes, value.mute_ranges)
    }
}
impl From<ClipGain> for Clip {
    fn from(value: ClipGain) -> Self {
        Self {
            trim: value.trim,
            muted: value.muted,
            envelopes: value.envelopes,
            mute_ranges: value.mute_ranges,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Treatments {
    #[serde(deserialize_with = "stages")]
    order: Vec<AudioTreatmentStage>,
    // Required, including explicit null for the empty treatment chain.
    #[serde(deserialize_with = "required_clip")]
    clip_gain: Option<ClipGain>,
}
impl TryFrom<Treatments> for AudioTreatments {
    type Error = GainError;
    fn try_from(value: Treatments) -> Result<Self, Self::Error> {
        Self::new(value.order, value.clip_gain)
    }
}
impl From<AudioTreatments> for Treatments {
    fn from(value: AudioTreatments) -> Self {
        Self {
            order: value.order,
            clip_gain: value.clip_gain,
        }
    }
}

fn required_clip<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<ClipGain>, D::Error> {
    Option::deserialize(decoder)
}
fn segments<'de, D: Deserializer<'de>>(decoder: D) -> Result<Vec<GainSegment>, D::Error> {
    bounded::<D, GainSegment, MAX_GAIN_SEGMENTS>(decoder)
}
fn envelopes<'de, D: Deserializer<'de>>(decoder: D) -> Result<Vec<GainEnvelope>, D::Error> {
    bounded::<D, GainEnvelope, MAX_GAIN_ENVELOPES>(decoder)
}
fn mute_ranges<'de, D: Deserializer<'de>>(decoder: D) -> Result<Vec<GainRange>, D::Error> {
    bounded::<D, GainRange, MAX_GAIN_MUTE_RANGES>(decoder)
}
fn stages<'de, D: Deserializer<'de>>(decoder: D) -> Result<Vec<AudioTreatmentStage>, D::Error> {
    bounded::<D, AudioTreatmentStage, MAX_AUDIO_TREATMENT_STAGES>(decoder)
}

fn bounded<'de, D, T, const N: usize>(decoder: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "at most {N} gain records")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            if seq.size_hint().is_some_and(|size| size > N) {
                return Err(de::Error::custom(GainError::Limit));
            }
            let mut values = Vec::new();
            while values.len() < N {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            // Reject before asking an over-limit element to deserialize, even
            // when its recursive payload would itself be costly or malformed.
            seq.next_element_seed(Reject)?;
            Ok(values)
        }
    }
    decoder.deserialize_seq(Visitor::<T, N>(std::marker::PhantomData))
}

struct Reject;
impl<'de> de::DeserializeSeed<'de> for Reject {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(de::Error::custom(GainError::Limit))
    }
}
