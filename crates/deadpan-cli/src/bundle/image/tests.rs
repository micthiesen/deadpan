use super::*;
use std::cell::Cell;

const GOOD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn even_one_empty_or_partial_pin_variable_requires_a_package() {
    assert!(!any_runtime_pins([None, None, None]));
    for index in 0..3 {
        for value in ["", GOOD] {
            let mut pins = [None, None, None];
            pins[index] = Some(value);
            assert!(any_runtime_pins(pins));
        }
    }
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let contents = fs::canonicalize(temp.path())
        .unwrap()
        .join("Deadpan.app/Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    for name in ["deadpan-cli", "deadpan-app", "worker"] {
        fs::write(contents.join("MacOS").join(name), b"image").unwrap();
    }
    fs::write(
        contents.join("Info.plist"),
        "<plist version=\"1.0\"><dict><key>DeadpanPackaging</key><string>xtask-bundle-1</string></dict></plist>",
    )
    .unwrap();
    (temp, contents.join("MacOS/deadpan-cli"))
}

#[test]
fn expired_or_cancelled_admission_never_opens_the_image_or_package() {
    for (cancelled, deadline, expected) in [
        (
            true,
            Instant::now() + Duration::from_secs(5),
            io::ErrorKind::Interrupted,
        ),
        (false, Instant::now(), io::ErrorKind::TimedOut),
    ] {
        let flag = AtomicBool::new(cancelled);
        let control = Control::new(&flag, deadline);
        let path = Path::new("/no/native-image");
        assert_eq!(
            content_interruptible(path, control).unwrap_err().kind(),
            expected
        );
        assert_eq!(
            qualify_with_control(path, &[], control, |_| panic!("hash"), |_| panic!("signer"))
                .unwrap_err()
                .kind(),
            expected
        );
    }
}

#[test]
fn cancellation_during_a_hash_prevents_signer_work_and_later_admission() {
    let (_temp, current) = fixture();
    let flag = AtomicBool::new(false);
    let control = Control::new(&flag, Instant::now() + Duration::from_secs(5));
    let error = qualify_with_control(
        &current,
        &[("MacOS/worker", GOOD)],
        control,
        |_| {
            flag.store(true, Ordering::Release);
            Ok(GOOD.into())
        },
        |_| panic!("must not inspect signer after cancellation"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn cancellation_is_rechecked_between_signature_commands() {
    let (_temp, current) = fixture();
    let flag = AtomicBool::new(false);
    let control = Control::new(&flag, Instant::now() + Duration::from_secs(5));
    let checks = Cell::new(0);
    let error = qualify_with_control(
        &current,
        &[("MacOS/worker", GOOD)],
        control,
        |_| Ok(GOOD.into()),
        |_| {
            checks.set(checks.get() + 1);
            flag.store(true, Ordering::Release);
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(checks.get(), 1);
}
