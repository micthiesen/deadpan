//! Current KeysOnly byte and typed-reference validation for ordinary O observations.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

const DOMAIN: &[u8] = b"deadpan-canonical-pcm-input-key-1\0stereo-f32-le-48000\0";
const HEAD: [&str; 8] = [
    "domain",
    "engine_pin",
    "recipe_engine_id",
    "input_frames",
    "output_frames",
    "rate_numerator",
    "rate_denominator",
    "pitch_semitones",
];
type Key = Vec<u8>;
type StreamId = (String, u64);

fn path(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn walk<'a>(v: &'a Value, p: String, out: &mut Vec<(String, &'a Value)>) {
    out.push((p.clone(), v));
    match v {
        Value::Object(v) => {
            for (k, c) in v {
                walk(c, path(&p, k), out);
            }
        }
        Value::Array(v) => {
            for (i, c) in v.iter().enumerate() {
                walk(c, path(&p, &i.to_string()), out);
            }
        }
        _ => {}
    }
}
fn fields(v: &Value, expected: &[&str]) {
    assert_eq!(
        v.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected.iter().copied().collect::<BTreeSet<_>>()
    );
}
fn bytes(v: &Value) -> Key {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn hex(v: &[u8]) -> String {
    v.iter().map(|v| format!("{v:02x}")).collect()
}
fn file_key(v: &Value) -> Key {
    let s = v.as_str().unwrap();
    let s = s.strip_prefix("b3-").unwrap().strip_suffix(".pcm").unwrap();
    assert_eq!(s.len(), 64);
    assert!(
        s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn key_hash(v: &[u8]) -> Key {
    blake3::hash(v).as_bytes().to_vec()
}
fn input_frames(s: &Value) -> u32 {
    u32::from_le_bytes(bytes(&s["updates"][3]["bytes"]).try_into().unwrap())
}
fn output_frames(s: &Value) -> u32 {
    u32::from_le_bytes(bytes(&s["updates"][4]["bytes"]).try_into().unwrap())
}
struct Proof<'a> {
    streams: BTreeMap<StreamId, &'a Value>,
    frames: BTreeMap<Key, u32>,
    engine: BTreeSet<String>,
    keys: BTreeSet<String>,
    listings: BTreeSet<String>,
    payloads: BTreeSet<String>,
}
fn keys_fields(value: &Value) {
    fields(
        value,
        &[
            "schema",
            "attempted",
            "dropped",
            "invalid",
            "active_at_take",
            "calls",
            "rows",
            "payload_bytes",
            "key_transcript_schema",
            "key_updates",
            "key_bytes",
            "key_failed",
            "active_key_streams_at_take",
            "key_result_pending_at_take",
            "key_transcripts",
        ],
    );
}
fn prove<'a>(body: &'a Value, schema: &str, current: bool) -> Proof<'a> {
    if schema == "read-signal-policy-keys-v1" {
        keys_fields(body);
    }
    let mut nodes = Vec::new();
    walk(body, String::new(), &mut nodes);
    let mut proof = Proof {
        streams: BTreeMap::new(),
        frames: BTreeMap::new(),
        engine: BTreeSet::new(),
        keys: BTreeSet::new(),
        listings: BTreeSet::new(),
        payloads: BTreeSet::new(),
    };
    for (p, t) in &nodes {
        if t.get("key_transcript_schema").is_none() {
            continue;
        }
        assert_eq!(t["schema"], schema);
        assert_eq!(t["key_transcript_schema"], "stage-pcm-hash-updates-v1");
        assert_eq!(t["invalid"], false);
        assert_eq!(t["dropped"], 0);
        assert_eq!(t["active_at_take"], json!([]));
        assert_eq!(t["key_failed"], false);
        assert_eq!(t["active_key_streams_at_take"], json!([]));
        assert!(t["key_result_pending_at_take"].is_null());
        let rows = t["rows"].as_array().unwrap();
        frame_history(t);
        assert_eq!(t["attempted"].as_u64().unwrap(), rows.len() as u64);
        let payload: usize = rows
            .iter()
            .map(|r| serde_json::to_vec(r).unwrap().len())
            .sum();
        assert_eq!(t["payload_bytes"].as_u64().unwrap(), payload as u64);
        proof.payloads.insert(path(p, "payload_bytes"));
        let raw = rows
            .iter()
            .filter(|r| r["phase"] == "saved_result" && r["value"]["phase"] == "RawResult")
            .collect::<Vec<_>>();
        let streams = t["key_transcripts"].as_array().unwrap();
        let mut used = BTreeSet::new();
        let mut global = Vec::new();
        let mut total = 0usize;
        for (i, s) in streams.iter().enumerate() {
            fields(
                s,
                &[
                    "ordinal",
                    "parent",
                    "entry_position",
                    "updates",
                    "actual_finish",
                    "actual_scan_result",
                    "exited",
                    "termination",
                ],
            );
            let ordinal = (i + 1) as u64;
            assert_eq!(s["ordinal"], ordinal);
            assert_eq!(s["exited"], true);
            let mut ancestors = Vec::new();
            let mut parent = s["parent"].as_u64();
            while let Some(n) = parent {
                assert!(n > 0 && n < ordinal);
                ancestors.push(n);
                parent = streams[n as usize - 1]["parent"].as_u64();
            }
            ancestors.reverse();
            assert_eq!(s["entry_position"]["active_key_streams"], json!(ancestors));
            let entry = &s["entry_position"];
            if let Some(ticket) = entry["selected_ticket"].as_u64() {
                let call = &t["calls"][ticket as usize - 1];
                assert_eq!(call["ordinal"], ticket);
                assert_eq!(call["entered_read_signal"], true);
            }
            let joined = raw
                .iter()
                .filter(|r| {
                    r["position"]["active_stack"] == entry["active_stack"]
                        && r["position"]["selected_ticket"] == entry["selected_ticket"]
                })
                .collect::<Vec<_>>();
            assert_eq!(joined.len(), 1, "{p} stream {ordinal}");
            let row = joined[0];
            assert!(used.insert(row["ordinal"].as_u64().unwrap()));
            assert_eq!(row["value"]["result"], s["actual_scan_result"]);
            assert_eq!(row["snapshot"]["work"]["billing_lane"], "Cache");
            let updates = s["updates"].as_array().unwrap();
            assert!(updates.len() >= HEAD.len());
            let mut payload = Vec::new();
            for (j, u) in updates.iter().enumerate() {
                fields(u, &["ordinal", "capture_ordinal", "site", "bytes"]);
                assert_eq!(u["ordinal"], j + 1);
                if j < HEAD.len() {
                    assert_eq!(u["site"], HEAD[j]);
                }
                assert!(matches!(
                    u["site"].as_str(),
                    Some(
                        "domain"
                            | "engine_pin"
                            | "recipe_engine_id"
                            | "input_frames"
                            | "output_frames"
                            | "rate_numerator"
                            | "rate_denominator"
                            | "pitch_semitones"
                            | "input_pcm_sample"
                            | "output_suppression_bit"
                            | "dependency_name_length"
                            | "dependency_name"
                            | "dependency_provenance"
                            | "input_mapping_serde_chunk"
                    )
                ));
                global.push(u["capture_ordinal"].as_u64().unwrap());
                let b = bytes(&u["bytes"]);
                match u["site"].as_str().unwrap() {
                    "input_pcm_sample" => assert_eq!(b.len(), 4),
                    "output_suppression_bit" => assert!(b == [0] || b == [1]),
                    _ => {}
                }
                payload.extend(b);
            }
            assert_eq!(
                updates.iter().filter(|u| u["site"] == "engine_pin").count(),
                1
            );
            assert_eq!(bytes(&updates[0]["bytes"]).as_slice(), DOMAIN);
            let engine = bytes(&updates[1]["bytes"]);
            assert_eq!(engine.len(), 64);
            assert!(engine.iter().all(u8::is_ascii_hexdigit));
            if current {
                assert_eq!(
                    engine.as_slice(),
                    crate::persistent_pcm::ENGINE_PIN.as_bytes()
                );
            }
            assert_eq!(bytes(&updates[5]["bytes"]).len(), 8);
            assert_eq!(bytes(&updates[6]["bytes"]).len(), 8);
            assert_eq!(bytes(&updates[7]["bytes"]).len(), 4);
            let input = input_frames(s);
            let output = output_frames(s);
            total += payload.len();
            let result = &s["actual_scan_result"];
            if s["actual_finish"].is_null() {
                assert_eq!(s["termination"], "prefix_only");
                assert_eq!(result["status"], "Err");
            } else {
                assert_eq!(s["termination"], "finalized");
                fields(result, &["status", "saved"]);
                assert_eq!(result["status"], "Ok");
                fields(&result["saved"], &["key", "dependencies", "relative_depth"]);
                let key = bytes(&s["actual_finish"]);
                assert_eq!(key.len(), 32);
                assert_eq!(key_hash(&payload), key);
                assert_eq!(bytes(&result["saved"]["key"]), key);
                assert_eq!(
                    updates
                        .iter()
                        .filter(|u| u["site"] == "input_pcm_sample")
                        .count() as u64,
                    u64::from(input) * 2
                );
                assert_eq!(
                    updates
                        .iter()
                        .filter(|u| u["site"] == "output_suppression_bit")
                        .count() as u64,
                    u64::from(output)
                );
                let mut dependencies = BTreeMap::<String, Key>::new();
                for (j, u) in updates.iter().enumerate() {
                    if u["site"] == "dependency_name_length" {
                        assert_eq!(updates[j + 1]["site"], "dependency_name");
                        assert_eq!(updates[j + 2]["site"], "dependency_provenance");
                        let name = bytes(&updates[j + 1]["bytes"]);
                        assert_eq!(
                            u64::from_le_bytes(bytes(&u["bytes"]).try_into().unwrap()),
                            name.len() as u64
                        );
                        let provenance = bytes(&updates[j + 2]["bytes"]);
                        assert_eq!(provenance.len(), 32);
                        assert!(
                            dependencies
                                .insert(String::from_utf8(name).unwrap(), provenance)
                                .is_none()
                        );
                    }
                }
                assert_eq!(json!(dependencies), result["saved"]["dependencies"]);
                if let Some(old) = proof.frames.insert(key, output) {
                    assert_eq!(old, output);
                }
            }
            let sp = format!("{p}/key_transcripts/{i}");
            proof.engine.insert(format!("{sp}/updates/1/bytes"));
            if !s["actual_finish"].is_null() {
                proof.keys.insert(format!("{sp}/actual_finish"));
                proof
                    .keys
                    .insert(format!("{sp}/actual_scan_result/saved/key"));
            }
            assert!(proof.streams.insert((p.clone(), ordinal), s).is_none());
        }
        assert_eq!(used.len(), raw.len());
        global.sort_unstable();
        assert_eq!(
            global,
            (1..=t["key_updates"].as_u64().unwrap()).collect::<Vec<_>>()
        );
        assert_eq!(total as u64, t["key_bytes"].as_u64().unwrap());
    }
    for (p, v) in &nodes {
        if p.ends_with("/saved_audio") && !v.is_null() {
            assert!(
                p.ends_with("/monitor_after/saved_audio")
                    || p.ends_with("/monitor_before/saved_audio")
                    || p.ends_with("/snapshot/progress/saved_audio")
            );
            fields(v, &["key", "bytes", "event", "regenerated_from"]);
            let key = bytes(&v["key"]);
            let frames = *proof
                .frames
                .get(&key)
                .expect("progress key requires actual replay");
            assert_eq!(v["bytes"], u64::from(frames) * 8);
            proof.keys.insert(path(p, "key"));
        }
        if let Some(list) = v.get("entry_multiset_by_key") {
            for entry in list.as_array().unwrap() {
                fields(entry, &["key", "bytes", "staging", "origin", "validity"]);
                let key = file_key(&entry["key"]);
                let frames = *proof
                    .frames
                    .get(&key)
                    .expect("listing key requires actual replay");
                assert_eq!(entry["staging"], false);
                assert_eq!(entry["bytes"], 80 + u64::from(frames) * 8);
            }
            proof.listings.insert(path(p, "entry_multiset_by_key"));
        }
    }
    // Only typed saved Result rows can introduce these additional key references.
    for (p, t) in &nodes {
        if t.get("key_transcript_schema").is_none() {
            continue;
        }
        for (i, row) in t["rows"].as_array().unwrap().iter().enumerate() {
            if row["phase"] == "saved_result"
                && row["value"]["result"]["status"] == "Ok"
                && !row["value"]["result"]["saved"].is_null()
            {
                let saved = &row["value"]["result"]["saved"];
                fields(saved, &["key", "dependencies", "relative_depth"]);
                assert!(proof.frames.contains_key(&bytes(&saved["key"])));
                proof
                    .keys
                    .insert(format!("{p}/rows/{i}/value/result/saved/key"));
            }
        }
    }
    proof
}
fn selected(position: &Value) -> Value {
    for frame in position["active_stack"].as_array().unwrap().iter().rev() {
        if matches!(
            frame["kind"].as_str(),
            Some("consumer" | "invocation" | "mix")
        ) {
            return if frame["kind"] == "mix" {
                Value::Null
            } else {
                frame["ticket"].clone()
            };
        }
    }
    Value::Null
}
fn frame_step(
    stack: &mut Vec<Value>,
    created: &mut u64,
    closed: u64,
    position: &Value,
    exit: Option<&Value>,
) {
    let current = position["active_stack"].as_array().unwrap();
    assert_eq!(position["selected_ticket"], selected(position));
    let next = closed + current.len() as u64;
    assert!(next >= *created);
    let births = next - *created;
    let prefix = if let Some(row) = exit {
        if births == 0 {
            let previous = stack
                .last()
                .expect("actual lexical exit has its active frame");
            assert_eq!(previous["kind"], row["value"]["kind"]);
            assert_eq!(previous["phase"], row["value"]["phase"]);
            assert_eq!(previous["ticket"], row["ticket"]);
            stack.len() - 1
        } else {
            stack.len()
        }
    } else {
        stack.len()
    };
    assert!(current.len() >= prefix);
    assert_eq!(&current[..prefix], &stack[..prefix]);
    let visible_births = births - u64::from(exit.is_some() && births > 0);
    assert_eq!(current.len() - prefix, visible_births as usize);
    for (index, frame) in current[prefix..].iter().enumerate() {
        assert_eq!(
            frame["frame"].as_u64().unwrap(),
            *created + index as u64 + 1
        );
    }
    *created = next;
    *stack = current.clone();
}
fn frame_history(trace: &Value) {
    let mut entries = BTreeMap::<u64, Vec<&Value>>::new();
    for stream in trace["key_transcripts"].as_array().unwrap() {
        let p = &stream["entry_position"];
        entries
            .entry(p["last_attempted_ordinal"].as_u64().unwrap())
            .or_default()
            .push(p);
    }
    let mut stack = Vec::new();
    let mut created = 0u64;
    let mut closed = 0u64;
    if let Some(values) = entries.remove(&0) {
        for p in values {
            frame_step(&mut stack, &mut created, closed, p, None);
        }
    }
    for (index, row) in trace["rows"].as_array().unwrap().iter().enumerate() {
        assert_eq!(row["ordinal"], index + 1);
        assert_eq!(row["position"]["last_attempted_ordinal"], row["ordinal"]);
        let exit = row["phase"] == "lexical_exit";
        closed += u64::from(exit);
        frame_step(
            &mut stack,
            &mut created,
            closed,
            &row["position"],
            exit.then_some(row),
        );
        if let Some(values) = entries.remove(&((index + 1) as u64)) {
            for p in values {
                frame_step(&mut stack, &mut created, closed, p, None);
            }
        }
    }
    assert!(entries.is_empty());
    assert!(stack.is_empty());
    assert_eq!(created, closed);
}
pub(super) fn valid_keys(keys: &Value) {
    keys_fields(keys);
    assert_eq!(
        hex(&key_hash(b"")),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    assert_eq!(
        hex(&key_hash(b"abc")),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    assert_eq!(keys["schema"], "read-signal-policy-keys-v1");
    prove(keys, "read-signal-policy-keys-v1", true);
}
fn visit_body<'a>(value: &'a Value, position: String, visit: &mut impl FnMut(&str, &'a Value)) {
    visit(&position, value);
    match value {
        Value::Object(values) => {
            for (key, child) in values {
                visit_body(child, path(&position, key), visit);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                visit_body(child, path(&position, &index.to_string()), visit);
            }
        }
        _ => {}
    }
}

fn validate_body_for(body: &Value, current: bool) -> Value {
    let mut frames = BTreeMap::<Key, u32>::new();
    let mut stream_count = 0usize;
    let mut trace_count = 0usize;
    visit_body(body, String::new(), &mut |_, value| {
        if value.get("key_transcript_schema").is_some() {
            keys_fields(value);
            assert_eq!(value["schema"], "read-signal-policy-keys-v1");
            let proof = prove(value, "read-signal-policy-keys-v1", current);
            trace_count += 1;
            stream_count += proof.streams.len();
            for (key, count) in proof.frames {
                if let Some(previous) = frames.insert(key, count) {
                    assert_eq!(previous, count);
                }
            }
        }
    });
    let mut monitor_references = 0usize;
    let mut listing_entries = 0usize;
    visit_body(body, String::new(), &mut |position, value| {
        if position.ends_with("/saved_audio") && !value.is_null() {
            fields(value, &["key", "bytes", "event", "regenerated_from"]);
            let count = *frames
                .get(&bytes(&value["key"]))
                .expect("actual monitor key requires actual current byte replay");
            assert_eq!(value["bytes"], u64::from(count) * 8);
            monitor_references += 1;
        }
        if let Some(entries) = value.get("entry_multiset_by_key") {
            for entry in entries.as_array().unwrap() {
                fields(entry, &["key", "bytes", "staging", "origin", "validity"]);
                let count = *frames
                    .get(&file_key(&entry["key"]))
                    .expect("actual filename requires actual current byte replay");
                assert_eq!(entry["staging"], false);
                assert_eq!(entry["bytes"], 80 + u64::from(count) * 8);
                listing_entries += 1;
            }
        }
    });
    json!({"actual_key_traces":trace_count,"actual_streams":stream_count,
        "actual_monitor_references":monitor_references,"actual_listing_entries":listing_entries,
        "scope":"exact current ordered bytes/ENGINE_PIN/digests and typed result/progress/filename associations; no normalized body or frozen43 expectation"})
}

pub(super) fn validate_body(body: &Value) -> Value {
    validate_body_for(body, true)
}

fn prove_body(body: &Value, current: bool) -> Proof<'_> {
    fields(
        body,
        &[
            "schema",
            "actual",
            "observed_counts",
            "current_key_proof",
            "qualification_status",
            "scope",
        ],
    );
    assert_eq!(body["schema"], "stage-block-policy-ordinary-before-v1");
    assert_eq!(
        body["observed_counts"],
        super::observed_counts(&body["actual"])
    );
    assert_eq!(
        body["current_key_proof"],
        validate_body_for(&body["actual"], current)
    );
    let mut proof = Proof {
        streams: BTreeMap::new(),
        frames: BTreeMap::new(),
        engine: BTreeSet::new(),
        keys: BTreeSet::new(),
        listings: BTreeSet::new(),
        payloads: BTreeSet::new(),
    };
    let mut nodes = Vec::new();
    walk(body, String::new(), &mut nodes);
    for (p, value) in &nodes {
        if value.get("key_transcript_schema").is_some() {
            let local = prove(value, "read-signal-policy-keys-v1", current);
            for ((relative, ordinal), stream) in local.streams {
                assert!(
                    proof
                        .streams
                        .insert((format!("{p}{relative}"), ordinal), stream)
                        .is_none()
                );
            }
            for (key, count) in local.frames {
                if let Some(previous) = proof.frames.insert(key, count) {
                    assert_eq!(previous, count);
                }
            }
            proof.engine.extend(
                local
                    .engine
                    .into_iter()
                    .map(|relative| format!("{p}{relative}")),
            );
            proof.keys.extend(
                local
                    .keys
                    .into_iter()
                    .map(|relative| format!("{p}{relative}")),
            );
            proof.listings.extend(
                local
                    .listings
                    .into_iter()
                    .map(|relative| format!("{p}{relative}")),
            );
            proof.payloads.extend(
                local
                    .payloads
                    .into_iter()
                    .map(|relative| format!("{p}{relative}")),
            );
        }
        if value["schema"] == "stage-block-policy-before-action-v1" {
            fields(
                value,
                &[
                    "schema",
                    "action",
                    "entry",
                    "observer",
                    "keys",
                    "selected_calls",
                    "zero_selected_calls",
                    "existing_mask_ops",
                    "existing_public_owner_work",
                    "supplemental_counter_scope",
                ],
            );
            let selected = super::validate_observer(&value["observer"]);
            assert_eq!(value["selected_calls"], selected);
            assert_eq!(value["zero_selected_calls"], selected == 0);
        }
        if value["schema"] == "stage-block-policy-before-v1" {
            super::validate_observer(value);
            proof.payloads.insert(path(p, "payload_bytes"));
        }
    }
    let mut engines = BTreeSet::new();
    for p in &proof.engine {
        engines.insert(bytes(body.pointer(p).unwrap()));
    }
    assert!(
        engines.len() <= 1,
        "one compiled ENGINE_PIN per complete capture"
    );
    for (p, value) in &nodes {
        if p.ends_with("/saved_audio") && !value.is_null() {
            assert!(
                p.ends_with("/monitor_after/saved_audio")
                    || p.ends_with("/monitor_before/saved_audio")
                    || p.ends_with("/snapshot/progress/saved_audio")
            );
            fields(value, &["key", "bytes", "event", "regenerated_from"]);
            let count = *proof.frames.get(&bytes(&value["key"])).unwrap();
            assert_eq!(value["bytes"], u64::from(count) * 8);
            proof.keys.insert(path(p, "key"));
        }
        if let Some(entries) = value.get("entry_multiset_by_key") {
            for entry in entries.as_array().unwrap() {
                fields(entry, &["key", "bytes", "staging", "origin", "validity"]);
                let count = *proof.frames.get(&file_key(&entry["key"])).unwrap();
                assert_eq!(entry["staging"], false);
                assert_eq!(entry["bytes"], 80 + u64::from(count) * 8);
            }
            proof.listings.insert(path(p, "entry_multiset_by_key"));
        }
    }
    proof
}

fn ordered_stream_relation(a: &Value, b: &Value) {
    assert_eq!(a["ordinal"], b["ordinal"]);
    assert_eq!(a["parent"], b["parent"]);
    assert_eq!(a["entry_position"], b["entry_position"]);
    assert_eq!(a["termination"], b["termination"]);
    assert_eq!(a["exited"], b["exited"]);
    let au = a["updates"].as_array().unwrap();
    let bu = b["updates"].as_array().unwrap();
    assert_eq!(au.len(), bu.len());
    for (a, b) in au.iter().zip(bu) {
        assert_eq!(a["ordinal"], b["ordinal"]);
        assert_eq!(a["capture_ordinal"], b["capture_ordinal"]);
        assert_eq!(a["site"], b["site"]);
        if a["site"] != "engine_pin" {
            assert_eq!(a["bytes"], b["bytes"]);
        }
    }
    assert_eq!(a["actual_finish"].is_null(), b["actual_finish"].is_null());
}

fn stream_pairs(old: &Proof<'_>, new: &Proof<'_>) -> BTreeMap<Key, Key> {
    assert_eq!(
        old.streams.keys().collect::<Vec<_>>(),
        new.streams.keys().collect::<Vec<_>>()
    );
    assert_eq!(old.engine, new.engine);
    assert_eq!(old.keys, new.keys);
    assert_eq!(old.listings, new.listings);
    assert_eq!(old.payloads, new.payloads);
    let mut pairs = BTreeMap::new();
    let mut inverse = BTreeMap::new();
    for (id, a) in &old.streams {
        let b = new.streams[id];
        ordered_stream_relation(a, b);
        if !a["actual_finish"].is_null() {
            let x = bytes(&a["actual_finish"]);
            let y = bytes(&b["actual_finish"]);
            if let Some(previous) = pairs.insert(x.clone(), y.clone()) {
                assert_eq!(previous, y);
            }
            if let Some(previous) = inverse.insert(y.clone(), x.clone()) {
                assert_eq!(previous, x);
            }
        }
    }
    pairs
}

fn listing_relation(a: &Value, b: &Value, pairs: &BTreeMap<Key, Key>) {
    let old = a.as_array().unwrap();
    let new = b.as_array().unwrap();
    assert_eq!(old.len(), new.len());
    let mut used = BTreeSet::new();
    for a in old {
        fields(a, &["key", "bytes", "staging", "origin", "validity"]);
        let expected = pairs.get(&file_key(&a["key"])).unwrap();
        let matches = new
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                !used.contains(i)
                    && file_key(&b["key"]) == *expected
                    && a["bytes"] == b["bytes"]
                    && a["staging"] == b["staging"]
                    && a["origin"] == b["origin"]
                    && a["validity"] == b["validity"]
            })
            .collect::<Vec<_>>();
        assert!(
            !matches.is_empty(),
            "no exact duplicate-preserving listing counterpart"
        );
        used.insert(matches[0].0);
    }
}

fn compare(
    a: &Value,
    b: &Value,
    p: &str,
    left: &Proof<'_>,
    right: &Proof<'_>,
    pairs: &BTreeMap<Key, Key>,
    changes: &mut Vec<Value>,
) {
    // Typed associations must also hold when a reference's literal bytes did not change.
    let role = if left.engine.contains(p) {
        assert!(right.engine.contains(p));
        Some("compiled_engine_update")
    } else if left.payloads.contains(p) {
        assert!(right.payloads.contains(p));
        Some("independently_derived_compact_row_bytes")
    } else if left.listings.contains(p) {
        assert!(right.listings.contains(p));
        listing_relation(a, b, pairs);
        Some("complete_duplicate_preserving_listing_bijection")
    } else if left.keys.contains(p) {
        assert!(right.keys.contains(p));
        assert_eq!(pairs.get(&bytes(a)), Some(&bytes(b)), "{p}");
        Some("actual_stream_bound_key")
    } else {
        None
    };
    if let Some(role) = role {
        if a != b {
            changes.push(json!({"path":p,"kind":role,"before":a,"current":b}));
        }
        return;
    }
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{p}"
            );
            for (k, v) in a {
                compare(v, &b[k], &path(p, k), left, right, pairs, changes);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{p}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &path(p, &i.to_string()), left, right, pairs, changes);
            }
        }
        _ => assert_eq!(a, b, "unexplained literal difference at {p}"),
    }
}
pub(super) fn assert_relation(frozen: &Value, current: &Value) -> Value {
    assert_eq!(
        hex(&key_hash(b"")),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    assert_eq!(
        hex(&key_hash(b"abc")),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    let old = prove_body(frozen, false);
    let new = prove_body(current, true);
    let pairs = stream_pairs(&old, &new);
    let mut changes = Vec::new();
    compare(frozen, current, "", &old, &new, &pairs, &mut changes);
    json!({"streams":old.streams.len(),"literal_bodies_equal":frozen==current,
        "derived_differences":changes,
        "scope":"full O body, ordered bytes and exact typed reference relation; both recorder payload counters independently derived; no normalized body"})
}

fn refresh_payloads(body: &mut Value) {
    let mut nodes = Vec::new();
    walk(body, String::new(), &mut nodes);
    let updates = nodes
        .iter()
        .filter_map(|(p, value)| {
            let payload = if value.get("key_transcript_schema").is_some() {
                Some(
                    value["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| serde_json::to_vec(row).unwrap().len())
                        .sum::<usize>(),
                )
            } else if value["schema"] == "stage-block-policy-before-v1" {
                Some(super::observer_payload_bytes(value))
            } else {
                None
            };
            payload.map(|bytes| (path(p, "payload_bytes"), bytes))
        })
        .collect::<Vec<_>>();
    for (p, bytes) in updates {
        *body.pointer_mut(&p).unwrap() = json!(bytes);
    }
}
fn rejects(name: &str, check: impl FnOnce()) {
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)).is_err(),
        "mutation {name} was incorrectly accepted"
    );
}
pub(super) fn assert_mutations_rejected(frozen: &Value, current: &Value) {
    assert_relation(frozen, current);
    let old = prove_body(frozen, false);
    let proof = prove_body(current, true);
    let pairs = stream_pairs(&old, &proof);
    let ((trace_path, ordinal), stream) = proof.streams.iter().next().unwrap();
    let stream_path = format!("{trace_path}/key_transcripts/{}", ordinal - 1);
    let updates = stream["updates"].as_array().unwrap();
    let sample = updates
        .iter()
        .position(|u| u["site"] == "input_pcm_sample")
        .unwrap();
    for (name, p, value) in [
        (
            "ordered_nonengine_pcm",
            format!("/updates/{sample}/bytes/0"),
            json!(bytes(&updates[sample]["bytes"])[0] ^ 1),
        ),
        ("stream_parent", "/parent".to_owned(), json!(999)),
        (
            "stream_position",
            "/entry_position/selected_ticket".to_owned(),
            json!(999),
        ),
        (
            "stream_update_order",
            "/updates/0/ordinal".to_owned(),
            json!(2),
        ),
    ] {
        let mut changed: Value = (**stream).clone();
        assert_ne!(changed.pointer(&p).unwrap(), &value);
        *changed.pointer_mut(&p).unwrap() = value;
        // Exercise ordered relation directly, without a stale digest masking this check.
        rejects(name, || ordered_stream_relation(stream, &changed));
    }
    let progress = proof
        .keys
        .iter()
        .find(|p| p.ends_with("/saved_audio/key"))
        .unwrap();
    let original = bytes(current.pointer(progress).unwrap());
    let other = proof.frames.keys().find(|key| **key != original).unwrap();
    let listing = proof
        .listings
        .iter()
        .find(|p| !current.pointer(p).unwrap().as_array().unwrap().is_empty())
        .unwrap();
    let mut nodes = Vec::new();
    walk(current, String::new(), &mut nodes);
    let observer = nodes
        .iter()
        .find(|(_, v)| v["schema"] == "stage-block-policy-before-v1")
        .unwrap()
        .0
        .clone();
    let action = nodes
        .iter()
        .find(|(_, v)| v["schema"] == "stage-block-policy-before-action-v1")
        .unwrap()
        .0
        .clone();
    let pcm = nodes
        .iter()
        .find(|(_, v)| {
            v["phase"] == "helper_final_pcm"
                && v.get("position").is_some()
                && !v["value"]["pcm_bits"].as_array().unwrap().is_empty()
        })
        .unwrap()
        .0
        .clone();
    let work = nodes
        .iter()
        .find(|(_, v)| {
            v["phase"] == "manual_debit_result"
                && v.get("position").is_some()
                && v["snapshot"]["work"].is_object()
        })
        .unwrap()
        .0
        .clone();
    let literal_cases = [
        ("literal_pcm", format!("{pcm}/value/pcm_bits/0/0")),
        ("literal_work", format!("{work}/snapshot/work/plan_work")),
    ];
    for (name, p) in literal_cases {
        let mut changed = current.clone();
        *changed.pointer_mut(&p).unwrap() =
            json!(current.pointer(&p).unwrap().as_u64().unwrap() ^ 1);
        refresh_payloads(&mut changed);
        // Both independent proofs still pass, isolating full-body literal comparison.
        prove_body(&changed, true);
        rejects(name, || {
            assert_relation(frozen, &changed);
        });
    }
    let old_reference = frozen.pointer(progress).unwrap();
    if pairs[&bytes(old_reference)] != bytes(old_reference) {
        let mut changes = Vec::new();
        rejects("unchanged_literal_cannot_bypass_typed_pair", || {
            compare(
                old_reference,
                old_reference,
                progress,
                &old,
                &proof,
                &pairs,
                &mut changes,
            );
        });
    }
    let mut changed = current.clone();
    *changed.pointer_mut(progress).unwrap() = json!(other);
    let count = proof.frames[other];
    let bytes_path = progress.strip_suffix("/key").unwrap().to_owned() + "/bytes";
    *changed.pointer_mut(&bytes_path).unwrap() = json!(u64::from(count) * 8);
    refresh_payloads(&mut changed);
    prove_body(&changed, true);
    rejects("typed_reference_to_other_valid_stream", || {
        assert_relation(frozen, &changed);
    });
    let mut changed = current.clone();
    *changed.pointer_mut(&format!("{listing}/0/origin")).unwrap() = json!("unexpected-origin");
    refresh_payloads(&mut changed);
    prove_body(&changed, true);
    rejects("listing_literal_origin", || {
        assert_relation(frozen, &changed);
    });
    let mut duplicate = current.pointer(listing).unwrap().clone();
    let entry = duplicate[0].clone();
    duplicate.as_array_mut().unwrap().push(entry);
    rejects("listing_duplicate_multiplicity", || {
        listing_relation(frozen.pointer(listing).unwrap(), &duplicate, &pairs);
    });
    let mut extra = current.pointer(&format!("{listing}/0")).unwrap().clone();
    extra["unexpected"] = json!(true);
    let cases = [
        (
            "current_ENGINE_PIN",
            format!("{stream_path}/updates/1/bytes/0"),
            json!(bytes(&updates[1]["bytes"])[0] ^ 1),
        ),
        (
            "actual_finish",
            format!("{stream_path}/actual_finish/0"),
            json!(bytes(&stream["actual_finish"])[0] ^ 1),
        ),
        (
            "N_row_payload_bytes",
            format!("{trace_path}/payload_bytes"),
            json!(
                current
                    .pointer(&format!("{trace_path}/payload_bytes"))
                    .unwrap()
                    .as_u64()
                    .unwrap()
                    + 1
            ),
        ),
        (
            "O_calls_and_rows_payload_bytes",
            format!("{observer}/payload_bytes"),
            json!(
                current
                    .pointer(&format!("{observer}/payload_bytes"))
                    .unwrap()
                    .as_u64()
                    .unwrap()
                    + 1
            ),
        ),
        (
            "key_summary",
            "/current_key_proof/actual_streams".to_owned(),
            json!(
                current["current_key_proof"]["actual_streams"]
                    .as_u64()
                    .unwrap()
                    + 1
            ),
        ),
        (
            "action_summary",
            "/observed_counts/actual_actions".to_owned(),
            json!(
                current["observed_counts"]["actual_actions"]
                    .as_u64()
                    .unwrap()
                    + 1
            ),
        ),
        (
            "action_selected_calls",
            format!("{action}/selected_calls"),
            json!(
                current
                    .pointer(&format!("{action}/selected_calls"))
                    .unwrap()
                    .as_u64()
                    .unwrap()
                    + 1
            ),
        ),
        ("listing_closed_fields", format!("{listing}/0"), extra),
        (
            "unrelated_literal",
            "/scope/checkpoint".to_owned(),
            json!("unexpected-checkpoint"),
        ),
        (
            "body_schema",
            "/schema".to_owned(),
            json!("unexpected-schema"),
        ),
    ];
    for (name, p, replacement) in cases {
        let mut changed = current.clone();
        assert_ne!(changed.pointer(&p).unwrap(), &replacement);
        *changed.pointer_mut(&p).unwrap() = replacement;
        rejects(name, || {
            assert_relation(frozen, &changed);
        });
    }
    assert_relation(frozen, current);
}

#[test]
fn stage_block_policy_body_visitor_preserves_all_nodes_and_pretty_bytes() {
    let document = json!({
        "a/b~c": [null, true, 7, "literal", [], {}, {"saved_audio": {"key": [1, 2]}}],
        "entry_multiset_by_key": [{"key": "literal.json", "bytes": 5}],
        "key_transcript_schema": "traversal-only synthetic metadata",
        "other": {"array": [{"nested": false}], "µ": "unchanged"}
    });
    let mut original = Vec::new();
    walk(&document, String::new(), &mut original);
    let mut visited = Vec::new();
    visit_body(&document, String::new(), &mut |position, value| {
        visited.push((position.to_owned(), value));
    });
    assert_eq!(original.len(), visited.len());
    for ((old_path, old_value), (new_path, new_value)) in original.iter().zip(&visited) {
        assert_eq!(old_path, new_path);
        assert!(std::ptr::eq(*old_value, *new_value));
    }
    assert!(
        visited
            .iter()
            .any(|(position, _)| position == "/a~1b~0c/6/saved_audio/key/1")
    );
    assert!(visited.iter().any(|(position, _)| position == "/other/µ"));
    let mut expected = serde_json::to_vec_pretty(&document).unwrap();
    expected.push(b'\n');
    let mut actual = Vec::new();
    {
        let mut writer = std::io::BufWriter::new(&mut actual);
        serde_json::to_writer_pretty(&mut writer, &document).unwrap();
        std::io::Write::write_all(&mut writer, b"\n").unwrap();
        std::io::Write::flush(&mut writer).unwrap();
    }
    assert_eq!(actual, expected);
}

pub(super) struct StreamedBodyProof {
    frames: BTreeMap<Key, u32>,
    trace_count: usize,
    stream_count: usize,
    monitor_references: usize,
    listing_entries: usize,
}
impl StreamedBodyProof {
    pub(super) fn new() -> Self {
        Self {
            frames: BTreeMap::new(),
            trace_count: 0,
            stream_count: 0,
            monitor_references: 0,
            listing_entries: 0,
        }
    }
    pub(super) fn collect_current(&mut self, body: &Value) {
        visit_body(body, String::new(), &mut |_, value| {
            if value.get("key_transcript_schema").is_some() {
                keys_fields(value);
                assert_eq!(value["schema"], "read-signal-policy-keys-v1");
                let proof = prove(value, "read-signal-policy-keys-v1", true);
                self.trace_count += 1;
                self.stream_count += proof.streams.len();
                for (key, count) in proof.frames {
                    self.bind(key, count);
                }
            }
        });
    }
    fn bind(&mut self, key: Key, count: u32) {
        if let Some(previous) = self.frames.insert(key, count) {
            assert_eq!(previous, count);
        }
    }
    pub(super) fn references(&mut self, body: &Value) {
        visit_body(body, String::new(), &mut |position, value| {
            if position.ends_with("/saved_audio") && !value.is_null() {
                fields(value, &["key", "bytes", "event", "regenerated_from"]);
                let count = *self
                    .frames
                    .get(&bytes(&value["key"]))
                    .expect("actual monitor key requires actual current byte replay");
                assert_eq!(value["bytes"], u64::from(count) * 8);
                self.monitor_references += 1;
            }
            if let Some(entries) = value.get("entry_multiset_by_key") {
                for entry in entries.as_array().unwrap() {
                    fields(entry, &["key", "bytes", "staging", "origin", "validity"]);
                    let count = *self
                        .frames
                        .get(&file_key(&entry["key"]))
                        .expect("actual filename requires actual current byte replay");
                    assert_eq!(entry["staging"], false);
                    assert_eq!(entry["bytes"], 80 + u64::from(count) * 8);
                    self.listing_entries += 1;
                }
            }
        });
    }
    pub(super) fn summary(&self) -> Value {
        json!({"actual_key_traces":self.trace_count,"actual_streams":self.stream_count,
            "actual_monitor_references":self.monitor_references,"actual_listing_entries":self.listing_entries,
            "scope":"exact current ordered bytes/ENGINE_PIN/digests and typed result/progress/filename associations; no normalized body or frozen43 expectation"})
    }
    pub(super) fn bindings(&self) -> Value {
        json!(
            self.frames
                .iter()
                .map(|(key, frames)| json!({"key":key,"output_frames":frames}))
                .collect::<Vec<_>>()
        )
    }
}

#[test]
fn stage_block_policy_streamed_key_summary_matches_empty_actual_capture() {
    let guard = before_trace::begin_keys();
    let keys = before_trace::take_keys();
    drop(guard);
    let body = json!({"actual_keys":keys,"monitor_after":{"saved_audio":null},
        "cache":{"entry_multiset_by_key":[]}});
    let expected = validate_body(&body);
    let mut proof = StreamedBodyProof::new();
    proof.collect_current(&body);
    proof.references(&body);
    assert_eq!(proof.summary(), expected);
    assert_eq!(proof.bindings(), json!([]));
}
#[test]
fn stage_block_policy_streamed_frame_registry_rejects_collisions_and_missing_refs() {
    // Registry mechanics only. These bytes do not establish any media/hash witness.
    let key = vec![7u8; 32];
    let mut registry = StreamedBodyProof::new();
    registry.bind(key.clone(), 3);
    registry.bind(key.clone(), 3);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || registry.bind(key.clone(), 4)
        ))
        .is_err()
    );
    let mut missing = StreamedBodyProof::new();
    let reference = json!({"monitor_after":{"saved_audio":{
        "key":key,"bytes":24,"event":"Saved","regenerated_from":null}}});
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || missing.references(&reference)
        ))
        .is_err()
    );
}

#[test]
fn stage_block_policy_streamed_partition_summary_matches_literal_aggregate() {
    let capture = || {
        let guard = before_trace::begin_keys();
        let keys = before_trace::take_keys();
        drop(guard);
        json!({"keys":keys,"saved_audio":null,"cache":{"entry_multiset_by_key":[]}})
    };
    let a = capture();
    let b = capture();
    let whole = json!({"parts":[a,b]});
    let mut proof = StreamedBodyProof::new();
    for part in whole["parts"].as_array().unwrap() {
        proof.collect_current(part);
    }
    for part in whole["parts"].as_array().unwrap() {
        proof.references(part);
    }
    assert_eq!(proof.summary(), validate_body(&whole));
    assert_eq!(proof.summary()["actual_key_traces"], 2);
}
