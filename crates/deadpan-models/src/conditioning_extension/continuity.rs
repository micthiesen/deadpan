//! Retained observations for the conservative temporal-context shot rule.
//!
//! The host binds physical source counts to its qualified indexes. This record
//! lets later reads repeat the policy; it cannot prove a supplied index or
//! signature came from the claimed source. Store admission recaptures identity.

use deadpan_analysis::{
    CONTEXT_SHOT_RULE, CONTEXT_SIGNATURE_ENCODING, CONTEXT_SIGNATURE_HEADER_BYTES,
    ContextShotQualification, MAX_CONTEXT_SHOT_SIGNATURES, MAX_CONTEXT_SIGNATURE_BYTES,
    PICTURE_SIGNATURE_BYTES, PictureSignature, context_seam_change, context_shot_window,
    decode_context_signatures, qualify_context,
};
use deadpan_core::SourceFrameId;
use deadpan_jobs::{GenerationInputBinding, GenerationInputs, GenerationPictureIdentity};
use sha2::Digest;

use super::*;

pub const EXTENSION_CAPTURE_POLICY: &str = "deadpan-extension-context-1";
const MAX_INTERVALS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionContinuityEvidence {
    capture_policy: String,
    shot_rule: String,
    signature_encoding: String,
    binding: GenerationInputBinding,
    /// One identity per binary signature, in exactly the same order.
    #[serde(deserialize_with = "bounded_pictures")]
    pictures: Vec<GenerationPictureIdentity>,
    /// Physical source count for each support span, then its closed terminal.
    /// Only authored black uses null. Counts come from verified media indexes.
    #[serde(deserialize_with = "bounded_counts")]
    source_picture_counts: Vec<Option<usize>>,
    signatures: WorkspaceArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionContextMeasurement {
    pub source: GenerationPictureIdentity,
    pub qualification: ContextShotQualification,
}

impl ExtensionContinuityEvidence {
    pub fn new(
        binding: GenerationInputBinding,
        pictures: Vec<GenerationPictureIdentity>,
        source_picture_counts: Vec<Option<usize>>,
        signatures: WorkspaceArtifact,
    ) -> Result<Self, QualificationError> {
        let value = Self {
            capture_policy: EXTENSION_CAPTURE_POLICY.into(),
            shot_rule: CONTEXT_SHOT_RULE.into(),
            signature_encoding: CONTEXT_SIGNATURE_ENCODING.into(),
            binding,
            pictures,
            source_picture_counts,
            signatures,
        };
        value.validate_shape()?;
        Ok(value)
    }

    pub fn binding(&self) -> &GenerationInputBinding {
        &self.binding
    }

    pub fn signatures(&self) -> &WorkspaceArtifact {
        &self.signatures
    }

    pub fn pictures(&self) -> &[GenerationPictureIdentity] {
        &self.pictures
    }

    pub(super) fn validate_shape(&self) -> Result<(), QualificationError> {
        let GenerationInputs::Extension {
            support, terminal, ..
        } = &self.binding.inputs
        else {
            return Err(extension_error("continuity requires extension inputs"));
        };
        if self.capture_policy != EXTENSION_CAPTURE_POLICY
            || self.shot_rule != CONTEXT_SHOT_RULE
            || self.signature_encoding != CONTEXT_SIGNATURE_ENCODING
            || self.pictures.len() > MAX_CONTEXT_SHOT_SIGNATURES
            || self.source_picture_counts.len() > MAX_INTERVALS
            || self.source_picture_counts.len() != support.len().saturating_add(1)
            || self.signatures.byte_length()
                != (CONTEXT_SIGNATURE_HEADER_BYTES + self.pictures.len() * PICTURE_SIGNATURE_BYTES)
                    as u64
            || self.signatures.byte_length() > MAX_CONTEXT_SIGNATURE_BYTES as u64
        {
            return Err(extension_error("invalid bounded continuity evidence"));
        }
        for (index, picture) in self.pictures.iter().enumerate() {
            if matches!(picture, GenerationPictureIdentity::AuthoredBlack)
                || self.pictures[..index].contains(picture)
            {
                return Err(extension_error(
                    "signature identities must be unique decoded pictures",
                ));
            }
        }
        let intervals = support
            .iter()
            .map(|span| (&span.first, &span.last))
            .chain(std::iter::once((&terminal.picture, &terminal.picture)));
        let mut known_counts: Vec<(GenerationPictureIdentity, usize)> = Vec::new();
        let mut decoded_intervals = 0;
        for ((first, last), count) in intervals.zip(&self.source_picture_counts) {
            match (ordinal(first), ordinal(last), count) {
                (None, None, None) => {}
                (Some(a), Some(b), Some(count))
                    if same_provider(first, last) && a.max(b) < *count as u64 =>
                {
                    decoded_intervals += 1;
                    let provider = at_frame(first, SourceFrameId(0));
                    if let Some((_, known)) = known_counts.iter().find(|(key, _)| key == &provider)
                    {
                        if known != count {
                            return Err(extension_error(
                                "one provider declares contradictory source counts",
                            ));
                        }
                    } else {
                        known_counts.push((provider, *count));
                    }
                }
                _ => {
                    return Err(extension_error(
                        "support differs from its physical source count",
                    ));
                }
            }
        }
        // The host charges reader opens and unique decoded signatures together.
        if decoded_intervals + self.pictures.len() > MAX_CONTEXT_SHOT_SIGNATURES {
            return Err(extension_error(
                "continuity exceeds its total picture-read budget",
            ));
        }
        Ok(())
    }

    /// Recompute every interval and structural seam from immutable signatures.
    /// This remains a rejection heuristic, not proof of perceptual continuity.
    pub fn qualify_signatures(
        &self,
        bytes: &[u8],
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Vec<ExtensionContextMeasurement>, QualificationError> {
        check_control(cancelled, deadline)?;
        self.validate_shape()?;
        if bytes.len() as u64 != self.signatures.byte_length()
            || sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                != self.signatures.sha256().as_str()
        {
            return Err(extension_error(
                "continuity signature hash or length differs",
            ));
        }
        let signatures = decode_context_signatures(bytes)
            .map_err(|error| extension_error(&error.to_string()))?;
        if signatures.len() != self.pictures.len() {
            return Err(extension_error(
                "signature rows differ from their identities",
            ));
        }
        let black = PictureSignature::from_rgba(&[0; 32 * 18 * 4], 32, 18, 128)
            .map_err(|error| extension_error(&error.to_string()))?;
        let GenerationInputs::Extension {
            support, terminal, ..
        } = &self.binding.inputs
        else {
            unreachable!("validated extension binding")
        };
        let mut used = vec![false; self.pictures.len()];
        let mut lookup =
            |identity: &GenerationPictureIdentity| -> Result<PictureSignature, QualificationError> {
                let index = self
                    .pictures
                    .iter()
                    .position(|picture| picture == identity)
                    .ok_or_else(|| {
                        extension_error("continuity is missing a required physical picture")
                    })?;
                used[index] = true;
                Ok(signatures[index].clone())
            };
        let intervals = support
            .iter()
            .map(|span| (&span.first, &span.last))
            .chain(std::iter::once((&terminal.picture, &terminal.picture)));
        let mut previous: Option<(&GenerationPictureIdentity, PictureSignature)> = None;
        let mut measurements = Vec::new();
        for ((first, last), count) in intervals.zip(&self.source_picture_counts) {
            check_control(cancelled, deadline)?;
            let (first_signature, last_signature) = if let Some(count) = count {
                let a = usize::try_from(ordinal(first).expect("validated decoded provider"))
                    .map_err(|_| extension_error("source ordinal overflows"))?;
                let b = usize::try_from(ordinal(last).expect("validated decoded provider"))
                    .map_err(|_| extension_error("source ordinal overflows"))?;
                let requested = a.min(b)..=a.max(b);
                let window = context_shot_window(requested.clone(), *count)
                    .map_err(|error| extension_error(&error.to_string()))?;
                let rows = window
                    .clone()
                    .map(|ordinal| lookup(&at_frame(first, SourceFrameId(ordinal as u64))))
                    .collect::<Result<Vec<_>, _>>()?;
                let qualification = qualify_context(&rows, window.start, *count, requested)
                    .map_err(|error| extension_error(&error.to_string()))?;
                if qualification.transition.is_some() {
                    return Err(extension_error(
                        "extension context crosses a detected picture transition",
                    ));
                }
                let endpoints = (
                    rows[a - window.start].clone(),
                    rows[b - window.start].clone(),
                );
                measurements.push(ExtensionContextMeasurement {
                    source: at_frame(first, SourceFrameId(a.min(b) as u64)),
                    qualification,
                });
                endpoints
            } else {
                (black.clone(), black.clone())
            };
            if let Some((before, signature)) = previous {
                check_seam_identity(before, first)?;
                if context_seam_change(&signature, &first_signature).is_some() {
                    return Err(extension_error(
                        "extension context crosses an abrupt picture seam",
                    ));
                }
            }
            previous = Some((last, last_signature));
        }
        if used.iter().any(|used| !used) {
            return Err(extension_error("continuity retains an unrelated signature"));
        }
        check_control(cancelled, deadline)?;
        Ok(measurements)
    }
}

fn ordinal(picture: &GenerationPictureIdentity) -> Option<u64> {
    match picture {
        GenerationPictureIdentity::Original { frame, .. }
        | GenerationPictureIdentity::Generated { frame, .. } => Some(frame.0),
        GenerationPictureIdentity::AuthoredBlack => None,
    }
}

fn at_frame(
    picture: &GenerationPictureIdentity,
    frame: SourceFrameId,
) -> GenerationPictureIdentity {
    let mut result = picture.clone();
    match &mut result {
        GenerationPictureIdentity::Original { frame: value, .. }
        | GenerationPictureIdentity::Generated { frame: value, .. } => *value = frame,
        GenerationPictureIdentity::AuthoredBlack => {}
    }
    result
}

fn same_provider(a: &GenerationPictureIdentity, b: &GenerationPictureIdentity) -> bool {
    at_frame(a, SourceFrameId(0)) == at_frame(b, SourceFrameId(0))
}

fn check_seam_identity(
    a: &GenerationPictureIdentity,
    b: &GenerationPictureIdentity,
) -> Result<(), QualificationError> {
    let jump = match (a, b) {
        (
            GenerationPictureIdentity::Original {
                qualification: a,
                frame: af,
            },
            GenerationPictureIdentity::Original {
                qualification: b,
                frame: bf,
            },
        ) => a != b || af.0.abs_diff(bf.0) > 1,
        (
            GenerationPictureIdentity::Generated {
                sampled_object: a,
                frame: af,
                content_aspect: ac,
            },
            GenerationPictureIdentity::Generated {
                sampled_object: b,
                frame: bf,
                content_aspect: bc,
            },
        ) if a == b => ac != bc || af.0.abs_diff(bf.0) > 1,
        _ => false,
    };
    if jump {
        Err(extension_error(
            "extension context contains a discontinuous source-picture mapping",
        ))
    } else {
        Ok(())
    }
}

fn bounded_pictures<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<GenerationPictureIdentity>, D::Error> {
    bounded_sequence::<_, _, MAX_CONTEXT_SHOT_SIGNATURES>(deserializer)
}
fn bounded_counts<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Option<usize>>, D::Error> {
    bounded_sequence::<_, _, MAX_INTERVALS>(deserializer)
}
fn bounded_sequence<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>, const N: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Visitor<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "at most {N} continuity entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Vec<T>, A::Error> {
            let mut values = Vec::new();
            while values.len() < N {
                let Some(value) = sequence.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "continuity entry count exceeds its bound",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor::<T, N>(std::marker::PhantomData))
}
