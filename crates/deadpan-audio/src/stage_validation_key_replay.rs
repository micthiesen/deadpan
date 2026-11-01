//! Literal key relation for the immutable before41 body; no replacement body is made.
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

fn path(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn walk<'a>(value: &'a Value, p: String, out: &mut Vec<(String, &'a Value)>) {
    out.push((p.clone(), value));
    match value {
        Value::Object(v) => {
            for (k, c) in v {
                walk(c, path(&p, k), out)
            }
        }
        Value::Array(v) => {
            for (i, c) in v.iter().enumerate() {
                walk(c, path(&p, &i.to_string()), out)
            }
        }
        _ => {}
    }
}
fn bytes(v: &Value) -> Vec<u8> {
    v.as_array()
        .expect("actual byte array")
        .iter()
        .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn hash(b: &[u8]) -> Vec<u8> {
    blake3::hash(b).as_bytes().to_vec()
}
fn identity(position: &Value) -> Value {
    let stack = position["active_stack"].as_array().unwrap();
    let build = stack
        .iter()
        .rev()
        .find(|f| f["kind"] == "build_stage")
        .unwrap();
    let caller = stack
        .iter()
        .rev()
        .find(|f| {
            matches!(
                f["kind"].as_str(),
                Some("caller_current_stage" | "caller_projected")
            )
        })
        .unwrap();
    assert_eq!(build["parent"], caller["ordinal"]);
    json!({"caller":{"ordinal":caller["ordinal"],"kind":caller["kind"],"context":caller["context"]},
        "build":{"ordinal":build["ordinal"],"parent":build["parent"],"context":build["context"]}})
}
struct Proof<'a> {
    streams: BTreeMap<(String, u64), (&'a Value, Value)>,
    roles: BTreeMap<Vec<u8>, u64>,
    engine_paths: BTreeSet<String>,
}
fn prove(body: &Value, current: bool) -> Proof<'_> {
    let mut nodes = Vec::new();
    walk(body, String::new(), &mut nodes);
    let mut proof = Proof {
        streams: BTreeMap::new(),
        roles: BTreeMap::new(),
        engine_paths: BTreeSet::new(),
    };
    for (p, t) in &nodes {
        if t["key_transcript_schema"] != "stage-pcm-hash-updates-v1" {
            continue;
        }
        assert_eq!(t["invalid"], false);
        assert_eq!(t["dropped"], 0);
        assert_eq!(t["active_at_take"], json!([]));
        assert_eq!(t["key_failed"], false);
        assert_eq!(t["active_key_streams_at_take"], json!([]));
        assert!(t["key_result_pending_at_take"].is_null());
        let streams = t["key_transcripts"].as_array().unwrap();
        assert!(streams.len() <= 256);
        let raw = t["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["phase"] == "saved_key_raw_result")
            .collect::<Vec<_>>();
        let mut used = BTreeSet::new();
        let mut global = Vec::new();
        let mut total = 0usize;
        for (i, s) in streams.iter().enumerate() {
            let ordinal = (i + 1) as u64;
            assert_eq!(s["ordinal"], ordinal);
            assert_eq!(s["exited"], true);
            let mut ancestry = Vec::new();
            let mut parent = s["parent"].as_u64();
            while let Some(n) = parent {
                assert!(n > 0 && n < ordinal);
                ancestry.push(n);
                parent = streams[n as usize - 1]["parent"].as_u64();
            }
            ancestry.reverse();
            assert_eq!(s["entry_position"]["active_key_streams"], json!(ancestry));
            let id = identity(&s["entry_position"]);
            let rows = raw
                .iter()
                .filter(|r| identity(&r["position"]) == id)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 1);
            assert!(used.insert(rows[0]["ordinal"].as_u64().unwrap()));
            assert_eq!(rows[0]["value"], s["actual_scan_result"]);
            assert_eq!(rows[0]["work"]["billing_lane"], "Cache");
            let updates = s["updates"].as_array().unwrap();
            assert!(updates.len() >= HEAD.len());
            let mut payload = Vec::new();
            for (j, u) in updates.iter().enumerate() {
                assert_eq!(u["ordinal"], j + 1);
                if j < 8 {
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
            for (j, name) in [(3, "input_frames"), (4, "output_frames")] {
                let b = bytes(&updates[j]["bytes"]);
                let n = u32::from_le_bytes(b.try_into().unwrap());
                assert_eq!(json!(n), id["build"]["context"][name]);
            }
            for (j, name) in [(5, "numerator"), (6, "denominator")] {
                let b = bytes(&updates[j]["bytes"]);
                let n = u64::from_le_bytes(b.try_into().unwrap());
                assert_eq!(json!(n), id["build"]["context"]["rate"][name]);
            }
            let pitch = i32::from_le_bytes(bytes(&updates[7]["bytes"]).try_into().unwrap());
            assert_eq!(json!(pitch), id["build"]["context"]["pitch_semitones"]);
            total += payload.len();
            let result = &s["actual_scan_result"];
            if s["actual_finish"].is_null() {
                assert_eq!(s["termination"], "prefix_only");
                assert_eq!(result["status"], "Err");
            } else {
                assert_eq!(s["termination"], "finalized");
                assert_eq!(result["status"], "Ok");
                let finish = bytes(&s["actual_finish"]);
                assert_eq!(finish.len(), 32);
                assert_eq!(hash(&payload), finish);
                assert_eq!(bytes(&result["saved"]["key"]), finish);
                let frames = id["build"]["context"]["output_frames"].as_u64().unwrap();
                let input = id["build"]["context"]["input_frames"].as_u64().unwrap();
                assert_eq!(
                    updates
                        .iter()
                        .filter(|u| u["site"] == "input_pcm_sample")
                        .count() as u64,
                    input * 2
                );
                assert_eq!(
                    updates
                        .iter()
                        .filter(|u| u["site"] == "output_suppression_bit")
                        .count() as u64,
                    frames
                );
                let mut deps = BTreeMap::<String, Vec<u8>>::new();
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
                            deps.insert(String::from_utf8(name).unwrap(), provenance)
                                .is_none()
                        );
                    }
                }
                assert_eq!(json!(deps), result["saved"]["dependencies"]);
                if let Some(old) = proof.roles.insert(finish, frames) {
                    assert_eq!(old, frames);
                }
            }
            proof
                .engine_paths
                .insert(format!("{p}/key_transcripts/{i}/updates/1/bytes"));
            assert!(
                proof
                    .streams
                    .insert((p.clone(), ordinal), (s, id))
                    .is_none()
            );
        }
        assert_eq!(used.len(), raw.len());
        global.sort_unstable();
        assert_eq!(
            global,
            (1..=t["key_updates"].as_u64().unwrap()).collect::<Vec<_>>()
        );
        assert_eq!(total, t["key_bytes"].as_u64().unwrap() as usize);
        assert!(total <= 16 * 1024 * 1024 && global.len() <= 262144);
    }
    assert!(!proof.streams.is_empty());
    for (p, v) in nodes {
        if let Some(key) = v
            .get("key")
            .filter(|k| k.is_array() && k.as_array().unwrap().len() == 32)
            && (v.get("event").is_some() || v.get("dependencies").is_some())
        {
            let k = bytes(key);
            let frames = proof
                .roles
                .get(&k)
                .expect("actual reference needs replayed key");
            if v.get("event").is_some() {
                assert_eq!(v["bytes"], frames * 8, "{p}");
            }
        }
        if let Some(list) = v.get("entry_multiset_by_key") {
            listing(list, &proof.roles);
        }
    }
    proof
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2));
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn listing(v: &Value, roles: &BTreeMap<Vec<u8>, u64>) -> Vec<String> {
    let mut result = Vec::new();
    for entry in v.as_array().unwrap() {
        let fields = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            fields,
            BTreeSet::from(["key", "bytes", "staging", "origin", "validity"])
        );
        let name = entry["key"].as_str().unwrap();
        let h = name
            .strip_prefix("b3-")
            .unwrap()
            .strip_suffix(".pcm")
            .unwrap();
        assert_eq!(h.len(), 64);
        assert!(
            h.bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        );
        let role = *roles
            .get(&unhex(h))
            .expect("literal filename needs replayed key");
        assert!(matches!(role, 640 | 768));
        assert_eq!(entry["staging"], false);
        assert_eq!(entry["bytes"], 80 + role * 8);
        result.push(
            serde_json::to_string(
                &json!({"frames":role,"bytes":entry["bytes"],"staging":entry["staging"],
            "origin":entry["origin"],"validity":entry["validity"]}),
            )
            .unwrap(),
        );
    }
    result.sort();
    result
}
fn compare(
    a: &Value,
    b: &Value,
    p: &str,
    left: &Proof<'_>,
    right: &Proof<'_>,
    changes: &mut Vec<Value>,
) {
    if a == b {
        return;
    }
    if left.engine_paths.contains(p) {
        assert!(right.engine_paths.contains(p));
        changes.push(json!({"path":p,"kind":"literal_engine_input","before":a,"current":b}));
        return;
    }
    if p.ends_with("/entry_multiset_by_key") {
        assert_eq!(listing(a, &left.roles), listing(b, &right.roles), "{p}");
        changes.push(
            json!({"path":p,"kind":"actual_keyed_listing_role_relation","before":a,"current":b}),
        );
        return;
    }
    if (p.ends_with("/key") || p.ends_with("/actual_finish"))
        && a.is_array()
        && b.is_array()
        && a.as_array().unwrap().len() == 32
        && b.as_array().unwrap().len() == 32
    {
        let old = bytes(a);
        let new = bytes(b);
        if let (Some(x), Some(y)) = (left.roles.get(&old), right.roles.get(&new)) {
            assert_eq!(x, y, "{p}");
            changes.push(json!({"path":p,"kind":"replayed_actual_key",
                "frames":x,"before":hex(&old),"current":hex(&new)}));
            return;
        }
    }
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{p}"
            );
            for (k, v) in a {
                compare(v, &b[k], &path(p, k), left, right, changes);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{p}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &path(p, &i.to_string()), left, right, changes);
            }
        }
        _ => assert_eq!(a, b, "unexplained literal body difference at {p}"),
    }
}
pub(super) fn assert_relation(frozen: &Value, current: &Value) -> Value {
    assert_eq!(
        hex(&hash(b"")),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    assert_eq!(
        hex(&hash(b"abc")),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    let old = prove(frozen, false);
    let new = prove(current, true);
    assert_eq!(
        old.streams.keys().collect::<Vec<_>>(),
        new.streams.keys().collect::<Vec<_>>()
    );
    for (k, (a, id)) in &old.streams {
        let (b, other) = &new.streams[k];
        assert_eq!(id, other);
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
    }
    let mut changes = Vec::new();
    compare(frozen, current, "", &old, &new, &mut changes);
    json!({"streams":old.streams.len(),"literal_bodies_equal":frozen==current,
        "derived_differences":changes,"scope":"real full-byte replay/reference relation only; no normalized body"})
}
