//! Additive private PointCeil oracle. The exact existing v1 producer performs
//! the actual captures/probes once; this collector only borrows its returned
//! Point domain after set_evaluation. It is not an allocation/peak oracle.
use super::*;

fn checked_output(path: &std::path::Path) -> std::path::PathBuf {
    assert!(path.is_absolute());
    let parent = path
        .parent()
        .expect("output parent")
        .canonicalize()
        .unwrap();
    let scratch = std::path::Path::new("/tmp").canonicalize().unwrap();
    assert!(
        parent.starts_with(scratch),
        "output must remain in canonical /tmp"
    );
    parent.join(path.file_name().expect("output filename"))
}

#[test]
fn supplement_output_rejects_parent_escape_before_file_creation() {
    let rejected = std::panic::catch_unwind(|| {
        checked_output(std::path::Path::new("/tmp/../point-supplement.json"))
    });
    assert!(rejected.is_err());
    let allowed = checked_output(std::path::Path::new("/tmp/point-supplement.json"));
    assert_eq!(
        allowed.parent().unwrap(),
        std::path::Path::new("/tmp").canonicalize().unwrap()
    );
}

thread_local! {
    static POINTS: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
}
struct PointSession;
impl PointSession {
    fn start() -> Self {
        POINTS.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none(), "nested PointCeil baseline collector");
            *slot = Some(Vec::new());
        });
        Self
    }
    fn finish(self) -> Vec<Value> {
        POINTS.with(|slot| slot.borrow_mut().take().unwrap())
    }
}
impl Drop for PointSession {
    fn drop(&mut self) {
        POINTS.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
pub(in crate::plan) fn returned_point(
    expected_plan: &RenderPlan,
    input: Option<&crate::AudioOwnerRetainedInput<'_>>,
    work: usize,
) {
    // Existing v1 observers and ordinary runtime tests never allocate here.
    if !POINTS.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let Some(input) = input else {
        return;
    };
    assert!(input.belongs_to(expected_plan));
    let AudioBoundDomain::Point(point) = input.domain() else {
        return;
    };
    let (case, phase) = RECORDING.with(|slot| {
        let slot = slot.borrow();
        let recording = slot.as_ref().expect("existing producer recording");
        (
            recording.case.as_ref().expect("actual case scope").clone(),
            recording.phase,
        )
    });
    // Borrow the actual private signal via the returned Point, never signal().
    let point = point.probe_baseline_snapshot(expected_plan);
    POINTS.with(|slot| {
        slot.borrow_mut().as_mut().unwrap().push(json!({
            "case":case,"phase":phase,"work":work,"point":point,
            "attestation":"same actual returned PointCeil raw domain after set_evaluation",
        }))
    });
}

#[test]
fn borrowed_point_private_snapshot_rejects_identical_foreign_plan() {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = frozen["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["fixture"] == "nonunity_shift_fractional_point_ceil_source44100")
        .unwrap();
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let actual = RenderPlan::compile(&doc).unwrap();
    let foreign = RenderPlan::compile(&doc).unwrap();
    let at = AudioSample(0);
    let owners = actual
        .audio_gain_owners(at..AudioSample(1), Default::default())
        .unwrap();
    let owner = owners.spans()[0]
        .owners()
        .iter()
        .find(|v| v.instance().node.as_str() == "voice")
        .unwrap();
    let input = actual
        .audio_owner_input(owner, at, Default::default())
        .unwrap();
    let probe = input.retained_probe_at(at, Default::default()).unwrap();
    let retained = probe.input().expect("actual frozen PointCeil Some");
    let AudioBoundDomain::Point(point) = retained.domain() else {
        panic!("real PointCeil fixture")
    };
    let logical = point.probe_baseline_snapshot(&actual);
    assert_eq!(logical["structural_signal"]["provider"], "Structural");
    assert_eq!(logical["structural_signal"].as_object().unwrap().len(), 11);
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        point.probe_baseline_snapshot(&foreign)
    }));
    assert_foreign_plan(rejected);
    // This valid empty Sequence cannot resolve the actual Point's child index.
    // The identity assertion must fire before index-based serialization.
    let mut shorter = fixture["document"].clone();
    shorter["nodes"].as_object_mut().unwrap().remove("voice");
    shorter["nodes"]["root"]["kind"]["children"] = json!([]);
    shorter.as_object_mut().unwrap().remove("audio_bindings");
    let shorter = ProjectDocument::from_json(&shorter.to_string()).unwrap();
    let shorter = RenderPlan::compile(&shorter).unwrap();
    assert_eq!(shorter.nodes.len(), 1);
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        point.probe_baseline_snapshot(&shorter)
    }));
    assert_foreign_plan(rejected);
}

fn assert_foreign_plan(result: std::thread::Result<Value>) {
    let payload = result.expect_err("foreign Plan must be rejected");
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    assert_eq!(message, Some("foreign Plan in Point probe snapshot"));
}

#[test]
#[ignore = "one-shot additive old-probe oracle; parent owns runtime/executable pins"]
fn capture_legacy_point_probe_supplement_v1() {
    let output = std::path::PathBuf::from(
        std::env::var_os("DEADPAN_OWNER_PROBE_OUTPUT")
            .expect("explicit new /tmp supplement path required"),
    );
    let output = checked_output(&output);
    let v1_path = std::path::PathBuf::from(
        std::env::var_os("DEADPAN_OWNER_CAPTURE_OUTPUT")
            .expect("explicit distinct new /tmp v1 compatibility output required"),
    );
    let v1_path = checked_output(&v1_path);
    assert_ne!(output, v1_path);
    let points = PointSession::start();
    // This is the existing v1 producer itself, including its exact actual
    // fixture/probe call sites and create_new output. No second query/probe.
    super::capture_legacy_owner_capture_baseline_v1();
    let points = points.finish();
    let actual: Value = serde_json::from_slice(&std::fs::read(v1_path).unwrap()).unwrap();
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let actual_fields = actual
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !["variant", "runtime_source_manifest_sha256"].contains(&key.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let frozen_fields = frozen
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !["variant", "runtime_source_manifest_sha256"].contains(&key.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        actual_fields, frozen_fields,
        "complete immutable old v1 body"
    );
    assert_eq!(actual["events"].as_array().unwrap().len(), 128);
    assert_eq!(
        points.len(),
        1,
        "one actual returned Point in the unchanged sixteen cases"
    );
    let point = &points[0];
    assert_eq!(point["case"], "retained_point_voice");
    assert_eq!(point["phase"], "retained_probe_ample");
    let old = actual["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| {
            v["case"] == point["case"] && v["observation"]["kind"] == "actual_retained_probe_result"
        })
        .unwrap();
    assert_eq!(old["observation"]["result"], "Some");
    assert_eq!(old["observation"]["work"], point["work"]);
    let private = &point["point"];
    assert_eq!(private["structural_signal"]["provider"], "Structural");
    assert_eq!(private["structural_signal"].as_object().unwrap().len(), 11);
    let body = json!({
        "schema":"OwnerProbePointSupplementV1",
        "variant":actual["variant"],
        "runtime_source_manifest_sha256":actual["runtime_source_manifest_sha256"],
        "immutable_v1_sha256":"32661e1a8abf59eb7d8146cd2898fd979415db3763685463df576d5e916d27c0",
        "legacy_v1":actual,
        "point_private_signals":points,
        "provider_coverage":"actual returned Structural only",
        "empty_raw_domain_coverage":"not exercised; no Empty claim",
        "excluded":"pointers, raw bytes, Debug, timings, capacities, observer allocation counts",
    });
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &body).unwrap();
    use std::io::Write;
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
