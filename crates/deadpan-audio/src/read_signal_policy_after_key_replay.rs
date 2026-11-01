//! Actual AFTER key streams, independently verified without widening ordinary replay.
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
fn context(position: &Value) -> Value {
    let mut frames = BTreeSet::new();
    let stack = position["active_stack"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            fields(f, &["frame", "index", "kind", "phase", "ticket"]);
            assert!(frames.insert(f["frame"].as_u64().unwrap()));
            json!({"kind":f["kind"],"phase":f["phase"],"ticket":f["ticket"],"index":f["index"]})
        })
        .collect::<Vec<_>>();
    json!({"active_key_streams":position["active_key_streams"],"selected_ticket":position["selected_ticket"],
        "semantic_stack":stack})
}
pub(super) fn assert_keys_match(ordinary: &Value, keys: &Value) -> Value {
    let old = prove(ordinary, "read-signal-policy-before-v1", true);
    let current = prove(keys, "read-signal-policy-keys-v1", true);
    assert_eq!(
        old.streams.keys().collect::<Vec<_>>(),
        current.streams.keys().collect::<Vec<_>>()
    );
    let old_calls = ordinary["calls"].as_array().unwrap();
    let new_calls = keys["calls"].as_array().unwrap();
    assert_eq!(old_calls.len(), new_calls.len());
    for (a, b) in old_calls.iter().zip(new_calls) {
        for field in [
            "ordinal",
            "caller",
            "parent",
            "parent_selected_ticket",
            "start",
            "frames",
            "depth",
            "input",
        ] {
            assert_eq!(a[field], b[field], "key call identity {field}");
        }
    }
    let mut joined = Vec::new();
    for (id, a) in &old.streams {
        let b = current.streams[id];
        assert_eq!(a["parent"], b["parent"]);
        assert_eq!(context(&a["entry_position"]), context(&b["entry_position"]));
        // Ordinary policy-query/manual-debit frames and KeysOnly row counters differ.
        // Their raw IDs remain in each receipt; actual caller tickets/context are joined.
        let au = a["updates"].as_array().unwrap();
        let bu = b["updates"].as_array().unwrap();
        assert_eq!(au.len(), bu.len());
        for (a, b) in au.iter().zip(bu) {
            assert_eq!(
                a, b,
                "all actual bytes, including this binary's engine pin, must match"
            );
        }
        assert_eq!(a["actual_finish"], b["actual_finish"]);
        assert_eq!(a["actual_scan_result"], b["actual_scan_result"]);
        joined.push(json!({"stream":id.1,"ordinary_entry":a["entry_position"],
            "current_entry":b["entry_position"],"actual_finish":b["actual_finish"]}));
    }
    json!({"streams":joined,"relation":"exact actual ordered bytes/results, same caller ancestry; distinct raw observer frame and row ordinals retained"})
}
pub(super) fn current_references(body: &Value, receipts: &[Value]) -> Value {
    let mut frames = BTreeMap::new();
    let mut streams = 0usize;
    for receipt in receipts {
        let proof = prove(&receipt["keys"], "read-signal-policy-keys-v1", true);
        streams += proof.streams.len();
        for (key, count) in proof.frames {
            if let Some(old) = frames.insert(key, count) {
                assert_eq!(old, count);
            }
        }
    }
    let mut nodes = Vec::new();
    walk(body, String::new(), &mut nodes);
    let mut references = 0usize;
    let mut listings = 0usize;
    for (p, v) in nodes {
        if p.ends_with("/saved_audio") && !v.is_null() {
            fields(v, &["key", "bytes", "event", "regenerated_from"]);
            let count = *frames
                .get(&bytes(&v["key"]))
                .expect("actual current monitor key requires actual current hash replay");
            assert_eq!(v["bytes"], u64::from(count) * 8);
            references += 1;
        }
        if let Some(entries) = v.get("entry_multiset_by_key") {
            for entry in entries.as_array().unwrap() {
                fields(entry, &["key", "bytes", "staging", "origin", "validity"]);
                let count = *frames
                    .get(&file_key(&entry["key"]))
                    .expect("current filename requires current replay");
                assert_eq!(entry["staging"], false);
                assert_eq!(entry["bytes"], 80 + u64::from(count) * 8);
                listings += 1;
            }
        }
    }
    json!({"streams":streams,"monitor_references":references,"listing_entries":listings,
        "scope":"actual current hash/progress/file-size association; no inferred update bytes or normalized body"})
}

pub(super) fn mutations_rejected(receipts: &[Value]) {
    let keys = &receipts
        .iter()
        .find(|r| !r["keys"]["key_transcripts"].as_array().unwrap().is_empty())
        .unwrap()["keys"];
    valid_keys(keys);
    let mut extra = keys.clone();
    extra["unexpected_ordinary_policy"] = json!({});
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| valid_keys(&extra))).is_err());
    let mut cases = Vec::new();
    cases.push(("/schema".to_owned(), json!("read-signal-policy-before-v1")));
    cases.push((
        "/payload_bytes".to_owned(),
        json!(keys["payload_bytes"].as_u64().unwrap() + 1),
    ));
    for p in [
        "/key_transcripts/0/updates/1/bytes/0",
        "/key_transcripts/0/actual_finish/0",
        "/key_transcripts/0/actual_scan_result/saved/key/0",
    ] {
        cases.push((
            p.to_owned(),
            json!(keys.pointer(p).unwrap().as_u64().unwrap() ^ 1),
        ));
    }
    let sample = keys["key_transcripts"][0]["updates"]
        .as_array()
        .unwrap()
        .iter()
        .position(|u| u["site"] == "input_pcm_sample")
        .unwrap();
    let path = format!("/key_transcripts/0/updates/{sample}/bytes/0");
    cases.push((
        path.clone(),
        json!(keys.pointer(&path).unwrap().as_u64().unwrap() ^ 1),
    ));
    for (path, value) in cases {
        let mut changed = keys.clone();
        *changed.pointer_mut(&path).unwrap() = value;
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| valid_keys(&changed)))
                .is_err(),
            "current keys mutation was accepted at {path}"
        );
    }
}
