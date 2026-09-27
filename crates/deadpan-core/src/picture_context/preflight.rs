//! Admit contexts one at a time at actual recipe paths before allocating the
//! whole document. A node or asset ID named `picture_context` is ordinary data.

use std::fmt;

use serde::{
    Deserialize,
    de::{DeserializeSeed, Error, IgnoredAny, MapAccess, Visitor},
};
use serde_json::value::RawValue;

use super::{CapturedFraming, MAX_CAPTURED_FRAMING_RECORDS};
use crate::{DocumentError, MAX_DOCUMENT_JSON_BYTES};

#[derive(Clone, Copy)]
enum Role {
    Root,
    Nodes,
    Node,
    Kind,
    Recipe,
    Other,
}

pub(crate) fn check(json: &str) -> Result<(), DocumentError> {
    if json.len() > MAX_DOCUMENT_JSON_BYTES {
        return Err(super::limit("document JSON exceeds 64 MiB"));
    }
    if !json.contains("picture_context") && !json.contains("\\u") {
        return Ok(());
    }
    let mut records = 0usize;
    let mut decoder = serde_json::Deserializer::from_str(json);
    Scan {
        role: Role::Root,
        records: &mut records,
    }
    .deserialize(&mut decoder)
    .map_err(DocumentError::json)?;
    decoder.end().map_err(DocumentError::json)
}

struct Scan<'a> {
    role: Role,
    records: &'a mut usize,
}

impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        if matches!(self.role, Role::Other) {
            IgnoredAny::deserialize(decoder).map(|_| ())
        } else {
            decoder.deserialize_any(self)
        }
    }
}

impl<'de> Visitor<'de> for Scan<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded authored document")
    }
    fn visit_unit<E: Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            if matches!(self.role, Role::Recipe) && key == "picture_context" {
                let raw = map.next_value::<&RawValue>()?;
                let context: Option<CapturedFraming> =
                    serde_json::from_str(raw.get()).map_err(A::Error::custom)?;
                if let Some(context) = context {
                    *self.records = self
                        .records
                        .checked_add(context.record_count().map_err(A::Error::custom)?)
                        .ok_or_else(|| {
                            A::Error::custom("captured framing record count overflow")
                        })?;
                    if *self.records > MAX_CAPTURED_FRAMING_RECORDS {
                        return Err(A::Error::custom(
                            "aggregate captured framing record limit exceeded",
                        ));
                    }
                }
            } else {
                let role = match (self.role, key.as_ref()) {
                    (Role::Root, "nodes") => Role::Nodes,
                    (Role::Nodes, _) => Role::Node,
                    (Role::Node, "kind") => Role::Kind,
                    (Role::Kind, "recipe" | "gap") => Role::Recipe,
                    _ => Role::Other,
                };
                map.next_value_seed(Scan {
                    role,
                    records: self.records,
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_bound_includes_gaps_and_decodes_escaped_recipe_keys() {
        let context = r#"{"canvases":[{"width":1920,"height":1080,"fit":"fit","layers":[]}]}"#;
        let count = MAX_CAPTURED_FRAMING_RECORDS / 2 + 1;
        let nodes = (0..count)
            .map(|i| format!(r#""n{i}":{{"kind":{{"gap":{{"picture_context":{context}}}}}}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(r#"{{"nodes":{{{nodes}}}}}"#);
        assert!(
            check(&json)
                .unwrap_err()
                .to_string()
                .contains("aggregate captured framing record limit")
        );
        assert!(check(&json.replace("picture_context", "picture_con\\u0074ext")).is_err());
    }

    #[test]
    fn only_actual_recipe_paths_are_inspected() {
        check(r#"{"nodes":{"picture_context":{"kind":{"type":"sequence","children":[]}}},"assets":{"picture_context":{}}}"#).unwrap();
        check(r#"{"nodes":{"picture_con\u0074ext":{"kind":{"type":"sequence","children":[]}}}}"#)
            .unwrap();
    }
}
