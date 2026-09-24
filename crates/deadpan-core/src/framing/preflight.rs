//! Count the flat framing grammar before typed node/segment allocation. The
//! authoritative parser still rejects duplicate, missing and unknown fields.
//! Like audio_reference/preflight, this retains counters, never a JSON tree.

use std::fmt;

use serde::de::{DeserializeSeed, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};

use crate::{DocumentError, MAX_DOCUMENT_NODES};

use super::{MAX_FRAMING_RECORDS, MAX_FRAMING_SEGMENTS};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Root,
    Nodes,
    Node,
    Framing,
    Value,
    Envelope,
    Segments,
    Segment,
    Curve,
    Other,
}

#[derive(Default)]
struct Counts {
    records: usize,
    nodes: usize,
}

pub(crate) fn check(json: &str) -> Result<(), DocumentError> {
    // Plain old documents have no framing. Escaped keys must take the scanner
    // path, so a Unicode escape cannot hide a collection from admission.
    if !json.contains("framing") && !json.contains("\\u") {
        return Ok(());
    }
    let mut counts = Counts::default();
    let mut decoder = serde_json::Deserializer::from_str(json);
    Scan {
        role: Role::Root,
        counts: &mut counts,
        charge: false,
    }
    .deserialize(&mut decoder)
    .map_err(DocumentError::json)?;
    decoder.end().map_err(DocumentError::json)
}

struct Scan<'a> {
    role: Role,
    counts: &'a mut Counts,
    charge: bool,
}

impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        if self.charge {
            if self.counts.records == MAX_FRAMING_RECORDS {
                return Err(D::Error::custom("aggregate framing record limit exceeded"));
            }
            self.counts.records += 1;
        }
        if self.role == Role::Other {
            IgnoredAny::deserialize(decoder).map(|_| ())
        } else {
            decoder.deserialize_any(self)
        }
    }
}

impl<'de> Visitor<'de> for Scan<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded framing document records")
    }
    fn visit_unit<E: Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            if self.role == Role::Nodes {
                if self.counts.nodes == MAX_DOCUMENT_NODES {
                    return Err(A::Error::custom("document node limit exceeded"));
                }
                self.counts.nodes += 1;
            }
            let (role, charge) = match (self.role, key.as_ref()) {
                (Role::Root, "nodes") => (Role::Nodes, false),
                (Role::Nodes, _) => (Role::Node, false),
                (Role::Node, "framing") => (Role::Framing, false),
                (Role::Framing, "value") => (Role::Value, false),
                (Role::Value, "pose") => (Role::Other, true),
                (Role::Value, "envelope") => (Role::Envelope, false),
                (Role::Envelope, "initial") => (Role::Other, true),
                (Role::Envelope, "segments") => (Role::Segments, false),
                (Role::Segment, "curve") => (Role::Curve, false),
                (Role::Curve, "control1" | "control2") => (Role::Other, true),
                _ => (Role::Other, false),
            };
            map.next_value_seed(Scan {
                role,
                counts: self.counts,
                charge,
            })?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        if self.role != Role::Segments {
            return Err(A::Error::custom("unexpected framing collection"));
        }
        let mut count = 0usize;
        loop {
            let next = if count == MAX_FRAMING_SEGMENTS {
                // A seed is called only if an element exists, so over-limit
                // data is rejected before it can materialize a typed value.
                sequence.next_element_seed(Reject)?
            } else {
                sequence.next_element_seed(Scan {
                    role: Role::Segment,
                    counts: self.counts,
                    charge: true,
                })?
            };
            if next.is_none() {
                return Ok(());
            }
            count += 1;
        }
    }
}

struct Reject;
impl<'de> DeserializeSeed<'de> for Reject {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(D::Error::custom("framing segment limit exceeded"))
    }
}

use serde::Deserialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_rejects_the_extra_segment_before_reading_its_malformed_body() {
        let segment = r#"{"curve":{"type":"linear"}}"#;
        let input = format!(
            "{{\"nodes\":{{\"n\":{{\"fr\\u0061ming\":{{\"value\":{{\"envelope\":{{\"initial\":{{}},\"segments\":[{},{{",
            vec![segment; MAX_FRAMING_SEGMENTS].join(",")
        );
        let error = check(&input).unwrap_err();
        assert!(
            error.to_string().contains("framing segment limit exceeded"),
            "{error}"
        );
    }

    #[test]
    fn scanner_counts_aggregate_controls_before_materializing_node_values() {
        let segment = r#"{"curve":{"type":"cubic","control1":{},"control2":{}}}"#;
        let envelope = format!(
            "{{\"framing\":{{\"value\":{{\"envelope\":{{\"initial\":{{}},\"segments\":[{}]}}}}}}}}",
            vec![segment; MAX_FRAMING_SEGMENTS].join(",")
        );
        let nodes = (0..519)
            .map(|i| format!("\"n{i}\":{envelope}"))
            .collect::<Vec<_>>()
            .join(",");
        let error = check(&format!("{{\"nodes\":{{{nodes}}}}}")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("aggregate framing record limit exceeded"),
            "{error}"
        );
    }
}
