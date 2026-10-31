//! Frozen native behavior before admitting fresh Bound raw-domain metadata.
use super::*;

const SHA256: &str = "2f57d0404dad1cb2d33054958d0578f67dd9e2ecc154a1d17fd2824d65700745";

fn expected() -> Value {
    let bytes = include_bytes!("../../tests/fixtures/raw-domain-host-baseline-v1/snapshot.json");
    assert_eq!(hex(Sha256::digest(bytes)), SHA256);
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn complete_pre_raw_native_body_keeps_current_fades_and_owners() {
    let expected = expected();
    raw_domain_query_trace::reset();
    let mut actual = raw_domain_request::with_legacy_raw_domain(|| {
        current_body("raw-domain-regression", &"0".repeat(64))
    });
    actual.as_object_mut().unwrap().remove("variant");
    actual.as_object_mut().unwrap().remove("source_sha256");
    assert_eq!(actual, expected, "complete frozen raw-domain caller body");
    let receipt = serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap();
    raw_domain_query_trace::disable();
    assert_eq!(receipt["invalid"], false);
    assert_eq!(receipt["overflow"], false);
    assert_eq!(receipt["nested"], false);
    assert_eq!(receipt["active_depth"], 0);
    let mut ordinary = 0;
    for lane in ["plan", "cache"] {
        assert_eq!(receipt[lane]["admitted_calls"], 0);
        assert_eq!(receipt[lane]["accepted_work"], 0);
        assert_eq!(receipt[lane]["partial_work"], 0);
        ordinary += receipt[lane]["ordinary_calls"].as_u64().unwrap();
    }
    assert!(ordinary > 0, "the narrow raw legacy scope actually ran");
}

thread_local! {
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
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
        raw_domain_query_trace::disable();
        CAPTURING.set(false);
    }
}
pub(super) fn reset_if_capturing() {
    if CAPTURING.get() {
        raw_domain_query_trace::reset();
    }
}
pub(super) fn receipt_if_capturing() -> Option<Value> {
    CAPTURING
        .get()
        .then(|| serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap())
}

#[test]
#[ignore = "manual admitted raw-domain capture; fresh /tmp output and pinned actual source required"]
fn capture_new_raw_domain_native_v1() {
    let path = std::env::var("DEADPAN_RAW_HOST_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&path);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_RAW_HOST_VARIANT").expect("actual source variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source =
        std::env::var("DEADPAN_RAW_HOST_SOURCE_SHA256").expect("actual complete source manifest");
    assert!(source.len() == 64 && source.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_RAW_HOST_PUBLISHED_CHECKPOINT")
        .expect("published preceding checkpoint");
    assert_eq!(checkpoint, "87ab6192e765a34dae9595139e10bd9010d6315e");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let value = {
        let _capture = Capture::new();
        let mut value = current_body(&variant, &source);
        value["schema"] = json!("raw-domain-native-admitted-v1");
        value["pre_raw_frozen_sha256"] = json!(SHA256);
        value["published_retained_fade_checkpoint"] = json!(checkpoint);
        value["scope"] = json!(
            "Complete fresh admitted raw-domain native results with separate scoped constructor receipts. The embedded pair comparison still names the earlier retained legacy reference; this unit compares paired.new against raw-domain-host-baseline-v1 paired.new. No whole recursive preparation claim."
        );
        value
    };
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &value).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}
