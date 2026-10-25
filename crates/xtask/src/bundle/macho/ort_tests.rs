use super::*;

#[test]
fn runtime_resolution_is_closed_over_explicit_qualified_sources() {
    let prefix = Path::new("/qualified/ffmpeg/lib");
    let canonical = Path::new("/actual/ffmpeg/lib");
    for reference in [
        "/qualified/ffmpeg/lib/libavcodec.62.dylib",
        "/actual/ffmpeg/lib/libavutil.60.dylib",
    ] {
        assert_eq!(
            reference_kind(reference, prefix, canonical, true).unwrap(),
            ReferenceKind::Ffmpeg
        );
    }
    assert_eq!(
        reference_kind(ORT_INSTALL_NAME, prefix, canonical, true).unwrap(),
        ReferenceKind::Ort
    );
    assert!(reference_kind(ORT_INSTALL_NAME, prefix, canonical, false).is_err());
    for reference in [
        "@rpath/libonnxruntime.dylib",
        "@rpath/libonnxruntime.2.dylib",
        "@rpath/libavcodec.62.dylib",
        "@loader_path/libonnxruntime.1.dylib",
        "/opt/homebrew/lib/libonnxruntime.1.dylib",
        "/qualified/ffmpeg/lib/../../outside.dylib",
        "/qualified/ffmpeg/lib-lookalike/libx.dylib",
    ] {
        assert!(
            reference_kind(reference, prefix, canonical, true).is_err(),
            "{reference}"
        );
    }
}

#[test]
fn ort_transitive_dependencies_are_system_only_not_other_approved_runtimes() {
    let system = BTreeSet::from([
        "/usr/lib/libSystem.B.dylib".to_owned(),
        "/System/Library/Frameworks/CoreML.framework/Versions/A/CoreML".to_owned(),
    ]);
    ort_dependencies_are_system(&system).unwrap();
    for reference in [
        "@rpath/libonnxruntime_providers_shared.dylib",
        "/qualified/ffmpeg/lib/libavcodec.62.dylib",
        "/usr/lib/../local/libx.dylib",
    ] {
        let mut dependencies = system.clone();
        dependencies.insert(reference.into());
        assert!(ort_dependencies_are_system(&dependencies).is_err());
    }
}

#[test]
fn basename_aliases_require_same_actual_content() {
    let mut copied = BTreeMap::new();
    assert!(
        register_library(
            &mut copied,
            "libsame.1.dylib",
            Path::new("/one/libsame.1.dylib"),
            "known-bytes",
        )
        .unwrap()
    );
    assert!(
        !register_library(
            &mut copied,
            "libsame.1.dylib",
            Path::new("/alias/libsame.1.dylib"),
            "known-bytes",
        )
        .unwrap()
    );
    let error = register_library(
        &mut copied,
        "libsame.1.dylib",
        Path::new("/different/libsame.1.dylib"),
        "different-bytes",
    )
    .unwrap_err();
    assert!(error.contains("basename collision"));
}

#[test]
fn selected_ort_prefix_accepts_internal_aliases_and_rejects_escape_or_wrong_bytes() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("xtask-ort-source-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let library = root.join("prefix/lib");
    fs::create_dir_all(&library).unwrap();
    let real = library.join("libonnxruntime.1.31.0.dylib");
    fs::write(&real, b"unqualified stub, never a runtime").unwrap();
    let alias = library.join(ORT_LIBRARY_NAME);
    symlink(real.file_name().unwrap(), &alias).unwrap();
    assert_eq!(
        select_ort_source(&root.join("prefix")).unwrap(),
        fs::canonicalize(&real).unwrap()
    );
    assert!(qualified_ort(&root.join("prefix")).is_err());
    assert!(select_ort_source(Path::new("relative-prefix")).is_err());
    fs::remove_file(&alias).unwrap();
    let outside = root.join("outside.dylib");
    fs::write(&outside, b"outside").unwrap();
    symlink(&outside, &alias).unwrap();
    assert!(select_ort_source(&root.join("prefix")).is_err());
    fs::remove_dir_all(&root).unwrap();
}
