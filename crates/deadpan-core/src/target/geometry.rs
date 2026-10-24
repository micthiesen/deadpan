//! Closed, bounded normalized point/rectangle wire vocabulary.
use super::*;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;

/// A normalized source point, with no implied area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetPoint {
    pub center: [u32; 2],
}

impl TargetPoint {
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.center.iter().all(|v| *v <= TARGET_UNITS) {
            Ok(())
        } else {
            Err(invalid("a target point has its center inside the picture"))
        }
    }

    pub fn center_ratio(&self) -> [ExactRatio; 2] {
        self.center.map(|value| {
            ExactRatio::new(i128::from(value), i128::from(TARGET_UNITS))
                .expect("constant positive denominator")
        })
    }
}

/// Target geometry. Rectangle bytes retain the original center/size object;
/// a point has only center, never an invented zero or tiny rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetGeometry {
    Region(TargetRegion),
    Point(TargetPoint),
}

impl TargetGeometry {
    pub const fn center(&self) -> [u32; 2] {
        match self {
            Self::Region(v) => v.center,
            Self::Point(v) => v.center,
        }
    }
    pub fn center_ratio(&self) -> [ExactRatio; 2] {
        match self {
            Self::Region(v) => v.center_ratio(),
            Self::Point(v) => v.center_ratio(),
        }
    }
    pub const fn as_region(&self) -> Option<&TargetRegion> {
        match self {
            Self::Region(v) => Some(v),
            Self::Point(_) => None,
        }
    }
    pub const fn as_point(&self) -> Option<&TargetPoint> {
        match self {
            Self::Point(v) => Some(v),
            Self::Region(_) => None,
        }
    }
    pub const fn is_point(&self) -> bool {
        matches!(self, Self::Point(_))
    }
    pub fn validate(&self) -> Result<(), DocumentError> {
        match self {
            Self::Region(v) => v.validate(),
            Self::Point(v) => v.validate(),
        }
    }
}
impl From<TargetRegion> for TargetGeometry {
    fn from(value: TargetRegion) -> Self {
        Self::Region(value)
    }
}
impl From<TargetPoint> for TargetGeometry {
    fn from(value: TargetPoint) -> Self {
        Self::Point(value)
    }
}
impl Serialize for TargetGeometry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(if self.is_point() { 1 } else { 2 }))?;
        map.serialize_entry("center", &self.center())?;
        if let Self::Region(region) = self {
            map.serialize_entry("size", &region.size)?;
        }
        map.end()
    }
}

struct Pair([u32; 2]);
impl<'de> Deserialize<'de> for Pair {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PairVisitor;
        impl<'de> Visitor<'de> for PairVisitor {
            type Value = Pair;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("exactly two unsigned coordinates")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Pair, A::Error> {
                let x = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let y = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                // deserialize_tuple's closing delimiter rejects an extra element
                // before traversing it, rather than buffering arbitrary content.
                Ok(Pair([x, y]))
            }
        }
        deserializer.deserialize_tuple(2, PairVisitor)
    }
}
impl<'de> Deserialize<'de> for TargetGeometry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Center,
            Size,
        }
        struct GeometryVisitor;
        impl<'de> Visitor<'de> for GeometryVisitor {
            type Value = TargetGeometry;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a target center with an optional positive rectangle size")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let (mut center, mut size) = (None, None);
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Center => {
                            if center.is_some() {
                                return Err(de::Error::duplicate_field("center"));
                            }
                            center = Some(map.next_value::<Pair>()?.0);
                        }
                        Field::Size => {
                            if size.is_some() {
                                return Err(de::Error::duplicate_field("size"));
                            }
                            size = Some(map.next_value::<Pair>()?.0);
                        }
                    }
                }
                let center = center.ok_or_else(|| de::Error::missing_field("center"))?;
                let value = match size {
                    Some(size) => TargetGeometry::Region(TargetRegion { center, size }),
                    None => TargetGeometry::Point(TargetPoint { center }),
                };
                value.validate().map_err(de::Error::custom)?;
                Ok(value)
            }
        }
        deserializer.deserialize_map(GeometryVisitor)
    }
}
impl<'de> Deserialize<'de> for TargetPoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match TargetGeometry::deserialize(deserializer)? {
            TargetGeometry::Point(point) => Ok(point),
            TargetGeometry::Region(_) => Err(de::Error::custom("a target point has no size")),
        }
    }
}
