//! Literal before42 relation. Bodies stay untouched; only replayed key bindings may differ.
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
fn prove(body: &Value, current: bool) -> Proof<'_> {
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
        assert_eq!(t["schema"], "read-signal-policy-before-v1");
        assert_eq!(t["key_transcript_schema"], "stage-pcm-hash-updates-v1");
        assert_eq!(t["invalid"], false);
        assert_eq!(t["dropped"], 0);
        assert_eq!(t["active_at_take"], json!([]));
        assert_eq!(t["key_failed"], false);
        assert_eq!(t["active_key_streams_at_take"], json!([]));
        assert!(t["key_result_pending_at_take"].is_null());
        let rows = t["rows"].as_array().unwrap();
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
    assert!(!proof.streams.is_empty());
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
    if a == b {
        return;
    }
    if left.engine.contains(p) {
        assert!(right.engine.contains(p));
        changes.push(json!({"path":p,"kind":"compiled_engine_update","before":a,"current":b}));
        return;
    }
    if left.payloads.contains(p) {
        assert!(right.payloads.contains(p));
        changes.push(json!({"path":p,"kind":"independently_derived_compact_row_bytes","before":a,"current":b}));
        return;
    }
    if left.listings.contains(p) {
        assert!(right.listings.contains(p));
        listing_relation(a, b, pairs);
        changes.push(json!({"path":p,"kind":"complete_duplicate_preserving_listing_bijection","before":a,"current":b}));
        return;
    }
    if left.keys.contains(p) {
        assert!(right.keys.contains(p));
        assert_eq!(pairs.get(&bytes(a)), Some(&bytes(b)), "{p}");
        changes.push(json!({"path":p,"kind":"actual_stream_bound_key","before":a,"current":b}));
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
    let old = prove(frozen, false);
    let new = prove(current, true);
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
        assert_eq!(a["ordinal"], b["ordinal"]);
        assert_eq!(a["parent"], b["parent"]);
        assert_eq!(a["entry_position"], b["entry_position"]);
        assert_eq!(a["termination"], b["termination"]);
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
        if !a["actual_finish"].is_null() {
            let x = bytes(&a["actual_finish"]);
            let y = bytes(&b["actual_finish"]);
            if let Some(prior) = pairs.insert(x.clone(), y.clone()) {
                assert_eq!(prior, y);
            }
            if let Some(prior) = inverse.insert(y.clone(), x.clone()) {
                assert_eq!(prior, x);
            }
        }
    }
    let mut changes = Vec::new();
    compare(frozen, current, "", &old, &new, &pairs, &mut changes);
    json!({"streams":old.streams.len(),"literal_bodies_equal":frozen==current,
        "derived_differences":changes,"scope":"full ordered byte replay and exact typed reference relation; no normalized body"})
}

pub(super) fn assert_mutations_rejected(frozen: &Value, current: &Value) {
    // First prove this exact pair, so a later rejection cannot be an unrelated engine mismatch.
    assert_relation(frozen, current);
    let proof = prove(current, true);
    let ((trace_path, ordinal), stream) = proof.streams.iter().next().unwrap();
    let stream_path = format!("{trace_path}/key_transcripts/{}", ordinal - 1);
    let updates = stream["updates"].as_array().unwrap();
    let nonengine = updates
        .iter()
        .position(|u| u["site"] == "input_pcm_sample")
        .unwrap();
    let finish = bytes(&stream["actual_finish"]);
    let other = proof.frames.keys().find(|k| **k != finish).unwrap();
    let progress = proof
        .keys
        .iter()
        .find(|p| p.ends_with("/saved_audio/key"))
        .unwrap();
    let listing = proof
        .listings
        .iter()
        .find(|p| !current.pointer(p).unwrap().as_array().unwrap().is_empty())
        .unwrap();
    let mut cases = vec![
        (
            "engine",
            format!("{stream_path}/updates/1/bytes/0"),
            json!(0),
        ),
        (
            "nonengine_pcm",
            format!("{stream_path}/updates/{nonengine}/bytes/0"),
            json!(bytes(&updates[nonengine]["bytes"])[0] ^ 1),
        ),
        (
            "finish",
            format!("{stream_path}/actual_finish/0"),
            json!(finish[0] ^ 1),
        ),
        (
            "stream_context",
            format!("{stream_path}/entry_position/selected_ticket"),
            json!(999),
        ),
        (
            "payload_bytes",
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
        ("typed_key_reference", progress.clone(), json!(other)),
        (
            "progress_bytes",
            progress.strip_suffix("/key").unwrap().to_owned() + "/bytes",
            json!(1),
        ),
        (
            "listing_origin",
            format!("{listing}/0/origin"),
            json!("unexpected-origin"),
        ),
        (
            "unrelated_literal",
            "/scope/checkpoint".to_owned(),
            json!("unexpected-checkpoint"),
        ),
    ];
    let entry = current.pointer(&format!("{listing}/0")).unwrap();
    let mut extra = entry.clone();
    extra["unexpected"] = json!(true);
    cases.push(("listing_extra_field", format!("{listing}/0"), extra));
    for (name, p, replacement) in cases {
        let mut changed = current.clone();
        let target = changed.pointer_mut(&p).unwrap();
        assert_ne!(
            target, &replacement,
            "{name} must actually mutate its target"
        );
        *target = replacement;
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert_relation(frozen, &changed);
            }))
            .is_err(),
            "mutation {name} was incorrectly accepted"
        );
    }
}
