//! Bounded JSON bridge for serde's internally tagged Command buffering. RawValue
//! cannot be deserialized from that buffer. Preserve exact scalar vocabulary and
//! reject duplicate keys before the closed typed slice/binding readers run.

use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserializer, Serialize};
use serde_json::Value;
use std::fmt;

struct Budget {
    values: usize,
    bytes: usize,
}
impl Budget {
    fn charge<E: Error>(&mut self, bytes: usize) -> Result<(), E> {
        self.values = self
            .values
            .checked_add(1)
            .filter(|value| *value <= 2_000_000)
            .ok_or_else(|| E::custom("slice JSON value limit"))?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|value| *value <= crate::MAX_DOCUMENT_JSON_BYTES)
            .ok_or_else(|| E::custom("slice JSON byte limit"))?;
        Ok(())
    }
}
struct Read<'a> {
    budget: &'a mut Budget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Read<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        if self.depth > 64 {
            return Err(D::Error::custom("slice JSON depth limit"));
        }
        self.budget.charge(1)?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Read<'_> {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded slice JSON")
    }
    fn visit_bool<E: Error>(self, value: bool) -> Result<Value, E> {
        self.budget.charge(5)?;
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: Error>(self, value: i64) -> Result<Value, E> {
        self.budget.charge(21)?;
        Ok(value.into())
    }
    fn visit_u64<E: Error>(self, value: u64) -> Result<Value, E> {
        self.budget.charge(21)?;
        Ok(value.into())
    }
    fn visit_f64<E: Error>(self, _value: f64) -> Result<Value, E> {
        Err(E::custom(
            "slice clocks and parameters require exact integer/string scalars",
        ))
    }
    fn visit_unit<E: Error>(self) -> Result<Value, E> {
        self.budget.charge(4)?;
        Ok(Value::Null)
    }
    fn visit_none<E: Error>(self) -> Result<Value, E> {
        self.visit_unit()
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<Value, E> {
        self.visit_string(value.to_owned())
    }
    fn visit_string<E: Error>(self, value: String) -> Result<Value, E> {
        self.budget.charge(value.len())?;
        Ok(Value::String(value))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = input.next_element_seed(Read {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = input.next_key::<String>()? {
            self.budget.charge(key.len())?;
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate slice JSON key"));
            }
            let value = input.next_value_seed(Read {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

pub(super) fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let mut budget = Budget {
        values: 0,
        bytes: 0,
    };
    let value = Read {
        budget: &mut budget,
        depth: 0,
    }
    .deserialize(deserializer)?;
    let mut output = super::SliceJson(Vec::new(), false);
    value
        .serialize(&mut serde_json::Serializer::new(&mut output))
        .map_err(D::Error::custom)?;
    String::from_utf8(output.0).map_err(D::Error::custom)
}
