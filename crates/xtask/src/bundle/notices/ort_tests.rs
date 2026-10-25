use super::*;

#[test]
fn qualified_wheel_notices_are_exact_and_required_even_without_a_manifest_claim() {
    let workspace = super::super::workspace_root();
    let mut notices = vendored_notices(&workspace).unwrap();
    require_ort_notices(&notices).unwrap();
    notices.retain(|notice| notice.path != "onnxruntime/ThirdPartyNotices.txt");
    assert!(require_ort_notices(&notices).is_err());
    let mut changed = vendored_notices(&workspace).unwrap();
    let license = changed
        .iter_mut()
        .find(|notice| notice.path == "onnxruntime/LICENSE")
        .unwrap();
    // A readable, legitimate notice from another component is still not the
    // independently compiled ORT notice pin.
    license.file = workspace.join("packaging/notices/ffmpeg/LICENSE.md");
    assert!(require_ort_notices(&changed).is_err());
}

#[test]
fn ort_sbom_distinguishes_original_runtime_wheel_and_signed_shipped_bytes() {
    let shipped = "a".repeat(64);
    let component = ort_component(&shipped);
    assert_eq!(component["bom-ref"], "native:onnxruntime");
    assert_eq!(component["version"], "1.31.0");
    assert_eq!(component["hashes"][0]["content"], shipped);
    assert_eq!(component["licenses"][0]["license"]["id"], "MIT");
    let properties = component["properties"].as_array().unwrap();
    for (name, expected) in [
        (
            "deadpan:upstream-library-sha256",
            super::super::macho::ORT_SOURCE_SHA256,
        ),
        ("deadpan:source-wheel-sha256", ORT_WHEEL_SHA256),
        ("deadpan:notice", "onnxruntime/LICENSE"),
        (
            "deadpan:third-party-notice",
            "onnxruntime/ThirdPartyNotices.txt",
        ),
    ] {
        assert!(
            properties
                .iter()
                .any(|item| item["name"] == name && item["value"] == expected)
        );
    }
    assert!(SHIPPED_PACKAGES.contains(&"deadpan-align"));
    assert_eq!(SHIPPED_PACKAGES.len(), 6);
}
