//! Complete callback/use cases on disk; retained metadata never substitutes for raw validation.
use super::*;
use sha2::Digest;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

struct State {
    root: PathBuf,
    files: Vec<Value>,
    index: File,
}
thread_local! {
    static JOURNAL: RefCell<Option<State>> = const { RefCell::new(None) };
}
pub(super) struct Guard {
    _temporary: Option<tempfile::TempDir>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        JOURNAL.with(|state| {
            state.borrow_mut().take();
        });
    }
}
fn create(root: PathBuf, temporary: Option<tempfile::TempDir>) -> Guard {
    let marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("INCOMPLETE"))
        .unwrap();
    marker.sync_all().unwrap();
    let index = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("index.jsonl"))
        .unwrap();
    index.sync_all().unwrap();
    JOURNAL.with(|state| {
        let mut state = state.borrow_mut();
        assert!(state.is_none());
        *state = Some(State {
            root,
            files: Vec::new(),
            index,
        });
    });
    Guard {
        _temporary: temporary,
    }
}
pub(super) fn temporary_if_absent() -> Option<Guard> {
    if JOURNAL.with(|state| state.borrow().is_some()) {
        return None;
    }
    let temporary = tempfile::tempdir().unwrap();
    Some(create(temporary.path().to_owned(), Some(temporary)))
}
pub(super) fn persistent(output: &Path) -> Guard {
    let mut root = output.as_os_str().to_owned();
    root.push(".journal");
    let root = PathBuf::from(root);
    std::fs::create_dir(&root).unwrap();
    create(root, None)
}
pub(super) fn mark() -> usize {
    JOURNAL.with(|state| state.borrow().as_ref().unwrap().files.len())
}
fn digest(file: &mut File) -> (u64, String) {
    let mut hash = sha2::Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let length = file.read(&mut buffer).unwrap();
        if length == 0 {
            break;
        }
        count = count.checked_add(u64::try_from(length).unwrap()).unwrap();
        hash.update(&buffer[..length]);
    }
    (
        count,
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}
fn catalog(reference: &Value) -> PathBuf {
    fields(
        reference,
        &["schema", "ordinal", "file", "kind", "bytes", "sha256"],
    );
    assert_eq!(reference["schema"], "stage-block-policy-evidence-ref-v1");
    let ordinal = usize::try_from(n(&reference["ordinal"])).unwrap();
    assert!(ordinal > 0);
    assert_eq!(reference["file"], format!("case-{ordinal:08}.json"));
    assert!(matches!(
        reference["kind"].as_str(),
        Some("callback" | "live_use" | "storage_test")
    ));
    assert!(n(&reference["bytes"]) > 0);
    let hash = reference["sha256"].as_str().unwrap();
    assert!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    JOURNAL.with(|state| {
        let state = state.borrow();
        let state = state.as_ref().unwrap();
        assert_eq!(state.files.get(ordinal - 1).unwrap(), reference);
        state.root.join(reference["file"].as_str().unwrap())
    })
}
pub(super) fn read_case<T>(reference: &Value, check: impl FnOnce(&Value) -> T) -> T {
    let path = catalog(reference);
    let mut file = File::open(path).unwrap();
    let (count, hash) = digest(&mut file);
    assert_eq!(reference["bytes"], count);
    assert_eq!(reference["sha256"], hash);
    file.seek(SeekFrom::Start(0)).unwrap();
    let envelope: Value = serde_json::from_reader(BufReader::new(file)).unwrap();
    fields(&envelope, &["schema", "kind", "case"]);
    assert_eq!(envelope["schema"], "stage-block-policy-evidence-case-v1");
    assert_eq!(envelope["kind"], reference["kind"]);
    check(&envelope["case"])
}
pub(super) fn append(kind: &str, raw: Value, mut check: impl FnMut(&Value)) -> Value {
    check(&raw);
    let ordinal = mark() + 1;
    let leaf = format!("case-{ordinal:08}.json");
    let root = JOURNAL.with(|state| state.borrow().as_ref().unwrap().root.clone());
    let envelope = object([
        ("schema", json!("stage-block-policy-evidence-case-v1")),
        ("kind", json!(kind)),
        ("case", raw),
    ]);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(&leaf))
        .unwrap();
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &envelope).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
    drop(writer);
    drop(envelope);
    let (count, hash) = digest(&mut File::open(root.join(&leaf)).unwrap());
    let reference = json!({"schema":"stage-block-policy-evidence-ref-v1","ordinal":ordinal,
        "file":leaf,"kind":kind,"bytes":count,"sha256":hash});
    JOURNAL.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.files.push(reference.clone());
        serde_json::to_writer(&mut state.index, &reference).unwrap();
        state.index.write_all(b"\n").unwrap();
        state.index.flush().unwrap();
        state.index.sync_all().unwrap();
    });
    read_case(&reference, &mut check);
    reference
}
fn files_from(start: usize) -> Vec<Value> {
    JOURNAL.with(|state| state.borrow().as_ref().unwrap().files[start..].to_vec())
}
fn proof(start: usize, remaining: &Value) -> super::super::key_replay::StreamedBodyProof {
    let files = files_from(start);
    let mut proof = super::super::key_replay::StreamedBodyProof::new();
    proof.collect_current(remaining);
    for reference in &files {
        read_case(reference, |raw| {
            validate_journal_case(raw);
            proof.collect_current(raw);
        });
    }
    proof.references(remaining);
    for reference in &files {
        read_case(reference, |raw| {
            validate_journal_case(raw);
            proof.references(raw);
        });
    }
    proof
}
pub(super) fn group_proof(start: usize, remaining: &Value) -> Value {
    assert_references(start, remaining);
    audit_storage();
    proof(start, remaining).summary()
}
fn references(value: &Value, found: &mut BTreeMap<u64, Value>) {
    match value {
        Value::Object(map) => {
            if map.get("schema").and_then(Value::as_str)
                == Some("stage-block-policy-evidence-ref-v1")
            {
                catalog(value);
                assert!(
                    found.insert(n(&value["ordinal"]), value.clone()).is_none(),
                    "each raw case has exactly one body reference"
                );
            } else {
                for child in map.values() {
                    references(child, found);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                references(child, found);
            }
        }
        _ => {}
    }
}
fn assert_references(start: usize, remaining: &Value) {
    let mut found = BTreeMap::new();
    references(remaining, &mut found);
    assert_eq!(found.into_values().collect::<Vec<_>>(), files_from(start));
}
fn audit_catalog(remaining: &Value) -> Value {
    assert_references(0, remaining);
    audit_storage()
}
fn audit_storage() -> Value {
    let files = files_from(0);
    let root = JOURNAL.with(|state| state.borrow().as_ref().unwrap().root.clone());
    let mut expected = files
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    expected.extend(["index.jsonl".to_owned(), "INCOMPLETE".to_owned()]);
    let actual = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    let mut file = File::open(root.join("index.jsonl")).unwrap();
    let (bytes, sha256) = digest(&mut file);
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut rows = serde_json::Deserializer::from_reader(BufReader::new(file)).into_iter::<Value>();
    for expected in &files {
        assert_eq!(&rows.next().unwrap().unwrap(), expected);
    }
    assert!(rows.next().is_none());
    json!({"file":"index.jsonl","bytes":bytes,"sha256":sha256})
}
pub(super) fn manifest(remaining: &Value) -> Value {
    let index = audit_catalog(remaining);
    let files = files_from(0);
    let proof = proof(0, remaining);
    json!({"schema":"stage-block-policy-evidence-journal-v1","status":"complete",
        "root_convention":"append .journal to raw body filepath","files":files,
        "actual_cases":files.len(),"total_case_bytes":files.iter().map(|v|n(&v["bytes"])).sum::<u64>(),
        "index":index,
        "current_key_proof":proof.summary(),"key_frame_bindings":proof.bindings(),
        "scope":"full raw callback/live-use cases; exact byte/hash readback and two complete key/reference passes; no parsed case retained across replay"})
}
pub(super) fn publish(output: &Path, manifest: &Value) {
    let root = JOURNAL.with(|state| state.borrow().as_ref().unwrap().root.clone());
    let (bytes, sha256) = digest(&mut File::open(output).unwrap());
    let completion = json!({"schema":"stage-block-policy-evidence-publication-v1","status":"complete",
        "body":{"bytes":bytes,"sha256":sha256},"journal":manifest});
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("COMPLETE.json"))
        .unwrap();
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &completion).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
    drop(writer);
    std::fs::remove_file(root.join("INCOMPLETE")).unwrap();
    File::open(root).unwrap().sync_all().unwrap();
}

#[test]
fn stage_block_policy_journal_roundtrip_and_reference_damage() {
    let _guard = temporary_if_absent().unwrap();
    let raw = json!({"target":{"ordinal":7},"cancel":false,"actual":{"literal":[null,9,"x"]}});
    let expected = raw.clone();
    let reference = append("storage_test", raw, |value| assert_eq!(value, &expected));
    read_case(&reference, |value| assert_eq!(value, &expected));
    for (field, replacement) in [
        ("ordinal", json!(2)),
        ("file", json!("../bad")),
        ("bytes", json!(n(&reference["bytes"]) + 1)),
        ("sha256", json!("0".repeat(64))),
        ("kind", json!("callback")),
    ] {
        let mut changed = reference.clone();
        changed[field] = replacement;
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_case(&changed, |_| ())))
                .is_err()
        );
    }
    let path = catalog(&reference);
    let mut damaged = std::fs::read(&path).unwrap();
    *damaged.iter_mut().find(|byte| **byte == b'x').unwrap() = b'y';
    assert_eq!(
        u64::try_from(damaged.len()).unwrap(),
        n(&reference["bytes"])
    );
    std::fs::write(&path, &damaged).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_case(
            &reference,
            |_| ()
        )))
        .is_err()
    );
    std::fs::remove_file(path).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_case(
            &reference,
            |_| ()
        )))
        .is_err()
    );
}

#[test]
fn stage_block_policy_journal_catalog_requires_exact_case_multiplicity_and_index_order() {
    let _guard = temporary_if_absent().unwrap();
    let a = append("storage_test", json!({"literal":1}), |_| ());
    let b = append("storage_test", json!({"literal":2}), |_| ());
    let body = json!({"cases":[a,b]});
    audit_catalog(&body);
    for changed in [
        json!({"cases":[a]}),
        json!({"cases":[a,b,a]}),
        json!({"cases":[b,b]}),
    ] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| audit_catalog(&changed)))
                .is_err()
        );
    }
    let root = JOURNAL.with(|state| state.borrow().as_ref().unwrap().root.clone());
    let index = root.join("index.jsonl");
    let write_rows = |values: &[&Value]| {
        let mut writer = BufWriter::new(File::create(&index).unwrap());
        for value in values {
            serde_json::to_writer(&mut writer, value).unwrap();
            writer.write_all(b"\n").unwrap();
        }
        writer.flush().unwrap();
        writer.get_ref().sync_all().unwrap();
    };
    for values in [vec![&b, &a], vec![&a], vec![&a, &b, &b]] {
        write_rows(&values);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| audit_catalog(&body)))
                .is_err()
        );
    }
    write_rows(&[&a, &b]);
    audit_catalog(&body);
    std::fs::write(root.join("unexpected.json"), b"{}").unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| audit_catalog(&body))).is_err()
    );
}
