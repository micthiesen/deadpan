//! Actual after observations keep earlier query receipts and invalid flags literal.
use super::*;
use crate::stages::point_suppression;

const CHECKPOINT: &str = "e8fcdb3cb02f9682bef53f6ba2709e8a5ad692d0";
thread_local! {
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    static INTERVALS: Cell<bool> = const { Cell::new(false) };
}
struct Capture;
impl Capture {
    fn new() -> Self {
        assert!(!CAPTURING.replace(true));
        Self
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if INTERVALS.replace(false) {
            crate::stages::native_query_interval_trace::disable();
        }
        point_suppression::ops::trace::disable();
        CAPTURING.set(false);
    }
}
pub(super) fn active() -> bool {
    CAPTURING.get()
}
pub(super) fn reset_if_capturing() {
    if active() {
        point_suppression::ops::trace::reset();
        if INTERVALS.get() {
            crate::stages::native_query_interval_trace::reset();
        }
    }
}
pub(super) fn receipt_if_capturing() -> Option<Value> {
    active().then(|| serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap())
}

pub(super) fn interval_receipt_if_capturing() -> Option<Value> {
    INTERVALS.get().then(|| {
        serde_json::to_value(crate::stages::native_query_interval_trace::snapshot()).unwrap()
    })
}

#[test]
#[ignore = "manual current internal mask qualification; fresh /tmp and pinned sources"]
fn capture_active_internal_mask_after_v1() {
    crate::stages::point_policy_request::with_legacy_policy(|| {
        capture_after("DEADPAN_INTERNAL_MASK_AFTER", false, false, false);
    });
}

#[test]
#[ignore = "manual additive query intervals; fresh /tmp and pinned sources"]
fn capture_active_query_intervals_v1() {
    crate::stages::point_policy_request::with_legacy_policy(|| {
        capture_after("DEADPAN_QUERY_INTERVAL_AFTER", true, false, false);
    });
}

#[test]
#[ignore = "manual active Point policy handoff capture; pinned sources and fresh /tmp"]
fn capture_active_point_policy_after_v1() {
    crate::stages::root_mask_request::with_legacy_root_masks(|| {
        capture_after("DEADPAN_POINT_POLICY_AFTER", true, true, false);
    });
}

#[test]
#[ignore = "manual active Root mask destination capture; pinned sources and fresh /tmp"]
fn capture_active_root_mask_after_v1() {
    assert!(!crate::stages::root_mask_request::legacy());
    capture_after("DEADPAN_ROOT_MASK_AFTER", true, true, true);
}

fn capture_after(prefix: &str, intervals: bool, policy: bool, root: bool) {
    let output = std::env::var(format!("{prefix}_OUTPUT")).expect("fresh output path");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var(format!("{prefix}_VARIANT")).expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var(format!("{prefix}_SOURCE_SHA256")).expect("source manifest");
    assert!(source.len() == 64 && source.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var(format!("{prefix}_PUBLISHED_CHECKPOINT")).unwrap(),
        if root {
            "44068bd971dee62e2ed8cbaffbb2f18922283f83"
        } else if policy {
            "eedbc58272bdd009a65239b7104ddb786a2554fc"
        } else if intervals {
            "ab211a2f132379a246157c3596d95fc42121541c"
        } else {
            CHECKPOINT
        }
    );
    let mut value = observed_body(&variant, &source, intervals, policy);
    if root {
        value["schema"] = json!("active-root-mask-after-v1");
        value["root_mask_checkpoint"] = json!("44068bd971dee62e2ed8cbaffbb2f18922283f83");
        value["root_mask_before"] = json!({
            "path":"tests/fixtures/root-mask-emission-baseline-v1/snapshot.json",
            "sha256":"afa6038a02c959c7192c09c08a89266c7d52151a5e59a4f69828fbdf67e7414e",
        });
        value["native_requests"]["scope"] = json!(
            "744 actual current native requests with funded typed AudioSample Root output-mask emission. Existing query intervals remain separate; native mask work is whole invocation, not a synthesized query fee."
        );
        value["scope"] = json!(
            "Actual native Root output suppression Vec, future/final Box and in-place merge stay charged while source queries remain live. C Point policy remains active. Projected-provider/derived policies, exhausted vectors, PCM/dependencies and whole preparation remain outside this claim."
        );
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &value).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}

fn observed_body(variant: &str, source: &str, intervals: bool, policy: bool) -> Value {
    let _capture = Capture::new();
    INTERVALS.set(intervals);
    let mut native =
        raw_domain_host_baseline::captured_body_with_point(variant, source, CHECKPOINT, true);
    native["schema"] = json!("active-internal-mask-native-request-set-v1");
    native["scope"] = json!(
        "744 actual active-runtime requests with literal outputs, work, capacities, errors and earlier query traces. Native mask work is one independent top-level invocation receipt, including nested calls, not per nested query attribution. Earlier invalid flags are preserved; public mask capacity alone does not prove account lifetime."
    );
    let internal =
        super::super::internal_suppression_baseline::current_owned_mask_observations_with_intervals(
            intervals,
        );
    let mut value = json!({
        "schema":"active-internal-mask-after-v1",
        "published_checkpoint":CHECKPOINT,
        "variant":variant,"source_sha256":source,
        "before_authorities":[
            {"path":"tests/fixtures/owned-suppression-native-baseline-v1/snapshot.json",
             "sha256":"3d6338e4e4b0f5617647ab6e0fa4d80bf8882a87c87a55efe306002c3011dbdc"},
            {"path":"tests/fixtures/internal-suppression-paths-baseline-v1/snapshot.json",
             "sha256":"e81a3ecd80ad0155e754e7a708e14efd6f045613deab797154ef0999b266e076"},
            {"path":"tests/fixtures/internal-transfer-entries-baseline-v1/snapshot.json",
             "sha256":"c2d5c271979cf93caceff43b729a9b8bca2434d1bd90ea9bdf9a694cca6d826d"}
        ],
        "native_requests":native,
        "internal_and_transfer_paths":internal,
        "scope":"Actual closed native mask destinations and mask flag scratch. Plan policy producers, PCM/dependencies, independently derived bus/equalizer policies and whole preparation remain outside this ownership claim. Ten internal and four public transfer cases observe real account/controller/output-drop lifetimes at their recorded points."
    });
    if intervals {
        value["schema"] = json!("active-query-interval-after-v1");
        value["observer_checkpoint"] = json!("ab211a2f132379a246157c3596d95fc42121541c");
        value["scope"] = json!(
            "Additive actual raw/Point query intervals with literal completion claims and independent native/host boundaries. Parent rows include child rows and cannot be summed. Partial intervals have no invented query debit. Old receipts and invalid flags remain literal; no whole preparation ownership claim."
        );
    }
    if policy {
        value["schema"] = json!("active-point-policy-after-v1");
        value["policy_checkpoint"] = json!("eedbc58272bdd009a65239b7104ddb786a2554fc");
        value["immediate_before"] = json!({
            "path":"tests/fixtures/point-policy-final-mask-baseline-v1/snapshot.json",
            "sha256":"af7a292718365c34fa03692e2cbe58a2cfb3c6d444b5ae29d52647e1fdbbff14",
        });
        value["native_requests"]["scope"] = json!(
            "744 actual current requests. Point Signal policy is admitted and transfers its exact final Vec lease; independent native phase receipts remain whole invocation and raw/Point content intervals remain separately observed."
        );
        value["scope"] = json!(
            "Current Point SignalSample PointCeil policy metadata and exact final Vec lease handoff. Root/derived bus/equalizer products and whole preparation remain outside this claim. Historical authorities remain literal; the immediate complete before body is separately named."
        );
        for row in value["internal_and_transfer_paths"]["cases"]
            .as_array_mut()
            .unwrap()
        {
            row["scope"]["internal_masks"] = json!(
                "actual native destinations and admitted Point Signal policy; independent projected-provider and derived policies remain ordinary"
            );
        }
    }
    value
}

#[test]
fn active_internal_masks_before_point_policy_match_frozen_v1() {
    let bytes =
        include_bytes!("../../tests/fixtures/point-policy-final-mask-baseline-v1/snapshot.json");
    assert_eq!(
        hex(Sha256::digest(bytes)),
        "af7a292718365c34fa03692e2cbe58a2cfb3c6d444b5ae29d52647e1fdbbff14",
    );
    let expected: Value = serde_json::from_slice(bytes).unwrap();
    let mut actual = crate::stages::point_policy_request::with_legacy_policy(|| {
        observed_body("scoped-published", &"0".repeat(64), false, false)
    });
    for field in ["variant", "source_sha256"] {
        actual.as_object_mut().unwrap().remove(field);
        actual["native_requests"]
            .as_object_mut()
            .unwrap()
            .remove(field);
    }
    assert_eq!(actual, expected);
}

#[test]
fn point_policy_before_root_emission_matches_frozen_v1() {
    let bytes = include_bytes!("../../tests/fixtures/root-mask-emission-baseline-v1/snapshot.json");
    assert_eq!(
        hex(Sha256::digest(bytes)),
        "afa6038a02c959c7192c09c08a89266c7d52151a5e59a4f69828fbdf67e7414e",
    );
    let expected: Value = serde_json::from_slice(bytes).unwrap();
    let mut actual = crate::stages::root_mask_request::with_legacy_root_masks(|| {
        observed_body("scoped-published", &"0".repeat(64), true, true)
    });
    for field in ["variant", "source_sha256"] {
        actual.as_object_mut().unwrap().remove(field);
        actual["native_requests"]
            .as_object_mut()
            .unwrap()
            .remove(field);
    }
    assert_eq!(actual, expected);
}
