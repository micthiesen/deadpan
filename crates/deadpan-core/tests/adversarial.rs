//! Gate G adversarial regression for every untrusted JSON boundary of the
//! editing core: documents, frozen audio contexts and layouts, edited slices,
//! register values, command requests and document patches. Inputs must end in
//! a typed error or a value that round-trips and validates; accepted commands
//! must produce a transaction whose inverse restores the exact document.
//! Seeds are the store's real current-schema documents and values captured
//! from them. See docs/ADVERSARIAL.md.

use deadpan_chaos::{Outcome, Target, Verdict, fuzz, reject};
use deadpan_core::*;
use serde_json::{Value, json};
use std::path::PathBuf;

#[global_allocator]
static ALLOCATOR: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../deadpan-store/tests/fixtures")
}

/// The store's frozen current-schema documents, adapted to this schema the
/// same way the migration tests adapt them.
fn documents() -> Vec<ProjectDocument> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .expect("store fixtures")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("current-") && name.ends_with(".json"))
        })
        .collect();
    paths.sort();
    let documents: Vec<ProjectDocument> = paths
        .iter()
        .map(|path| {
            let mut wire: Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            wire["schema_version"] = DOCUMENT_SCHEMA_VERSION.into();
            ProjectDocument::from_json(&wire.to_string()).unwrap()
        })
        .collect();
    assert!(documents.len() >= 6, "document fixtures missing");
    documents
}

fn text(input: &[u8]) -> Result<&str, Outcome> {
    std::str::from_utf8(input).map_err(|_| Ok(Verdict::Rejected("utf8".into())))
}

macro_rules! utf8 {
    ($input:expr) => {
        match text($input) {
            Ok(text) => text,
            Err(outcome) => return outcome,
        }
    };
}

#[test]
fn adversarial_project_documents_reject_hostile_json() {
    let seeds = documents()
        .iter()
        .map(|document| document.to_json().unwrap().into_bytes())
        .collect();
    let report = fuzz(
        Target::json("core-document").iterations(300),
        seeds,
        |input| match ProjectDocument::from_json(utf8!(input)) {
            Ok(document) => {
                document
                    .validate()
                    .map_err(|error| format!("accepted invalid document: {error}"))?;
                let encoded = document
                    .to_json()
                    .map_err(|error| format!("re-encode: {error}"))?;
                let again = ProjectDocument::from_json(&encoded)
                    .map_err(|error| format!("accepted document does not round-trip: {error}"))?;
                if again != document {
                    return Err("document round trip changed the value".into());
                }
                Ok(Verdict::Accepted)
            }
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
}

#[test]
fn adversarial_frozen_audio_contexts_and_layouts_reject_hostile_json() {
    let documents = documents();
    // Contexts do not yet retain bindings or sound events; strip them from
    // the fixtures' wire form (both are optional) to obtain capturable inputs.
    let unbound: Vec<ProjectDocument> = documents
        .iter()
        .filter_map(|document| {
            let mut wire: Value = serde_json::from_str(&document.to_json().ok()?).ok()?;
            let object = wire.as_object_mut()?;
            object.remove("audio_bindings");
            object.remove("sounds");
            object.remove("sound_routes");
            object.remove("sound_allowances");
            ProjectDocument::from_json(&wire.to_string()).ok()
        })
        .collect();
    let contexts: Vec<Vec<u8>> = unbound
        .iter()
        .filter_map(|document| FrozenAudioContext::capture(document).ok())
        .map(|context| context.to_json().unwrap().into_bytes())
        .collect();
    let layouts: Vec<Vec<u8>> = documents
        .iter()
        .filter_map(|document| FrozenAudioLayout::capture(document).ok())
        .map(|layout| layout.to_json().unwrap().into_bytes())
        .collect();
    assert!(
        contexts.len() >= 2 && !layouts.is_empty(),
        "audio seeds missing: {} contexts, {} layouts",
        contexts.len(),
        layouts.len()
    );
    let report = fuzz(
        Target::json("core-audio-context").iterations(300),
        contexts,
        |input| match FrozenAudioContext::from_json(utf8!(input)) {
            Ok(context) => {
                let encoded = context
                    .to_json()
                    .map_err(|error| format!("re-encode: {error}"))?;
                FrozenAudioContext::from_json(&encoded)
                    .map_err(|error| format!("accepted context does not round-trip: {error}"))?;
                Ok(Verdict::Accepted)
            }
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
    let report = fuzz(
        Target::json("core-audio-layout").iterations(300),
        layouts,
        |input| match FrozenAudioLayout::from_json(utf8!(input)) {
            Ok(layout) => {
                let encoded = layout
                    .to_json()
                    .map_err(|error| format!("re-encode: {error}"))?;
                FrozenAudioLayout::from_json(&encoded)
                    .map_err(|error| format!("accepted layout does not round-trip: {error}"))?;
                Ok(Verdict::Accepted)
            }
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
}

fn slices(documents: &[ProjectDocument]) -> Vec<Vec<u8>> {
    let timing: AudioTimingId =
        serde_json::from_value(json!({"allocation": "copy", "ordinal": 0})).unwrap();
    let mut seeds: Vec<Vec<u8>> = Vec::new();
    for document in documents {
        let root = document.root();
        let Some(node) = document.nodes().get(root) else {
            continue;
        };
        for child in node.kind.children() {
            let selection = SliceCaptureSelection::Child {
                node: child.clone(),
            };
            if let Ok(slice) =
                CapturedEditSlice::capture_selection(document, root, &selection, timing.clone())
            {
                seeds.push(slice.to_json().unwrap().into_bytes());
            }
        }
    }
    let frozen: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("edited_slice/range-v1.json")).unwrap(),
    )
    .unwrap();
    seeds.push(frozen["capture"].as_str().unwrap().as_bytes().to_vec());
    seeds
}

#[test]
fn adversarial_edited_slices_and_registers_reject_hostile_json() {
    let documents = documents();
    let seeds = slices(&documents);
    assert!(seeds.len() >= 3, "slice seeds missing");
    let registers: Vec<Vec<u8>> = seeds
        .iter()
        .filter_map(|bytes| CapturedEditSlice::from_json(std::str::from_utf8(bytes).ok()?).ok())
        .map(|slice| {
            serde_json::to_vec(&RegisterValue::Edited {
                slice: std::sync::Arc::new(slice),
            })
            .unwrap()
        })
        .collect();
    let report = fuzz(
        Target::json("core-edit-slice").iterations(300),
        seeds,
        |input| match CapturedEditSlice::from_json(utf8!(input)) {
            Ok(slice) => {
                let encoded = slice
                    .to_json()
                    .map_err(|error| format!("re-encode: {error}"))?;
                CapturedEditSlice::from_json(&encoded)
                    .map_err(|error| format!("accepted slice does not round-trip: {error}"))?;
                Ok(Verdict::Accepted)
            }
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
    let report = fuzz(
        Target::json("core-register-value").iterations(300),
        registers,
        |input| match serde_json::from_slice::<RegisterValue>(input) {
            Ok(value) => {
                let encoded =
                    serde_json::to_vec(&value).map_err(|error| format!("re-encode: {error}"))?;
                serde_json::from_slice::<RegisterValue>(&encoded)
                    .map_err(|error| format!("accepted register does not round-trip: {error}"))?;
                Ok(Verdict::Accepted)
            }
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
}

/// Commands that reach the editing core's common paths for every node.
fn command_seeds(documents: &[ProjectDocument]) -> Vec<Value> {
    let mut seeds = Vec::new();
    for document in documents {
        let request = |command: Value| {
            json!({
                "project_id": document.project_id(),
                "expected_revision": document.revision_id(),
                "new_revision": "adversarial-next",
                "command": command,
            })
        };
        let root = document.root();
        for (id, node) in document.nodes() {
            seeds.push(request(
                json!({"command": "rename", "node": id, "label": "x"}),
            ));
            if id != root {
                seeds.push(request(json!({"command": "delete", "node": id})));
                seeds.push(request(json!({"command": "delete_ripple", "node": id,
                    "timing": {"allocation": "adversarial", "ordinal": 0}})));
                seeds.push(request(json!({"command": "wrap_repeat", "node": id,
                    "id": "adversarial-repeat", "plays": 3, "gap": null})));
                seeds.push(request(json!({"command": "split", "node": id, "at": 1,
                    "identities": {"nodes": ["adversarial-a", "adversarial-b", "adversarial-c"]}})));
                seeds.push(request(
                    json!({"command": "set_hold_duration", "node": id, "duration": 7}),
                ));
            }
            if !node.kind.children().is_empty() {
                seeds.push(request(
                    json!({"command": "insert", "parent": id, "index": 0,
                    "subtree": {"root": "adversarial-hold", "nodes": {"adversarial-hold": {
                        "label": "Pause", "kind": {"type": "hold", "recipe": {"duration": 4,
                            "video": {"type": "background"}, "audio": {"type": "silence"}}}}}}}),
                ));
            }
        }
    }
    seeds
}

fn document_for<'a>(documents: &'a [ProjectDocument], input: &[u8]) -> &'a ProjectDocument {
    let wanted = serde_json::from_slice::<Value>(input)
        .ok()
        .and_then(|value| {
            Some((
                value.get("project_id")?.as_str()?.to_owned(),
                value.get("expected_revision")?.as_str()?.to_owned(),
            ))
        });
    wanted
        .and_then(|(project, revision)| {
            documents.iter().find(|document| {
                document.project_id().as_str() == project
                    && document.revision_id().as_str() == revision
            })
        })
        .unwrap_or(&documents[0])
}

#[test]
fn adversarial_command_requests_are_atomic_and_reversible_under_mutation() {
    let documents = documents();
    let seeds = command_seeds(&documents);
    // Successful seed transactions supply real patches for the patch target.
    let mut patches = Vec::new();
    for seed in &seeds {
        let Ok(request) = serde_json::from_value::<CommandRequest>(seed.clone()) else {
            continue;
        };
        let document = documents
            .iter()
            .find(|document| {
                document.revision_id() == &request.expected_revision
                    && document.project_id() == &request.project_id
            })
            .unwrap();
        if let Ok(transaction) = apply(document, &request) {
            patches.push(serde_json::to_vec(&transaction.forward).unwrap());
            patches.push(serde_json::to_vec(&transaction.inverse).unwrap());
        }
    }
    assert!(
        patches.len() >= 20,
        "too few seed commands apply: {}",
        patches.len()
    );
    let seed_bytes: Vec<Vec<u8>> = seeds
        .iter()
        .map(|seed| serde_json::to_vec(seed).unwrap())
        .collect();
    let report = fuzz(
        Target::json("core-command").iterations(400),
        seed_bytes,
        |input| {
            let request = match serde_json::from_slice::<CommandRequest>(input) {
                Ok(request) => request,
                Err(error) => return reject(error),
            };
            let document = document_for(&documents, input);
            match apply(document, &request) {
                Ok(transaction) => {
                    let after = transaction.forward.apply(document).map_err(|error| {
                        format!("accepted forward patch does not apply: {error}")
                    })?;
                    after
                        .validate()
                        .map_err(|error| format!("command produced invalid document: {error}"))?;
                    let restored = transaction
                        .inverse
                        .apply(&after)
                        .map_err(|error| format!("inverse patch does not apply: {error}"))?;
                    if &restored != document {
                        return Err("inverse patch does not restore the exact document".into());
                    }
                    Ok(Verdict::Accepted)
                }
                Err(error) => reject(error),
            }
        },
    );
    report.assert_clean();
    let report = fuzz(
        Target::json("core-patch").iterations(400),
        patches,
        |input| {
            let patch = match serde_json::from_slice::<DocumentPatch>(input) {
                Ok(patch) => patch,
                Err(error) => return reject(error),
            };
            // A patch names its own revision; try each document it could fit.
            let mut verdict = Verdict::Rejected("no document accepts the patch".into());
            for document in &documents {
                match patch.apply(document) {
                    Ok(after) => {
                        after
                            .validate()
                            .map_err(|error| format!("patch produced invalid document: {error}"))?;
                        verdict = Verdict::Accepted;
                    }
                    Err(error) => {
                        if verdict != Verdict::Accepted {
                            verdict = reject(error)?;
                        }
                    }
                }
            }
            Ok(verdict)
        },
    );
    report.assert_clean();
}

/// Specification Section 6: maliciously deep documents. The JSON stays flat
/// (nodes are a map), so serde's recursion limit does not apply; the core's
/// own depth bound and iterative traversal must refuse deep chains, cycles,
/// shared children and dangling references without exhausting the stack.
#[test]
fn adversarial_deep_cyclic_and_shared_structures_fail_cleanly() {
    let base: Value = serde_json::from_str(&documents()[0].to_json().unwrap()).unwrap();
    let root = base["root"].as_str().unwrap().to_owned();
    let hold = json!({"label": "Pause", "kind": {"type": "hold", "recipe": {"duration": 2,
        "video": {"type": "background"}, "audio": {"type": "silence"}}}});
    let sequence = |children: Vec<String>| json!({"label": "Group", "kind": {"type": "sequence", "children": children}});
    let document = |nodes: serde_json::Map<String, Value>| {
        let mut wire = base.clone();
        wire["nodes"] = Value::Object(nodes);
        for key in [
            "marks",
            "overrides",
            "audio_lineage",
            "audio_bindings",
            "assets",
            "sounds",
            "gap_overrides",
        ] {
            if let Some(object) = wire.as_object_mut() {
                object.remove(key);
            }
        }
        wire.to_string()
    };
    let mut cases: Vec<(&str, String)> = Vec::new();
    for depth in [1_000_usize, 100_000] {
        let mut nodes = serde_json::Map::new();
        nodes.insert(root.clone(), sequence(vec!["d0".into()]));
        for level in 0..depth {
            nodes.insert(
                format!("d{level}"),
                sequence(vec![format!("d{}", level + 1)]),
            );
        }
        nodes.insert(format!("d{depth}"), hold.clone());
        cases.push(("deep chain", document(nodes)));
    }
    let mut nodes = serde_json::Map::new();
    nodes.insert(root.clone(), sequence(vec!["a".into()]));
    nodes.insert("a".into(), sequence(vec!["b".into()]));
    nodes.insert("b".into(), sequence(vec!["a".into()]));
    cases.push(("cycle", document(nodes)));
    let mut nodes = serde_json::Map::new();
    nodes.insert(root.clone(), sequence(vec!["a".into(), "b".into()]));
    nodes.insert("a".into(), sequence(vec!["c".into()]));
    nodes.insert("b".into(), sequence(vec!["c".into()]));
    nodes.insert("c".into(), hold.clone());
    cases.push(("shared child", document(nodes)));
    let mut nodes = serde_json::Map::new();
    nodes.insert(root.clone(), sequence(vec![root.clone()]));
    cases.push(("self parent", document(nodes)));
    let mut nodes = serde_json::Map::new();
    nodes.insert(root.clone(), sequence(vec!["missing".into()]));
    cases.push(("dangling child", document(nodes)));
    let mut nodes = serde_json::Map::new();
    nodes.insert(
        root.clone(),
        sequence((0..200_000).map(|index| format!("w{index}")).collect()),
    );
    for index in 0..200_000 {
        nodes.insert(format!("w{index}"), hold.clone());
    }
    cases.push(("wide", document(nodes)));
    for (name, json) in cases {
        let started = std::time::Instant::now();
        let result = ProjectDocument::from_json(&json);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "{name} took {:?}",
            started.elapsed()
        );
        match result {
            Ok(document) => {
                assert!(
                    name == "deep chain" && json.len() < 1_000_000 || name == "wide",
                    "{name} was admitted"
                );
                document.validate().unwrap();
            }
            Err(error) => assert!(!error.to_string().is_empty(), "{name}"),
        }
    }
}

/// Gate G: saved Macro programs are register-bank rows and live request
/// bodies, so they are untrusted JSON. Seeds cover motions, Visual state,
/// yank/cut/paste/replace, Repeat, calls and a gag expansion; accepted values
/// must round-trip to an equal register value.
#[test]
fn adversarial_macro_programs_reject_hostile_json() {
    let programs = [
        json!([{"type":"begin_selection"},{"type":"move_frames","forward":false,"count":6},
            {"type":"cut","selector":{"type":"visual_selection"},"register":"b"},
            {"type":"begin_selection"},{"type":"move_frames","forward":true,"count":4},
            {"type":"finish_selection"},{"type":"replace_selection","register":"b"},
            {"type":"begin_selection"},{"type":"move_frames","forward":true,"count":6},
            {"type":"yank_selection","register":"c"}]),
        json!([{"type":"yank_beat","register":"b"},{"type":"paste","register":"b","before":false},
            {"type":"call","register":"z","count":1}]),
        json!([{"type":"yank","selector":{"type":"motion","motion":{"type":"frames","forward":true,"count":2}},"register":"b"},
            {"type":"repeat","selector":{"type":"visual_selection"},"plays":3},
            {"type":"yank","selector":{"type":"selected_beat"},"register":"b"},
            {"type":"paste","register":"a","before":true}]),
    ];
    let mut seeds: Vec<Vec<u8>> = programs
        .iter()
        .map(|instructions| {
            serde_json::to_vec(&json!({"type":"macro","program":{"instructions":instructions}}))
                .unwrap()
        })
        .collect();
    let gag = GagRecipe::from_label("The Long Answer · v1 · pause 1500ms, creep to 1.350×")
        .expect("known gag label");
    let expanded = gag.expand(false, FrameRate::new(30, 1).unwrap()).unwrap();
    seeds.push(
        serde_json::to_vec(&RegisterValue::Macro {
            program: std::sync::Arc::new(SemanticProgram::new(expanded).unwrap()),
        })
        .unwrap(),
    );
    let accepted = seeds
        .iter()
        .filter(|seed| serde_json::from_slice::<RegisterValue>(seed).is_ok())
        .count();
    assert_eq!(accepted, seeds.len(), "every Macro seed is a valid program");
    let report = fuzz(
        Target::json("core-macro-program").iterations(300),
        seeds,
        |input| match serde_json::from_slice::<RegisterValue>(input) {
            Ok(value) => {
                if let RegisterValue::Macro { program } = &value {
                    program
                        .validate()
                        .map_err(|error| format!("accepted invalid program: {error}"))?;
                }
                let encoded =
                    serde_json::to_vec(&value).map_err(|error| format!("re-encode: {error}"))?;
                let again = serde_json::from_slice::<RegisterValue>(&encoded)
                    .map_err(|error| format!("accepted register does not round-trip: {error}"))?;
                if again != value {
                    return Err("register round trip changed the value".into());
                }
                Ok(Verdict::Accepted)
            }
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
}
