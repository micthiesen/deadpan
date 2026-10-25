use super::*;
use std::cell::RefCell;

const GOOD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let contents = root.join("Deadpan.app/Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("Frameworks")).unwrap();
    for name in ["MacOS/deadpan-cli", "MacOS/deadpan-app", WORKER, ORT] {
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
    (temp, contents.join("MacOS/deadpan-cli"))
}

#[test]
fn packaged_hosts_require_both_pins_and_cannot_downgrade_by_removing_the_marker() {
    let (_temp, current) = fixture();
    let hash = |_: &Path| Ok(GOOD.into());
    let verify = |_: &Path| Ok(());
    for pins in [(None, None), (Some(GOOD), None), (Some("bad"), Some(GOOD))] {
        assert!(qualify_with(&current, pins, hash, verify).is_err());
    }
    let contents = super::super::contents_of(&current).unwrap();
    for marker in [
        None,
        Some("<key>DeadpanPackaging</key><string>other</string>"),
    ] {
        match marker {
            None => fs::remove_file(contents.join("Info.plist")).unwrap(),
            Some(text) => fs::write(contents.join("Info.plist"), text).unwrap(),
        }
        assert!(qualify_with(&current, (Some(GOOD), Some(GOOD)), hash, verify).is_err());
    }
    assert!(
        qualify_with(
            Path::new("/tmp/deadpan-cli"),
            (Some(GOOD), Some(GOOD)),
            hash,
            verify
        )
        .is_err()
    );
    assert_eq!(
        qualify_with(Path::new("/tmp/deadpan-cli"), (None, None), hash, verify).unwrap(),
        Path::new("/tmp/deadpan-align")
    );
}

#[test]
fn both_images_are_independently_checked_before_any_signature_or_launch() {
    let (_temp, current) = fixture();
    for changed in [WORKER, ORT] {
        let verified = RefCell::new(Vec::new());
        let result = qualify_with(
            &current,
            (Some(GOOD), Some(GOOD)),
            |path| Ok(if path.ends_with(changed) { OTHER } else { GOOD }.into()),
            |path| {
                verified.borrow_mut().push(path.to_owned());
                Ok(())
            },
        );
        assert!(result.is_err(), "{changed}");
        assert!(verified.borrow().is_empty());
    }
}

#[test]
fn expected_signer_failure_refuses_each_image_and_success_checks_the_resource_seal() {
    let (_temp, current) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    let app = contents.parent().unwrap();
    let expected = [
        app.to_owned(),
        current.clone(),
        contents.join("MacOS/deadpan-app"),
        contents.join(WORKER),
        contents.join(ORT),
    ];
    for denied in &expected {
        assert!(
            qualify_with(
                &current,
                (Some(GOOD), Some(GOOD)),
                |_| Ok(GOOD.into()),
                |path| if path == denied {
                    Err(invalid("expected signer mismatch"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
    }
    let verified = RefCell::new(Vec::new());
    assert_eq!(
        qualify_with(
            &current,
            (Some(GOOD), Some(GOOD)),
            |_| Ok(GOOD.into()),
            |path| {
                verified.borrow_mut().push(path.to_owned());
                Ok(())
            }
        )
        .unwrap(),
        contents.join(WORKER)
    );
    assert_eq!(*verified.borrow(), expected);
}

#[test]
fn packaged_file_and_directory_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    for candidate in [WORKER, ORT, "MacOS/deadpan-app", "Info.plist"] {
        let (_temp, current) = fixture();
        let contents = super::super::contents_of(&current).unwrap();
        let original = contents.join(candidate);
        let retained = original.with_extension("retained");
        fs::rename(&original, &retained).unwrap();
        symlink(&retained, &original).unwrap();
        assert!(
            qualify_with(
                &current,
                (Some(GOOD), Some(GOOD)),
                |_| Ok(GOOD.into()),
                |_| Ok(())
            )
            .is_err(),
            "{candidate}"
        );
    }
    let (_temp, current) = fixture();
    let contents = super::super::contents_of(&current).unwrap();
    fs::rename(contents.join("Frameworks"), contents.join("elsewhere")).unwrap();
    symlink(contents.join("elsewhere"), contents.join("Frameworks")).unwrap();
    assert!(
        qualify_with(
            &current,
            (Some(GOOD), Some(GOOD)),
            |_| Ok(GOOD.into()),
            |_| Ok(())
        )
        .is_err()
    );
}

#[cfg(target_os = "macos")]
#[test]
fn real_system_signature_cannot_satisfy_a_different_signer_requirement() {
    assert!(signing::verify(Path::new("/bin/echo"), Some("anchor apple")).is_ok());
    assert!(
        signing::verify(
            Path::new("/bin/echo"),
            Some("anchor apple generic and certificate leaf[subject.OU] = \"DEADPANWRONG\"")
        )
        .is_err()
    );
}

#[test]
fn actual_content_reader_refuses_non_macho_and_symlink_files() {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("image");
    fs::write(&image, b"not a Mach-O image").unwrap();
    assert!(content(&image).is_err());
    std::os::unix::fs::symlink("/bin/echo", temp.path().join("alias")).unwrap();
    assert!(content(&temp.path().join("alias")).is_err());
}

#[test]
fn other_native_family_pins_cannot_enable_alignment_development_fallback() {
    assert!(
        qualify_using(
            Path::new("/tmp/deadpan-cli"),
            (None, None),
            true,
            |_, _| panic!("must refuse before image inspection"),
        )
        .is_err()
    );
}

#[test]
fn other_users_cannot_replace_package_owned_images_or_directories() {
    use std::os::unix::fs::PermissionsExt;
    for candidate in [WORKER, ORT, "MacOS", "Frameworks"] {
        let (_temp, current) = fixture();
        let contents = super::super::contents_of(&current).unwrap();
        fs::set_permissions(contents.join(candidate), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(
            qualify_with(
                &current,
                (Some(GOOD), Some(GOOD)),
                |_| Ok(GOOD.into()),
                |_| Ok(())
            )
            .is_err(),
            "{candidate}"
        );
    }
}
