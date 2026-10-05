//! Deterministic byte and JSON mutators. No coverage instrumentation: the
//! runner keeps inputs that reach a new verdict class instead.

use serde_json::{Map, Number, Value};

/// SplitMix64: small, seedable and stable across platforms and releases.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound`; `bound` must be positive.
    pub fn below(&mut self, bound: usize) -> usize {
        debug_assert!(bound > 0);
        (self.next_u64() % bound.max(1) as u64) as usize
    }

    pub fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        self.next_u64() % denominator < numerator
    }

    pub fn byte(&mut self) -> u8 {
        self.next_u64().to_le_bytes()[0]
    }
}

/// Stable 64-bit FNV-1a, used for seeds and artifact names.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

const INTERESTING: [u64; 22] = [
    0,
    1,
    2,
    7,
    8,
    0x10,
    0x7f,
    0x80,
    0xff,
    0x100,
    0x7fff,
    0x8000,
    0xffff,
    0x1_0000,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_fffe,
    0xffff_ffff,
    0x1_0000_0000,
    0x7fff_ffff_ffff_ffff,
    0x8000_0000_0000_0000,
    u64::MAX,
];

fn interesting(rng: &mut Rng, length_hint: usize) -> u64 {
    match rng.below(6) {
        0 => length_hint as u64,
        1 => (length_hint as u64)
            .wrapping_add(rng.below(16) as u64)
            .wrapping_sub(8),
        _ => INTERESTING[rng.below(INTERESTING.len())],
    }
}

/// Applies one to eight stacked byte mutations, bounded by `max_len`.
/// `other` supplies splice material from another corpus entry.
pub fn mutate_bytes(rng: &mut Rng, input: &mut Vec<u8>, other: &[u8], max_len: usize) {
    let bound = if rng.chance(1, 4) { 16 } else { 4 };
    let rounds = 1 + rng.below(bound);
    for _ in 0..rounds {
        mutate_once(rng, input, other, max_len);
    }
    input.truncate(max_len);
}

/// A position biased toward the start, where headers live, half the time.
fn position(rng: &mut Rng, length: usize) -> usize {
    if length == 0 {
        return 0;
    }
    if rng.chance(1, 2) {
        rng.below(length.min(512))
    } else {
        rng.below(length)
    }
}

fn mutate_once(rng: &mut Rng, input: &mut Vec<u8>, other: &[u8], max_len: usize) {
    let length = input.len();
    match rng.below(14) {
        0 | 1 if length > 0 => {
            let at = position(rng, length);
            input[at] ^= 1 << rng.below(8);
        }
        2 if length > 0 => {
            let at = position(rng, length);
            input[at] = rng.byte();
        }
        3 if length > 0 => {
            let at = position(rng, length);
            input[at] = [0, 0x7f, 0x80, 0xff, b'0', b'"', b'{', b'['][rng.below(8)];
        }
        4 if length > 0 && rng.chance(1, 4) => {
            // Truncate: common for framed formats, but rarer so stacked
            // rounds still reach deeper structure.
            input.truncate(rng.below(length));
        }
        5 if length > 1 => {
            let start = position(rng, length);
            let end = (start + 1 + rng.below(64.min(length - start))).min(length);
            input.drain(start..end);
        }
        6 if length < max_len => {
            let at = position(rng, length + 1).min(length);
            let count = 1 + rng.below(32);
            let bytes: Vec<u8> = (0..count).map(|_| rng.byte()).collect();
            input.splice(at..at, bytes);
        }
        7 if length > 0 && length < max_len => {
            // Duplicate a block in place (repeated boxes, elements, frames).
            let start = position(rng, length);
            let end = (start + 1 + rng.below(256.min(length - start))).min(length);
            let block = input[start..end].to_vec();
            let at = rng.below(length + 1);
            input.splice(at..at, block);
        }
        8..=10 if length >= 2 => {
            // Overwrite an integer field with a boundary value.
            let width = [2_usize, 4, 8][rng.below(3)].min(length);
            let at = position(rng, length - width + 1).min(length - width);
            let value = interesting(rng, length);
            let bytes = if rng.chance(1, 2) {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            };
            let slice = if rng.chance(1, 2) {
                &bytes[8 - width..]
            } else {
                &bytes[..width]
            };
            input[at..at + width].copy_from_slice(slice);
        }
        11 if !other.is_empty() => {
            // Splice: prefix of this input, suffix of another.
            let cut = rng.below(length + 1);
            let from = rng.below(other.len());
            input.truncate(cut);
            input.extend_from_slice(&other[from..]);
        }
        12 if length > 1 => {
            // Swap two blocks of equal size.
            let size = 1 + rng.below(16.min(length / 2));
            let a = rng.below(length - size + 1);
            let b = rng.below(length - size + 1);
            if a + size <= b || b + size <= a {
                for offset in 0..size {
                    input.swap(a + offset, b + offset);
                }
            }
        }
        13 if length > 0 => {
            // Increment or decrement a byte (counts, lengths, versions).
            let at = position(rng, length);
            input[at] = if rng.chance(1, 2) {
                input[at].wrapping_add(1)
            } else {
                input[at].wrapping_sub(1)
            };
        }
        _ => input.push(rng.byte()),
    }
}

/// Replaces one JSON node with a hostile or type-confused value, or splices a
/// subtree from `donor`. Returns serialized bytes.
pub fn mutate_json(rng: &mut Rng, value: &Value, donor: Option<&Value>, max_len: usize) -> Vec<u8> {
    let mut value = value.clone();
    let rounds = 1 + rng.below(3);
    for _ in 0..rounds {
        let count = count_nodes(&value, 1 << 16);
        let target = rng.below(count.max(1));
        let donor_node = donor.and_then(|donor| {
            let donor_count = count_nodes(donor, 1 << 16);
            nth(donor, rng.below(donor_count.max(1))).cloned()
        });
        let mut remaining = target;
        mutate_nth(rng, &mut value, &mut remaining, donor_node.as_ref());
    }
    let mut bytes = if rng.chance(1, 8) {
        serde_json::to_vec_pretty(&value)
    } else {
        serde_json::to_vec(&value)
    }
    .unwrap_or_default();
    if rng.chance(1, 6) {
        // Occasionally damage the text as well (duplicate keys, bad escapes).
        mutate_bytes(rng, &mut bytes, &[], max_len);
    }
    bytes.truncate(max_len);
    bytes
}

fn count_nodes(value: &Value, cap: usize) -> usize {
    let mut count = 0;
    let mut stack = vec![value];
    while let Some(node) = stack.pop() {
        count += 1;
        if count >= cap {
            break;
        }
        match node {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(map) => stack.extend(map.values()),
            _ => {}
        }
    }
    count
}

fn nth(value: &Value, mut index: usize) -> Option<&Value> {
    let mut stack = vec![value];
    while let Some(node) = stack.pop() {
        if index == 0 {
            return Some(node);
        }
        index -= 1;
        match node {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(map) => stack.extend(map.values()),
            _ => {}
        }
    }
    None
}

fn mutate_nth(
    rng: &mut Rng,
    value: &mut Value,
    remaining: &mut usize,
    donor: Option<&Value>,
) -> bool {
    if *remaining == 0 {
        mutate_node(rng, value, donor);
        return true;
    }
    *remaining -= 1;
    match value {
        Value::Array(items) => items
            .iter_mut()
            .rev()
            .any(|item| mutate_nth(rng, item, remaining, donor)),
        Value::Object(map) => map
            .values_mut()
            .rev()
            .any(|item| mutate_nth(rng, item, remaining, donor)),
        _ => false,
    }
}

fn nested(depth: usize, object: bool) -> Value {
    let mut value = Value::Null;
    for _ in 0..depth {
        value = if object {
            let mut map = Map::new();
            map.insert("a".into(), value);
            Value::Object(map)
        } else {
            Value::Array(vec![value])
        };
    }
    value
}

fn hostile_string(rng: &mut Rng) -> String {
    match rng.below(6) {
        0 => String::new(),
        1 => "x".repeat(1 + rng.below(70_000)),
        2 => "\u{0}\u{7}\u{1b}\u{feff}\u{202e}\u{fffd}".into(),
        3 => "../../../../etc/passwd".into(),
        4 => "9".repeat(1 + rng.below(400)),
        _ => "é𝄞\u{10ffff}".repeat(1 + rng.below(64)),
    }
}

fn mutate_node(rng: &mut Rng, value: &mut Value, donor: Option<&Value>) {
    if let (Some(donor), true) = (donor, rng.chance(1, 4)) {
        *value = donor.clone();
        return;
    }
    match value {
        Value::Array(items) if !items.is_empty() && rng.chance(1, 2) => {
            let index = rng.below(items.len());
            match rng.below(4) {
                0 => {
                    items.remove(index);
                }
                1 => {
                    let copy = items[index].clone();
                    let bound = if rng.chance(1, 8) { 4096 } else { 3 };
                    let copies = 1 + rng.below(bound);
                    for _ in 0..copies {
                        items.insert(index, copy.clone());
                    }
                }
                2 => {
                    let other = rng.below(items.len());
                    items.swap(index, other);
                }
                _ => items.truncate(index),
            }
            return;
        }
        Value::Object(map) if !map.is_empty() && rng.chance(1, 2) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            let key = &keys[rng.below(keys.len())];
            match rng.below(3) {
                0 => {
                    map.remove(key);
                }
                1 => {
                    map.insert(format!("{key}_unknown"), Value::Null);
                }
                _ => {
                    let moved = map.remove(key).unwrap_or(Value::Null);
                    map.insert(key.to_uppercase(), moved);
                }
            }
            return;
        }
        Value::Number(number) if rng.chance(1, 2) => {
            let replacement = if let Some(integer) = number.as_i64() {
                match rng.below(5) {
                    0 => Value::from(integer.wrapping_add(1)),
                    1 => Value::from(integer.wrapping_sub(1)),
                    2 => Value::from(integer.wrapping_neg()),
                    3 => Value::from(integer.wrapping_mul(1 << rng.below(40))),
                    _ => Number::from_f64(integer as f64 + 0.5).map_or(Value::Null, Value::Number),
                }
            } else {
                Value::from(u64::MAX)
            };
            *value = replacement;
            return;
        }
        Value::String(text) if rng.chance(1, 2) => {
            match rng.below(4) {
                0 => text.push_str(&hostile_string(rng)),
                1 => *text = hostile_string(rng),
                2 => *text = text.chars().rev().collect(),
                _ => {
                    let keep = rng.below(text.chars().count() + 1);
                    *text = text.chars().take(keep).collect();
                }
            }
            return;
        }
        _ => {}
    }
    *value = match rng.below(16) {
        0 => Value::Null,
        1 => Value::Bool(rng.chance(1, 2)),
        2 => Value::from(0),
        3 => Value::from(-1),
        4 => Value::from(i64::MAX),
        5 => Value::from(i64::MIN),
        6 => Value::from(u64::MAX),
        7 => Number::from_f64([1e308, -0.0, 0.5, 9_007_199_254_740_993.0][rng.below(4)])
            .map_or(Value::Null, Value::Number),
        8 => Value::String(hostile_string(rng)),
        9 => Value::Array(Vec::new()),
        10 => Value::Object(Map::new()),
        11 => nested(1 + rng.below(300), rng.chance(1, 2)),
        12 => Value::Array(vec![Value::from(0); 1 + rng.below(5000)]),
        13 => Value::from(1_u64 << (32 + rng.below(31))),
        14 => Value::from(i64::from(u32::MAX) + 1),
        _ => Value::String(value.to_string()),
    };
}

/// Splits `[selector][len][json]...` into its selector and frame bodies.
fn split_frames(input: &[u8]) -> Option<(u8, Vec<Vec<u8>>)> {
    let (&selector, mut rest) = input.split_first()?;
    let mut bodies = Vec::new();
    while rest.len() >= 4 {
        let length = u32::from_be_bytes(rest[..4].try_into().ok()?) as usize;
        let body = rest.get(4..4 + length)?;
        bodies.push(body.to_vec());
        rest = &rest[4 + length..];
    }
    rest.is_empty().then_some((selector, bodies))
}

/// Rewrites one frame of a well-formed frame stream: JSON-mutates its body,
/// duplicates, drops or reorders frames, or splices a donor's frame, then
/// recomputes every length prefix. `None` when the input is not well formed.
pub fn mutate_frames(rng: &mut Rng, input: &[u8], donor: &[u8], max_len: usize) -> Option<Vec<u8>> {
    let (selector, mut bodies) = split_frames(input)?;
    if bodies.is_empty() {
        return None;
    }
    let index = rng.below(bodies.len());
    match rng.below(8) {
        0 if bodies.len() > 1 => {
            bodies.remove(index);
        }
        1 => {
            let copy = bodies[index].clone();
            bodies.insert(index, copy);
        }
        2 if bodies.len() > 1 => {
            let other = rng.below(bodies.len());
            bodies.swap(index, other);
        }
        3 => {
            if let Some((_, donor_bodies)) = split_frames(donor).filter(|(_, b)| !b.is_empty()) {
                bodies[index] = donor_bodies[rng.below(donor_bodies.len())].clone();
            }
        }
        _ => {
            let donor_value = split_frames(donor)
                .and_then(|(_, b)| b.into_iter().next())
                .and_then(|body| serde_json::from_slice::<Value>(&body).ok());
            match serde_json::from_slice::<Value>(&bodies[index]) {
                Ok(value) => {
                    bodies[index] = mutate_json(rng, &value, donor_value.as_ref(), max_len);
                }
                Err(_) => {
                    let mut body = bodies[index].clone();
                    mutate_bytes(rng, &mut body, &[], max_len);
                    bodies[index] = body;
                }
            }
        }
    }
    let mut output = vec![if rng.chance(1, 16) {
        selector ^ 1
    } else {
        selector
    }];
    for body in bodies {
        output.extend_from_slice(&(body.len() as u32).to_be_bytes());
        output.extend_from_slice(&body);
    }
    output.truncate(max_len);
    Some(output)
}

/// Text inputs that stress depth and size preflight of any JSON reader.
pub fn json_bombs() -> Vec<Vec<u8>> {
    let mut bombs = vec![
        "[".repeat(100_000).into_bytes(),
        "{\"a\":".repeat(50_000).into_bytes(),
        format!("[{}0{}]", "[".repeat(10_000), "]".repeat(10_000)).into_bytes(),
        format!("\"{}\"", "\\u0000".repeat(50_000)).into_bytes(),
        format!("[{}]", vec!["0"; 200_000].join(",")).into_bytes(),
        b"1e999999".to_vec(),
        b"-0".to_vec(),
        b"\xef\xbb\xbf{}".to_vec(),
        b"{\"a\":1,\"a\":2}".to_vec(),
        b"\"\\ud800\"".to_vec(),
        Vec::new(),
        b"null".to_vec(),
    ];
    let mut object = String::from("{");
    for index in 0..20_000 {
        if index > 0 {
            object.push(',');
        }
        object.push_str(&format!("\"k{index}\":{index}"));
    }
    object.push('}');
    bombs.push(object.into_bytes());
    bombs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_stable_across_releases() {
        const PINNED: u64 = 0x63cb_e1e4_5932_0dd7;
        let mut rng = Rng::new(7);
        let first = rng.next_u64();
        assert_eq!(
            first, PINNED,
            "SplitMix64 output changed; seeds would not replay"
        );
    }

    #[test]
    fn mutators_respect_the_length_bound() {
        let mut rng = Rng::new(1);
        let seed = vec![7_u8; 100];
        for _ in 0..10_000 {
            let mut input = seed.clone();
            mutate_bytes(&mut rng, &mut input, &seed, 128);
            assert!(input.len() <= 128);
        }
        let value: Value = serde_json::json!({"a": [1, 2, {"b": "c"}], "d": 4});
        for _ in 0..2_000 {
            let bytes = mutate_json(&mut rng, &value, Some(&value), 1 << 20);
            assert!(bytes.len() <= 1 << 20);
        }
    }
}
