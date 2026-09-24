//! Scan collection sizes before serde materializes the frozen typed tree. The
//! scanner retains counters, one record key, and bounded lineage alias keys,
//! never a document-wide JSON value.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};

use super::{DocumentError, MAX_DOCUMENT_NODES, MAX_FROZEN_AUDIO_RUNS, limit};

// Valid frozen grammar is flat and needs fewer than sixteen JSON levels. This
// also bounds the scanner stack for unknown values rejected by the typed pass.
const MAX_JSON_DEPTH: usize = 64;

#[derive(Clone, Copy)]
enum Role {
    Root,
    Nodes,
    Node,
    Kind,
    Children,
    Iterations,
    Runs,
    Overrides,
    Lineages,
    Lineage,
    OverrideEntries,
    OverrideEntry,
    Iteration,
    Run,
    Placement,
    Ratio,
    Rate,
    Edges,
    Audibility,
    Mapping,
    String,
    Scalar,
    Unknown,
}

#[derive(Clone, Copy)]
enum Charge {
    None,
    Node,
    Edge,
    Run,
    OverrideOwner,
    Lineage,
}

#[derive(Default)]
struct Counts {
    nodes: usize,
    edges: usize,
    runs: usize,
    owners: usize,
    lineages: usize,
    exceeded: Option<&'static str>,
    invalid_shape: bool,
}

impl Counts {
    fn charge<E: Error>(&mut self, charge: Charge) -> Result<(), E> {
        let (count, cap, message) = match charge {
            Charge::None => return Ok(()),
            Charge::Node => (
                &mut self.nodes,
                MAX_DOCUMENT_NODES,
                "frozen node limit exceeded",
            ),
            Charge::Edge => (
                &mut self.edges,
                MAX_DOCUMENT_NODES,
                "frozen structural edge limit exceeded",
            ),
            Charge::Run => (
                &mut self.runs,
                MAX_FROZEN_AUDIO_RUNS,
                "frozen compact run limit exceeded",
            ),
            Charge::OverrideOwner => (
                &mut self.owners,
                MAX_DOCUMENT_NODES,
                "frozen override owner limit exceeded",
            ),
            Charge::Lineage => (
                &mut self.lineages,
                MAX_DOCUMENT_NODES,
                "frozen audio lineage limit exceeded",
            ),
        };
        if *count == cap {
            self.exceeded = Some(message);
            return Err(E::custom(message));
        }
        *count += 1;
        Ok(())
    }

    fn fail<E: Error>(&mut self, message: &'static str) -> E {
        self.exceeded = Some(message);
        E::custom(message)
    }
}

pub(super) fn check(json: &str) -> Result<(), DocumentError> {
    complexity(json).map(|_| ())
}

/// Shared aggregate admission counts for a collection of retained layouts.
pub(super) fn complexity(json: &str) -> Result<(usize, usize, usize), DocumentError> {
    let mut counts = Counts::default();
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let result = Scan {
        counts: &mut counts,
        role: Role::Root,
        depth: 0,
        charge: Charge::None,
    }
    .deserialize(&mut deserializer)
    .and_then(|()| deserializer.end());
    if let Some(message) = counts.exceeded {
        return Err(limit(message));
    }
    result.map_err(DocumentError::json)?;
    if counts.invalid_shape {
        return Err(DocumentError::new(
            super::DocumentErrorCode::InvalidJson,
            "frozen JSON has an unknown or duplicate field, or invalid container shape",
        ));
    }
    Ok((counts.nodes, counts.runs, counts.lineages))
}

struct Scan<'a> {
    counts: &'a mut Counts,
    role: Role,
    depth: usize,
    charge: Charge,
}

impl Scan<'_> {
    fn scalar(self) {
        if !matches!(self.role, Role::Scalar | Role::Unknown) {
            self.counts.invalid_shape = true;
        }
    }
}

impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        // SeqAccess invokes this only when a next element exists. Charge it
        // before parsing, so even a malformed over-budget value is never read.
        self.counts.charge(self.charge)?;
        if self.depth > MAX_JSON_DEPTH {
            return Err(self.counts.fail("frozen JSON nesting limit exceeded"));
        }
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Scan<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded frozen audio JSON")
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<(), E> {
        self.scalar();
        Ok(())
    }
    fn visit_i64<E: Error>(self, _: i64) -> Result<(), E> {
        self.scalar();
        Ok(())
    }
    fn visit_u64<E: Error>(self, _: u64) -> Result<(), E> {
        self.scalar();
        Ok(())
    }
    fn visit_f64<E: Error>(self, _: f64) -> Result<(), E> {
        self.scalar();
        Ok(())
    }
    fn visit_str<E: Error>(self, _: &str) -> Result<(), E> {
        if !matches!(self.role, Role::String) {
            self.scalar();
        }
        Ok(())
    }
    fn visit_borrowed_str<E: Error>(self, _: &'de str) -> Result<(), E> {
        self.visit_str("")
    }
    fn visit_string<E: Error>(self, _: String) -> Result<(), E> {
        self.visit_str("")
    }
    fn visit_unit<E: Error>(self) -> Result<(), E> {
        if !matches!(self.role, Role::Placement) {
            self.scalar();
        }
        Ok(())
    }
    fn visit_none<E: Error>(self) -> Result<(), E> {
        if !matches!(self.role, Role::Placement) {
            self.scalar();
        }
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        if matches!(
            self.role,
            Role::String | Role::Scalar | Role::Children | Role::Runs | Role::OverrideEntries
        ) {
            self.counts.invalid_shape = true;
        }
        let mut length = 0usize;
        let mut seen_fields = 0u64;
        let mut lineage_keys = BTreeSet::new();
        while let Some(key) = map.next_key_seed(Key)? {
            if length == MAX_DOCUMENT_NODES {
                return Err(self.counts.fail("frozen JSON object member limit exceeded"));
            }
            length += 1;
            // Closed records have a fixed vocabulary. Reject duplicate bodies
            // before the internally tagged typed parser can buffer them. Alias
            // maps remain bounded by their charged entries and their typed
            // duplicate-ID checks; they are not structural record field names.
            if matches!(self.role, Role::Lineages) && !lineage_keys.insert(key.clone()) {
                return Err(A::Error::custom("duplicate frozen audio lineage alias"));
            }
            if !matches!(
                self.role,
                Role::Nodes | Role::Overrides | Role::Lineages | Role::Unknown
            ) {
                let bit = record_field_bit(&key);
                if seen_fields & bit != 0 {
                    self.counts.invalid_shape = true;
                }
                seen_fields |= bit;
            }
            let (role, charge) = match (self.role, key.as_ref()) {
                (Role::Root, "nodes") => (Role::Nodes, Charge::None),
                (Role::Root, "overrides") => (Role::Overrides, Charge::None),
                (Role::Root, "audio_lineage") => (Role::Lineages, Charge::None),
                (Role::Lineages, _) => (Role::Lineage, Charge::Lineage),
                (Role::Lineage, "allocation" | "origin") => (Role::String, Charge::None),
                (Role::Root, "root") => (Role::String, Charge::None),
                (Role::Root, "rate") => (Role::Rate, Charge::None),
                (Role::Nodes, _) => (Role::Node, Charge::Node),
                (Role::Node, "kind") => (Role::Kind, Charge::None),
                (Role::Node, "duration") => (Role::Scalar, Charge::None),
                (Role::Node, "edges") => (Role::Edges, Charge::None),
                (Role::Kind, "children") => (Role::Children, Charge::None),
                (Role::Kind, "child") => (Role::String, Charge::Edge),
                (Role::Kind, "iterations") => (Role::Iterations, Charge::None),
                (Role::Kind, "placement") => (Role::Placement, Charge::None),
                (Role::Kind, "audio" | "gap_audio") => (Role::Audibility, Charge::None),
                (Role::Kind, "mapping") => (Role::Mapping, Charge::None),
                (Role::Kind, "type" | "pitch" | "purpose") => (Role::String, Charge::None),
                (Role::Kind, "gap_duration") => (Role::Scalar, Charge::None),
                (Role::Iterations, "runs") => (Role::Runs, Charge::None),
                (Role::Overrides, _) => (Role::OverrideEntries, Charge::OverrideOwner),
                (Role::OverrideEntry, "root") => (Role::String, Charge::None),
                (Role::OverrideEntry, "iteration") => (Role::Iteration, Charge::None),
                (Role::Iteration | Role::Run, "allocation") => (Role::String, Charge::None),
                (Role::Iteration, "ordinal") => (Role::Scalar, Charge::None),
                (Role::Run, "first" | "count") => (Role::Scalar, Charge::None),
                (Role::Placement, "start" | "end") => (Role::Ratio, Charge::None),
                (Role::Ratio | Role::Rate, "numerator" | "denominator") => {
                    (Role::Scalar, Charge::None)
                }
                (Role::Mapping, "start" | "end") => (Role::Scalar, Charge::None),
                (Role::Audibility, "type") => (Role::String, Charge::None),
                (Role::Audibility, "maximum") => (Role::Scalar, Charge::None),
                (
                    Role::Edges,
                    "node_start"
                    | "node_end"
                    | "source_placement_start"
                    | "source_placement_end"
                    | "repeat_gap_start"
                    | "repeat_gap_end",
                ) => (Role::String, Charge::None),
                (Role::Unknown, _) => (Role::Unknown, Charge::None),
                _ => {
                    self.counts.invalid_shape = true;
                    (Role::Unknown, Charge::None)
                }
            };
            map.next_value_seed(Scan {
                counts: self.counts,
                role,
                depth: self.depth + 1,
                charge,
            })?;
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        let (role, charge) = match self.role {
            Role::Children => (Role::String, Charge::Edge),
            Role::OverrideEntries => (Role::OverrideEntry, Charge::Edge),
            Role::Runs => (Role::Run, Charge::Run),
            Role::Unknown => (Role::Unknown, Charge::None),
            _ => {
                self.counts.invalid_shape = true;
                (Role::Unknown, Charge::None)
            }
        };
        let mut length = 0;
        while sequence
            .next_element_seed(Element {
                scan: Scan {
                    counts: self.counts,
                    role,
                    depth: self.depth + 1,
                    charge,
                },
                length: &mut length,
            })?
            .is_some()
        {}
        Ok(())
    }
}

fn record_field_bit(key: &str) -> u64 {
    let ordinal = match key {
        "nodes" => 0,
        "overrides" => 1,
        "root" => 2,
        "rate" => 3,
        "kind" => 4,
        "duration" => 5,
        "edges" => 6,
        "children" => 7,
        "child" => 8,
        "iterations" => 9,
        "placement" => 10,
        "audio" => 11,
        "gap_audio" => 12,
        "mapping" => 13,
        "type" => 14,
        "gap_duration" => 15,
        "pitch" => 16,
        "purpose" => 17,
        "runs" => 18,
        "iteration" => 19,
        "allocation" => 20,
        "ordinal" => 21,
        "first" => 22,
        "count" => 23,
        "start" => 24,
        "end" => 25,
        "numerator" => 26,
        "denominator" => 27,
        "maximum" => 28,
        "node_start" => 29,
        "node_end" => 30,
        "source_placement_start" => 31,
        "source_placement_end" => 32,
        "repeat_gap_start" => 33,
        "repeat_gap_end" => 34,
        "audio_lineage" => 35,
        "origin" => 36,
        _ => return 0,
    };
    1 << ordinal
}

struct Element<'a, 'b> {
    scan: Scan<'a>,
    length: &'b mut usize,
}
impl<'de> DeserializeSeed<'de> for Element<'_, '_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if *self.length == MAX_DOCUMENT_NODES {
            return Err(self.scan.counts.fail("frozen JSON list limit exceeded"));
        }
        *self.length += 1;
        self.scan.deserialize(deserializer)
    }
}

struct Key;
impl<'de> DeserializeSeed<'de> for Key {
    type Value = Cow<'de, str>;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for Key {
    type Value = Cow<'de, str>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object key")
    }
    fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<Self::Value, E> {
        Ok(Cow::Borrowed(value))
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Cow::Owned(value.to_owned()))
    }
    fn visit_string<E: Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(Cow::Owned(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_nested_values_never_reach_the_materializing_typed_parser() {
        for input in [
            r#"{"nodes":{"root":{"kind":{"children":[["id","id"]]}}}}"#,
            r#"{"nodes":{"root":{"kind":{"children":[true]}}}}"#,
            r#"{"nodes":{"root":{"kind":{"iterations":{"runs":[{"allocation":["id"],"first":0,"count":1}]}}}}}"#,
            r#"{"nodes":{"root":{"kind":{"placement":{"start":{"numerator":["1"],"denominator":"1"},"end":{"numerator":"2","denominator":"1"}}}}}}"#,
            r#"{"nodes":{"root":{"kind":{"unknown":[[0,0],[0,0]]}}}}"#,
            r#"{"nodes":{"root":{"kind":{"audio":{"type":"tail","maximum":[1]}}}}}"#,
            r#"{"nodes":{"root":{"kind":{"mapping":{"start":{},"end":1}}}}}"#,
            r#"{"nodes":{"root":{"edges":{"node_start":{"x":1}}}}}"#,
            r#"{"nodes":{"root":{"kind":{"pitch":["preserve"]}}}}"#,
            r#"{"overrides":{"root":[[0,0]]}}"#,
            r#"{"overrides":{"root":[{"iteration":{"allocation":{},"ordinal":0},"root":"id"}]}}"#,
            r#"{"audio_lineage":{"root":{"allocation":[],"origin":"root"}}}"#,
            r#"{"audio_lineage":{"root":{"allocation":"old","origin":{}}}}"#,
            r#"{"audio_lineage":{"root":{"allocation":"old","origin":"root","extra":{}}}}"#,
            r#"{"audio_lineage":[]}"#,
        ] {
            serde_json::from_str::<serde::de::IgnoredAny>(input).unwrap();
            assert_eq!(
                check(input).unwrap_err().code,
                super::super::DocumentErrorCode::InvalidJson
            );
        }
    }

    #[test]
    fn duplicate_closed_record_fields_never_reach_the_materializing_typed_parser() {
        for input in [
            r#"{"root":"a","r\u006fot":"b"}"#,
            r#"{"nodes":{"a":{"kind":{},"kind":{}}}}"#,
            r#"{"nodes":{"a":{"kind":{"type":"hold","type":"source"}}}}"#,
            r#"{"nodes":{"a":{"kind":{"audio":{"type":"tail","maximum":1,"maximum":2}}}}}"#,
            r#"{"nodes":{"a":{"kind":{"iterations":{"runs":[],"runs":[]}}}}}"#,
            r#"{"nodes":{"a":{"kind":{"iterations":{"runs":[{"count":1,"count":2}]}}}}}"#,
            r#"{"nodes":{"a":{"kind":{"placement":{"start":{"numerator":"1","numerator":"2"}}}}}}"#,
            r#"{"nodes":{"a":{"edges":{"node_start":"hard","node_start":"automatic"}}}}"#,
            r#"{"overrides":{"a":[{"root":"b","root":"c"}]}}"#,
            r#"{"overrides":{"a":[{"iteration":{"allocation":"a","allocation":"b"}}]}}"#,
            r#"{"audio_lineage":{"root":{"allocation":"a","allocation":"b","origin":"root"}}}"#,
            r#"{"audio_lineage":{"root":{"allocation":"a","origin":"root","origin":"other"}}}"#,
        ] {
            serde_json::from_str::<serde::de::IgnoredAny>(input).unwrap();
            assert_eq!(
                check(input).unwrap_err().code,
                super::super::DocumentErrorCode::InvalidJson
            );
        }
        // Each repeated body is small, but a tagged enum could otherwise retain
        // all of them before discovering the duplicate field.
        let repeated = format!(
            "{{\"nodes\":{{\"a\":{{\"kind\":{{{}\"audio\":{{\"type\":\"silence\"}}}}}}}}}}",
            "\"audio\":{\"type\":\"silence\"},".repeat(25_000)
        );
        serde_json::from_str::<serde::de::IgnoredAny>(&repeated).unwrap();
        assert_eq!(
            check(&repeated).unwrap_err().code,
            super::super::DocumentErrorCode::InvalidJson
        );
    }
}
