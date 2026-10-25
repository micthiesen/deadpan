use super::*;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

const GOOD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn caller_interruption_precedes_runtime_selection_even_in_development() {
    for (cancelled, deadline, expected) in [
        (
            true,
            Instant::now() + Duration::from_secs(5),
            io::ErrorKind::Interrupted,
        ),
        (false, Instant::now(), io::ErrorKind::TimedOut),
    ] {
        let cancelled = AtomicBool::new(cancelled);
        assert_eq!(
            qualify_native_worker_interruptible(
                NativeWorker::Media,
                Path::new("/no/native-worker"),
                &cancelled,
                deadline
            )
            .unwrap_err()
            .kind(),
            expected
        );
    }
}

fn pins() -> BTreeMap<String, String> {
    NativeWorker::ALL
        .into_iter()
        .map(NativeWorker::relative_path)
        .chain(LIBRARIES)
        .map(|name| (name.into(), GOOD.into()))
        .collect()
}

fn fixture() -> (tempfile::TempDir, PathBuf, String) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let contents = root.join("Deadpan.app/Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("Frameworks")).unwrap();
    for name in pins()
        .keys()
        .map(String::as_str)
        .chain(["MacOS/deadpan-cli", "MacOS/deadpan-app"])
    {
        fs::write(contents.join(name), b"image").unwrap();
    }
    fs::write(
        contents.join("Info.plist"),
        format!(
            "<plist version=\"1.0\"><dict><key>{}</key><string>{}</string></dict></plist>",
            super::super::PACKAGING_KEY,
            super::super::PACKAGING_LAYOUT
        ),
    )
    .unwrap();
    (
        temp,
        contents.join("MacOS/deadpan-cli"),
        serde_json::to_string(&pins()).unwrap(),
    )
}

fn qualify(
    current: &Path,
    kind: NativeWorker,
    executable: &Path,
    source: Option<&str>,
) -> io::Result<()> {
    qualify_with(
        current,
        kind,
        executable,
        source,
        false,
        |current, images| image::qualify_with(current, images, |_| Ok(GOOD.into()), |_| Ok(())),
    )
}

#[test]
fn complete_compiled_map_is_required_and_rejects_paths_or_hashes_from_a_manifest() {
    let original = pins();
    let encoded = serde_json::to_string(&original).unwrap();
    assert_eq!(parse_pins(Some(&encoded)).unwrap(), Some(original.clone()));
    for name in original.keys() {
        let mut removed = original.clone();
        removed.remove(name);
        assert!(
            parse_pins(Some(&serde_json::to_string(&removed).unwrap())).is_err(),
            "{name}"
        );
        let mut changed = original.clone();
        changed.insert(name.clone(), "not-a-hash".into());
        assert!(
            parse_pins(Some(&serde_json::to_string(&changed).unwrap())).is_err(),
            "{name}"
        );
    }
    for path in ["MacOS/../foreign", "/tmp/foreign", "Frameworks/extra.dylib"] {
        let mut extra = original.clone();
        extra.insert(path.into(), GOOD.into());
        assert!(parse_pins(Some(&serde_json::to_string(&extra).unwrap())).is_err());
    }
    for source in ["{}", "[]", "null", "{", &"x".repeat(MAX_PINS_BYTES + 1)] {
        assert!(parse_pins(Some(source)).is_err());
    }
}

#[test]
fn packaged_worker_role_cannot_select_another_worker_or_external_executable() {
    let (_temp, current, source) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    for kind in NativeWorker::ALL {
        let correct = contents.join(kind.relative_path());
        assert!(qualify(&current, kind, &correct, Some(&source)).is_ok());
        assert!(qualify(&current, kind, &correct, None).is_err());
        assert!(qualify(&current, kind, Path::new("/bin/echo"), Some(&source)).is_err());
        for other in NativeWorker::ALL {
            if kind != other {
                assert!(
                    qualify(
                        &current,
                        kind,
                        &contents.join(other.relative_path()),
                        Some(&source)
                    )
                    .is_err()
                );
            }
        }
    }
}

#[test]
fn any_family_of_compiled_pins_prevents_development_downgrade() {
    let (_temp, current, source) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    fs::remove_file(contents.join("Info.plist")).unwrap();
    let supplied = contents.join(NativeWorker::Media.relative_path());
    assert!(qualify(&current, NativeWorker::Media, &supplied, Some(&source)).is_err());
    assert!(
        qualify(
            Path::new("/tmp/deadpan-cli"),
            NativeWorker::Media,
            &supplied,
            Some(&source)
        )
        .is_err()
    );
    assert!(
        qualify_with(
            &current,
            NativeWorker::Media,
            &supplied,
            None,
            true,
            |_, _| panic!("must refuse before inspection")
        )
        .is_err()
    );
    assert!(
        qualify(
            Path::new("/tmp/deadpan-cli"),
            NativeWorker::Media,
            Path::new("/tmp/fake-worker"),
            None
        )
        .is_ok()
    );
}

#[test]
fn every_native_worker_and_every_library_has_an_independent_pin() {
    let (_temp, current, source) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    for kind in NativeWorker::ALL {
        let selected = contents.join(kind.relative_path());
        for changed in std::iter::once(kind.relative_path()).chain(LIBRARIES) {
            let verified = RefCell::new(Vec::new());
            assert!(
                qualify_with(
                    &current,
                    kind,
                    &selected,
                    Some(&source),
                    true,
                    |current, images| {
                        image::qualify_with(
                            current,
                            images,
                            |path| Ok(if path.ends_with(changed) { OTHER } else { GOOD }.into()),
                            |path| {
                                verified.borrow_mut().push(path.to_owned());
                                Ok(())
                            },
                        )
                    }
                )
                .is_err(),
                "{changed}"
            );
            assert!(verified.borrow().is_empty());
        }
    }
}

#[test]
fn mutable_resource_manifest_cannot_authorize_substitution() {
    let (_temp, current, source) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    fs::create_dir(contents.join("Resources")).unwrap();
    fs::write(
        contents.join("Resources/native-pins.json"),
        source.replace(GOOD, OTHER),
    )
    .unwrap();
    let selected = contents.join(NativeWorker::Track.relative_path());
    assert!(
        qualify_with(
            &current,
            NativeWorker::Track,
            &selected,
            Some(&source),
            true,
            |current, images| {
                image::qualify_with(
                    current,
                    images,
                    |_| Ok(OTHER.into()),
                    |_| panic!("content must refuse first"),
                )
            }
        )
        .is_err()
    );
}

#[test]
fn expected_signer_is_checked_for_resource_seal_hosts_and_every_admitted_image() {
    let (_temp, current, source) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    let selected = contents.join(NativeWorker::Media.relative_path());
    let expected = [
        contents.parent().unwrap().to_owned(),
        current.clone(),
        contents.join("MacOS/deadpan-app"),
    ]
    .into_iter()
    .chain(
        std::iter::once(NativeWorker::Media.relative_path())
            .chain(LIBRARIES)
            .map(|name| contents.join(name)),
    )
    .collect::<Vec<_>>();
    for denied in &expected {
        assert!(
            qualify_with(
                &current,
                NativeWorker::Media,
                &selected,
                Some(&source),
                true,
                |current, images| {
                    image::qualify_with(
                        current,
                        images,
                        |_| Ok(GOOD.into()),
                        |path| {
                            if path == denied {
                                Err(invalid("wrong expected signer"))
                            } else {
                                Ok(())
                            }
                        },
                    )
                }
            )
            .is_err(),
            "{}",
            denied.display()
        );
    }
    let verified = RefCell::new(Vec::new());
    qualify_with(
        &current,
        NativeWorker::Media,
        &selected,
        Some(&source),
        true,
        |current, images| {
            image::qualify_with(
                current,
                images,
                |_| Ok(GOOD.into()),
                |path| {
                    verified.borrow_mut().push(path.to_owned());
                    Ok(())
                },
            )
        },
    )
    .unwrap();
    assert_eq!(*verified.borrow(), expected);
}

#[test]
fn private_regular_package_paths_are_required_before_hashing() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for name in [
        NativeWorker::Media.relative_path(),
        LIBRARIES[0],
        "Info.plist",
        "MacOS/deadpan-app",
        "Frameworks",
    ] {
        for alias in [false, true] {
            let (_temp, current, source) = fixture();
            let contents = super::super::contents_of(&current).unwrap();
            let path = contents.join(name);
            if alias {
                let retained = path.with_extension("retained");
                fs::rename(&path, &retained).unwrap();
                symlink(&retained, &path).unwrap();
            } else {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
            }
            assert!(
                qualify(
                    &current,
                    NativeWorker::Media,
                    &contents.join(NativeWorker::Media.relative_path()),
                    Some(&source)
                )
                .is_err(),
                "{name} alias={alias}"
            );
        }
    }
}
