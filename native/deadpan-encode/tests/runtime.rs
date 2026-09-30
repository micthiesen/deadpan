use deadpan_encode::runtime::{
    MappedImageIdentity, RuntimeFileTime, RuntimeImageKind, RuntimePlatform,
};

fn identity() -> MappedImageIdentity {
    MappedImageIdentity {
        kind: RuntimeImageKind::Avcodec,
        device: 1,
        inode: 2,
        uuid: [3; 16],
        file_size: 4096,
        modification_time: RuntimeFileTime {
            seconds: 100,
            nanoseconds: 200,
        },
        change_time: RuntimeFileTime {
            seconds: 300,
            nanoseconds: 400,
        },
        birth_time: RuntimeFileTime {
            seconds: 10,
            nanoseconds: 20,
        },
        generation: 0,
    }
}

#[test]
fn runtime_identity_wire_is_closed_and_retains_exact_mapped_facts() {
    let expected = identity();
    expected.validate().unwrap();
    let bytes = serde_json::to_vec(&expected).unwrap();
    let decoded: MappedImageIdentity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, expected);
    for field in ["path", "address", "loaded_bytes_sha256"] {
        let mut value = serde_json::to_value(&expected).unwrap();
        value[field] = serde_json::json!("invented");
        assert!(serde_json::from_value::<MappedImageIdentity>(value).is_err());
    }
    assert!(serde_json::from_str::<RuntimeImageKind>("\"unknown_library\"").is_err());
    let mut value = serde_json::to_value(&expected).unwrap();
    value["modification_time"]["extra"] = serde_json::json!(1);
    assert!(serde_json::from_value::<MappedImageIdentity>(value).is_err());
}

#[test]
fn empty_and_out_of_range_identity_claims_fail_validation() {
    for field in 0..8 {
        let mut value = identity();
        match field {
            0 => value.device = 0,
            1 => value.device = u64::from(u32::MAX) + 1,
            2 => value.inode = 0,
            3 => value.uuid = [0; 16],
            4 => value.file_size = 0,
            5 => value.file_size = u64::MAX,
            6 => value.change_time.nanoseconds = 1_000_000_000,
            _ => value.birth_time.nanoseconds = u32::MAX,
        }
        assert!(value.validate().is_err());
    }
    let mut changed = identity();
    changed.modification_time.nanoseconds += 1;
    assert_ne!(changed, identity());
    changed = identity();
    changed.generation += 1;
    assert_ne!(changed, identity());
}

#[test]
fn platform_wire_contains_only_bounded_nonunique_platform_facts() {
    let platform = RuntimePlatform {
        os_build: "25F84".into(),
        hardware_model: "Mac17,6".into(),
        cpu_family: 0x1234,
    };
    platform.validate().unwrap();
    let json = serde_json::to_value(&platform).unwrap();
    assert_eq!(
        serde_json::from_value::<RuntimePlatform>(json.clone()).unwrap(),
        platform
    );
    for field in ["hostname", "serial_number", "environment"] {
        let mut value = json.clone();
        value[field] = serde_json::json!("forbidden");
        assert!(serde_json::from_value::<RuntimePlatform>(value).is_err());
    }
    for text in [
        "".to_owned(),
        "x".repeat(64),
        "model\nsecret".to_owned(),
        "https://example.com".to_owned(),
    ] {
        let mut value = platform.clone();
        value.hardware_model = text;
        assert!(value.validate().is_err());
    }
    let mut value = platform;
    value.cpu_family = 0;
    assert!(value.validate().is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn fresh_loaded_images_match_pinned_descriptors_without_changing_their_cursor() {
    use deadpan_encode::runtime::{RuntimeImageObservation, observe_platform};
    use std::io::{Seek, SeekFrom};
    use std::os::unix::fs::MetadataExt;

    observe_platform().unwrap().validate().unwrap();
    // Swscale is required by capture_current for the real helper. This encoder
    // crate's test executable need not link the source decoder's swscale.
    for kind in [
        RuntimeImageKind::Helper,
        RuntimeImageKind::Avcodec,
        RuntimeImageKind::Avformat,
        RuntimeImageKind::Avutil,
    ] {
        let observation = RuntimeImageObservation::capture(kind).unwrap();
        let mut file = observation.open_matching().unwrap();
        let metadata = file.metadata().unwrap();
        assert_eq!(metadata.dev(), observation.identity().device);
        assert_eq!(metadata.ino(), observation.identity().inode);
        assert_eq!(metadata.len(), observation.identity().file_size);
        assert_eq!(file.stream_position().unwrap(), 0);
        file.seek(SeekFrom::Start(17)).unwrap();
        observation.revalidate(&file).unwrap();
        assert_eq!(file.stream_position().unwrap(), 17);
        assert_eq!(
            RuntimeImageObservation::capture(kind).unwrap().identity(),
            observation.identity()
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn copied_loaded_header_cannot_make_another_vnode_a_loaded_image() {
    use deadpan_encode::runtime::RuntimeImageObservation;
    use std::io::{Read, Write};

    let observation = RuntimeImageObservation::capture(RuntimeImageKind::Avcodec).unwrap();
    let mut source = observation.open_matching().unwrap();
    let mut prefix = [0_u8; 4096];
    source.read_exact(&mut prefix).unwrap();
    let mut replacement = tempfile::tempfile().unwrap();
    replacement.write_all(&prefix).unwrap();
    replacement
        .set_len(observation.identity().file_size)
        .unwrap();
    replacement.sync_all().unwrap();
    // Even a copied header/UUID and matching length are not the mapped vnode.
    assert!(observation.revalidate(&replacement).is_err());
    observation.revalidate(&source).unwrap();
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_platform_never_synthesizes_loaded_identity() {
    use deadpan_encode::runtime::{RuntimeImageObservation, capture_current, observe_platform};
    assert!(RuntimeImageObservation::capture(RuntimeImageKind::Helper).is_err());
    assert!(capture_current().is_err());
    assert!(observe_platform().is_err());
}
