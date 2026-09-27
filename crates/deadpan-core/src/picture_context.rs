//! Bounded, immutable spatial context retained by a Hold provider.
//!
//! Context stores the ordered canvas clips that preceded the Hold, not a
//! rendered bitmap or media handle. The Hold's own framing and live ancestors
//! continue after this context.

mod admission;
mod preflight;
pub(crate) use admission::validate_command;
pub(crate) use preflight::check as preflight;

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{DocumentError, DocumentErrorCode, FramingPose};

pub const MAX_CAPTURED_CANVASES: usize = 32;
pub const MAX_CAPTURED_SCOPES: usize = 512;
pub const MAX_CAPTURED_POSES: usize = 256;
pub const MAX_CAPTURED_FRAMING_RECORDS: usize = 100_000;
// An occurrence edit may retire its copied subtree. Bound that temporary state
// separately; no public document, transaction or serialized state admits it.
pub(crate) const MAX_ISOLATED_FRAMING_RECORDS: usize = 2 * MAX_CAPTURED_FRAMING_RECORDS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturedFit {
    Fit,
    Fill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedCanvas {
    pub width: u32,
    pub height: u32,
    pub fit: CapturedFit,
    /// Ordered static operations in this canvas. `None` is an identity clip;
    /// an empty vector has the same one-identity-clip meaning.
    #[serde(deserialize_with = "bounded_layers")]
    pub layers: Vec<Option<FramingPose>>,
}

impl CapturedCanvas {
    pub fn validate(&self) -> Result<(), DocumentError> {
        crate::basis::validate_canvas(self.width, self.height)?;
        if self.layers.len() > MAX_CAPTURED_SCOPES {
            return Err(limit("captured framing exceeds its scope limit"));
        }
        let mut poses = 0usize;
        for pose in self.layers.iter().flatten() {
            poses += 1;
            if poses > MAX_CAPTURED_POSES {
                return Err(limit("captured framing exceeds its pose limit"));
            }
            pose.validate()
                .map_err(|error| invalid(error.to_string()))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedFraming {
    pub canvases: Vec<CapturedCanvas>,
}

impl<'de> Deserialize<'de> for CapturedFraming {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(deserialize_with = "bounded_canvases")]
            canvases: Vec<CapturedCanvas>,
        }
        let wire = Wire::deserialize(decoder)?;
        Self::new(wire.canvases).map_err(de::Error::custom)
    }
}

impl CapturedFraming {
    pub fn new(canvases: Vec<CapturedCanvas>) -> Result<Self, DocumentError> {
        let value = Self { canvases };
        value.validate()?;
        Ok(value)
    }

    /// Append one previously clipped canvas stage. Identity clips compact, and
    /// same-size Fit stages can extend the last canvas without changing order.
    /// All checks happen before cloning `previous`, so failure is atomic.
    pub fn capture(
        previous: Option<&CapturedFraming>,
        canvas: CapturedCanvas,
    ) -> Result<Self, DocumentError> {
        canvas.validate()?;
        if let Some(previous) = previous {
            previous.validate()?;
        }
        let incoming_layers = compact_layers(&canvas.layers);
        let previous_canvases = previous.map_or(0, |value| value.canvases.len());
        let merge = canvas.fit == CapturedFit::Fit
            && previous.is_some_and(|value| {
                value
                    .canvases
                    .last()
                    .is_some_and(|last| last.width == canvas.width && last.height == canvas.height)
            });
        let incoming_scopes = incoming_layers.len();
        let old_scopes = previous.map_or(0, |value| {
            value.canvases.iter().map(|stage| stage.layers.len()).sum()
        });
        let previous_implicit_clip = merge
            && previous
                .and_then(|value| value.canvases.last())
                .is_some_and(|last| last.layers.is_empty())
            && incoming_layers.iter().any(Option::is_some);
        let skip_incoming_leading_clip =
            merge && incoming_layers.first().is_some_and(Option::is_none);
        let scopes = old_scopes
            .checked_add(incoming_scopes)
            .and_then(|value| value.checked_add(usize::from(previous_implicit_clip)))
            .and_then(|value| value.checked_sub(usize::from(skip_incoming_leading_clip)))
            .ok_or_else(|| limit("captured framing scope count overflow"))?;
        let poses = previous.map_or(0, |value| {
            value
                .canvases
                .iter()
                .flat_map(|stage| stage.layers.iter())
                .filter(|pose| pose.is_some())
                .count()
        }) + incoming_layers.iter().filter(|pose| pose.is_some()).count();
        let canvas_count = previous_canvases + usize::from(previous.is_none() || !merge);
        if canvas_count > MAX_CAPTURED_CANVASES
            || scopes > MAX_CAPTURED_SCOPES
            || poses > MAX_CAPTURED_POSES
        {
            return Err(limit(
                "captured framing exceeds its canvas, scope or pose limit",
            ));
        }
        let records = 1usize
            .checked_add(canvas_count)
            .and_then(|value| value.checked_add(scopes))
            .and_then(|value| value.checked_add(poses))
            .ok_or_else(|| limit("captured framing record count overflow"))?;
        if records > MAX_CAPTURED_FRAMING_RECORDS {
            return Err(limit("captured framing exceeds its aggregate record limit"));
        }

        let mut canvases = previous.map_or_else(Vec::new, |value| {
            value
                .canvases
                .iter()
                .map(|stage| CapturedCanvas {
                    width: stage.width,
                    height: stage.height,
                    fit: stage.fit,
                    layers: compact_layers(&stage.layers),
                })
                .collect()
        });
        let mut incoming = canvas;
        incoming.layers = incoming_layers;
        if merge {
            let last = canvases.last_mut().expect("merge requires previous canvas");
            if last.layers.is_empty() && incoming.layers.iter().any(Option::is_some) {
                // Empty layers are one implicit identity clip. Keep it explicit
                // before adding poses so a prior Fill crop cannot be recovered.
                last.layers.push(None);
            }
            if last.layers.last().is_some() && incoming.layers.first().is_some_and(Option::is_none)
            {
                incoming.layers.remove(0);
            }
            last.layers.extend(incoming.layers);
            last.layers = compact_layers(&last.layers);
        } else {
            canvases.push(incoming);
        }
        let captured = Self { canvases };
        captured.validate()?;
        Ok(captured)
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.canvases.is_empty() || self.canvases.len() > MAX_CAPTURED_CANVASES {
            return Err(limit("captured framing must contain 1 to 32 canvases"));
        }
        let mut scopes = 0usize;
        let mut poses = 0usize;
        for canvas in &self.canvases {
            canvas.validate()?;
            scopes = scopes
                .checked_add(canvas.layers.len())
                .ok_or_else(|| limit("captured framing scope count overflow"))?;
            poses = poses
                .checked_add(canvas.layers.iter().filter(|pose| pose.is_some()).count())
                .ok_or_else(|| limit("captured framing pose count overflow"))?;
        }
        if scopes > MAX_CAPTURED_SCOPES || poses > MAX_CAPTURED_POSES {
            return Err(limit("captured framing exceeds its scope or pose limit"));
        }
        if self.record_count()? > MAX_CAPTURED_FRAMING_RECORDS {
            return Err(limit("captured framing exceeds its aggregate record limit"));
        }
        Ok(())
    }

    /// One context record, then canvas stages, scopes and nonempty poses.
    pub fn record_count(&self) -> Result<usize, DocumentError> {
        self.canvases.iter().try_fold(1usize, |total, canvas| {
            total
                .checked_add(1)
                .and_then(|value| value.checked_add(canvas.layers.len()))
                .and_then(|value| {
                    value.checked_add(canvas.layers.iter().filter(|pose| pose.is_some()).count())
                })
                .ok_or_else(|| limit("captured framing record count overflow"))
        })
    }
}

fn compact_layers(layers: &[Option<FramingPose>]) -> Vec<Option<FramingPose>> {
    let mut compact = Vec::with_capacity(layers.len());
    for layer in layers {
        if layer.is_none() && compact.last().is_some_and(Option::is_none) {
            continue;
        }
        compact.push(*layer);
    }
    if compact.as_slice() == [None] {
        compact.clear();
    }
    compact
}

fn bounded_canvases<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<CapturedCanvas>, D::Error> {
    struct Canvases;
    impl<'de> de::Visitor<'de> for Canvases {
        type Value = Vec<CapturedCanvas>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 32 bounded captured canvases")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut canvases = Vec::new();
            let mut scopes = 0usize;
            let mut poses = 0usize;
            loop {
                if canvases.len() == MAX_CAPTURED_CANVASES {
                    if seq.next_element_seed(Reject)?.is_none() {
                        return Ok(canvases);
                    }
                    unreachable!("Reject seed always fails");
                }
                let Some(canvas) = seq.next_element::<CapturedCanvas>()? else {
                    return Ok(canvases);
                };
                canvas.validate().map_err(de::Error::custom)?;
                scopes += canvas.layers.len();
                poses += canvas.layers.iter().filter(|pose| pose.is_some()).count();
                if scopes > MAX_CAPTURED_SCOPES || poses > MAX_CAPTURED_POSES {
                    return Err(de::Error::custom(
                        "captured framing exceeds its scope or pose limit",
                    ));
                }
                canvases.push(canvas);
            }
        }
    }
    deserializer.deserialize_seq(Canvases)
}

struct Reject;
impl<'de> de::DeserializeSeed<'de> for Reject {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(de::Error::custom(
            "captured framing exceeds a bounded sequence",
        ))
    }
}

fn bounded_layers<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Option<FramingPose>>, D::Error> {
    bounded_vec(
        deserializer,
        MAX_CAPTURED_SCOPES,
        "at most 512 captured scopes",
    )
}

fn bounded_vec<'de, D, T>(
    deserializer: D,
    max: usize,
    expected: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T> {
        max: usize,
        expected: &'static str,
        marker: std::marker::PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.expected)
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|size| size > self.max) {
                return Err(de::Error::custom(
                    "captured framing exceeds a bounded sequence",
                ));
            }
            let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(self.max));
            loop {
                if values.len() == self.max {
                    if seq.next_element_seed(Reject)?.is_none() {
                        return Ok(values);
                    }
                    unreachable!("Reject seed always fails");
                }
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
        }
    }
    deserializer.deserialize_seq(Bounded {
        max,
        expected,
        marker: std::marker::PhantomData,
    })
}

fn invalid(message: impl Into<String>) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: impl Into<String>) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}

/// Validate caller-owned nodes without copying their context. Also used on
/// virtual post-edit node sets before patches or isolation allocate copies.
pub(crate) fn validate_nodes<'a>(
    nodes: impl Iterator<Item = &'a crate::BeatNode>,
) -> Result<usize, DocumentError> {
    validate_nodes_with_limit(nodes, MAX_CAPTURED_FRAMING_RECORDS)
}

pub(crate) fn validate_nodes_with_limit<'a>(
    nodes: impl Iterator<Item = &'a crate::BeatNode>,
    record_limit: usize,
) -> Result<usize, DocumentError> {
    let mut records = 0usize;
    for node in nodes {
        let recipe = match &node.kind {
            crate::NodeKind::Hold { recipe }
            | crate::NodeKind::Repeat {
                gap: Some(recipe), ..
            } => recipe,
            _ => continue,
        };
        if let Some(context) = &recipe.picture_context {
            context.validate()?;
            records = records
                .checked_add(context.record_count()?)
                .ok_or_else(|| limit("captured framing record count overflow"))?;
            if records > record_limit {
                return Err(limit("aggregate captured framing record limit exceeded"));
            }
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExactRatio;

    fn canvas(
        width: u32,
        height: u32,
        fit: CapturedFit,
        layers: Vec<Option<FramingPose>>,
    ) -> CapturedCanvas {
        CapturedCanvas {
            width,
            height,
            fit,
            layers,
        }
    }

    fn pose(x: i64) -> FramingPose {
        FramingPose::new(
            ExactRatio::new(i128::from(x), 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::ONE,
        )
        .unwrap()
    }

    #[test]
    fn capture_compacts_identity_clips_and_merges_same_size_fit_without_quantizing() {
        let first =
            CapturedFraming::capture(None, canvas(1920, 1080, CapturedFit::Fit, vec![None, None]))
                .unwrap();
        assert_eq!(first.canvases.len(), 1);
        assert!(first.canvases[0].layers.is_empty());
        let exact = pose(1);
        let second = CapturedFraming::capture(
            Some(&first),
            canvas(1920, 1080, CapturedFit::Fit, vec![Some(exact), None, None]),
        )
        .unwrap();
        assert_eq!(second.canvases.len(), 1);
        assert_eq!(second.canvases[0].layers, vec![None, Some(exact), None]);
        assert_eq!(second.canvases[0].layers[1], Some(exact));
        let stable =
            CapturedFraming::capture(Some(&second), canvas(1920, 1080, CapturedFit::Fit, vec![]))
                .unwrap();
        assert_eq!(stable, second);
        let explicit_identity = FramingPose::identity();
        let authored = CapturedFraming::capture(
            Some(&second),
            canvas(1920, 1080, CapturedFit::Fit, vec![Some(explicit_identity)]),
        )
        .unwrap();
        assert_eq!(
            authored.canvases[0].layers.last(),
            Some(&Some(explicit_identity))
        );
    }

    #[test]
    fn capture_is_atomic_and_keeps_nontrivial_clip_order() {
        let first = CapturedFraming::capture(
            None,
            canvas(1920, 1080, CapturedFit::Fill, vec![Some(pose(1))]),
        )
        .unwrap();
        let invalid = canvas(1, 1080, CapturedFit::Fit, vec![Some(pose(2))]);
        assert!(CapturedFraming::capture(Some(&first), invalid).is_err());
        assert_eq!(first.canvases.len(), 1);
        let second = CapturedFraming::capture(
            Some(&first),
            canvas(1000, 700, CapturedFit::Fill, vec![Some(pose(2))]),
        )
        .unwrap();
        assert_eq!(second.canvases.len(), 2);
        assert_eq!(second.canvases[0].layers, vec![Some(pose(1))]);
        assert_eq!(second.canvases[1].layers, vec![Some(pose(2))]);
    }

    #[test]
    fn merge_retains_implicit_fill_clip_before_new_pose_and_reuses_prior_clip() {
        let fill =
            CapturedFraming::capture(None, canvas(1920, 1080, CapturedFit::Fill, vec![])).unwrap();
        let zoom = pose(1);
        let merged = CapturedFraming::capture(
            Some(&fill),
            canvas(1920, 1080, CapturedFit::Fit, vec![Some(zoom)]),
        )
        .unwrap();
        assert_eq!(merged.canvases.len(), 1);
        assert_eq!(merged.canvases[0].layers, vec![None, Some(zoom)]);
        let framed =
            CapturedFraming::capture(None, canvas(1920, 1080, CapturedFit::Fit, vec![Some(zoom)]))
                .unwrap();
        assert_eq!(
            CapturedFraming::capture(Some(&framed), canvas(1920, 1080, CapturedFit::Fit, vec![]))
                .unwrap(),
            framed
        );
    }

    #[test]
    fn bounded_deserialization_rejects_per_context_overflows() {
        let mut wire =
            serde_json::json!({"canvases":[{"width":1920,"height":1080,"fit":"fit","layers":[]}]});
        wire["canvases"] =
            serde_json::json!(vec![wire["canvases"][0].clone(); MAX_CAPTURED_CANVASES + 1]);
        assert!(serde_json::from_value::<CapturedFraming>(wire).is_err());
        let wire = serde_json::json!({"canvases":[{"width":1920,"height":1080,"fit":"fit","layers":vec![serde_json::Value::Null; MAX_CAPTURED_SCOPES + 1]}]});
        assert!(serde_json::from_value::<CapturedFraming>(wire).is_err());
    }
}
