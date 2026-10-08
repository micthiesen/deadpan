//! Bound the complete command before internally tagged serde enums buffer it.
//! Duplicate keys, excessive depth, bytes and scalar counts fail before typed
//! leaf construction. The same serialized-byte budget applies to direct values.
use super::{MAX_COMPOUND_WIRE_BYTES, limit};
use crate::EditError;
use serde::{
    Deserializer, Serialize,
    de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{fmt, io};

struct Count {
    bytes: usize,
    maximum: usize,
}
impl io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= self.maximum)
            .ok_or_else(|| io::Error::other("resolved transaction byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn size(value: &impl Serialize, maximum: usize) -> Result<usize, EditError> {
    let mut counter = Count { bytes: 0, maximum };
    serde_json::to_writer(&mut counter, value)
        .map_err(|_| limit("resolved transaction serialized byte limit"))?;
    Ok(counter.bytes)
}
struct Budget {
    bytes: usize,
    values: usize,
    maximum: usize,
}
impl Budget {
    fn charge<E: Error>(&mut self, bytes: usize) -> Result<(), E> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= self.maximum)
            .ok_or_else(|| E::custom("resolved transaction byte limit"))?;
        self.values = self
            .values
            .checked_add(1)
            .filter(|n| *n <= 2_000_000)
            .ok_or_else(|| E::custom("resolved transaction value limit"))?;
        Ok(())
    }
}
struct Read<'a> {
    budget: &'a mut Budget,
    depth: usize,
    collection_limit: Option<usize>,
    refuse: Option<usize>,
    transaction: bool,
}
impl<'de> DeserializeSeed<'de> for Read<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        if let Some(maximum) = self.refuse {
            return Err(D::Error::custom(if maximum == crate::MAX_DOCUMENT_NODES {
                "boundary replacements exceed the document node limit"
            } else {
                "resolved transaction exceeds 1024 expanded steps"
            }));
        }
        if self.depth > 64 {
            return Err(D::Error::custom("resolved transaction depth limit"));
        }
        self.budget.charge(1)?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Read<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded command JSON")
    }
    fn visit_bool<E: Error>(self, value: bool) -> Result<Value, E> {
        self.budget.charge(5)?;
        Ok(value.into())
    }
    fn visit_i64<E: Error>(self, value: i64) -> Result<Value, E> {
        self.budget.charge(21)?;
        Ok(value.into())
    }
    fn visit_u64<E: Error>(self, value: u64) -> Result<Value, E> {
        self.budget.charge(21)?;
        Ok(value.into())
    }
    fn visit_f64<E: Error>(self, value: f64) -> Result<Value, E> {
        self.budget.charge(32)?;
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite command scalar"))
    }
    fn visit_unit<E: Error>(self) -> Result<Value, E> {
        self.budget.charge(4)?;
        Ok(Value::Null)
    }
    fn visit_none<E: Error>(self) -> Result<Value, E> {
        self.visit_unit()
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<Value, E> {
        self.budget.charge(value.len())?;
        Ok(Value::String(value.to_owned()))
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
            collection_limit: None,
            refuse: self.collection_limit.filter(|limit| values.len() == *limit),
            transaction: false,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = input.next_key_seed(Key {
            budget: self.budget,
            refuse: self
                .collection_limit
                .is_some_and(|limit| values.len() == limit),
        })? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate command JSON key"));
            }
            let value = input.next_value_seed(Read {
                budget: self.budget,
                depth: self.depth + 1,
                collection_limit: match (self.transaction, key.as_str()) {
                    (true, "steps") => Some(super::MAX_COMPOUND_STEPS),
                    (true, "inputs") => Some(27),
                    (_, "instructions") => Some(crate::MAX_SEMANTIC_PROGRAM_INSTRUCTIONS),
                    (_, "replacements") => Some(crate::MAX_DOCUMENT_NODES),
                    _ => None,
                },
                refuse: None,
                transaction: key == "transaction",
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
struct Key<'a> {
    budget: &'a mut Budget,
    refuse: bool,
}
impl<'de> DeserializeSeed<'de> for Key<'_> {
    type Value = String;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        if self.refuse {
            return Err(D::Error::custom(
                "resolved transaction exceeds 27 register inputs",
            ));
        }
        deserializer.deserialize_string(self)
    }
}
impl Visitor<'_> for Key<'_> {
    type Value = String;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded command field")
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<String, E> {
        self.budget.charge(value.len())?;
        Ok(value.to_owned())
    }
    fn visit_string<E: Error>(self, value: String) -> Result<String, E> {
        self.budget.charge(value.len())?;
        Ok(value)
    }
}
pub(crate) fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
    read_root(deserializer, false, MAX_COMPOUND_WIRE_BYTES)
}
pub(crate) fn read_bounded<'de, D: Deserializer<'de>>(
    deserializer: D,
    maximum: usize,
) -> Result<Value, D::Error> {
    read_root(deserializer, false, maximum)
}
pub(super) fn read_transaction<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Value, D::Error> {
    read_root(deserializer, true, MAX_COMPOUND_WIRE_BYTES)
}
fn read_root<'de, D: Deserializer<'de>>(
    deserializer: D,
    transaction: bool,
    maximum: usize,
) -> Result<Value, D::Error> {
    let mut budget = Budget {
        bytes: 0,
        values: 0,
        maximum,
    };
    let value = Read {
        budget: &mut budget,
        depth: 0,
        collection_limit: None,
        refuse: None,
        transaction,
    }
    .deserialize(deserializer)?;
    size(&value, maximum).map_err(D::Error::custom)?;
    Ok(value)
}
