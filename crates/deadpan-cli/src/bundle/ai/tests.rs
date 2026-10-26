use super::*;
use std::cell::Cell;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::time::Duration;

struct Fixture {
    _temp: tempfile::TempDir,
    current: PathBuf,
    root: PathBuf,
    python: PathBuf,
    worker: PathBuf,
    source: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    manifest: PathBuf,
    pin: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let contents = fs::canonicalize(temp.path())
            .unwrap()
            .join("Deadpan.app/Contents");
        let root = contents.join(AI_RUNTIME_DIRECTORY);
        for path in ["python/bin", "worker", "ltx-2-mlx/src", "bin", "python/lib"] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::write(contents.join("MacOS/deadpan-cli"), b"host").unwrap();
        fs::write(contents.join("Info.plist"), "<plist version=\"1.0\"><dict><key>DeadpanPackaging</key><string>xtask-bundle-1</string></dict></plist>").unwrap();
        for path in [
            "python/bin/python3.12",
            "bin/ffmpeg",
            "bin/ffprobe",
            "python/lib/extension.so",
        ] {
            fs::write(
                root.join(path),
                crate::youtube::macho_content::tests::synthetic(path.as_bytes(), 64),
            )
            .unwrap();
        }
        for (path, bytes) in [
            ("worker/worker.py", b"import pinned".as_slice()),
            ("ltx-2-mlx/src/module.py", b"safe = True"),
            ("python/lib/cache.pyc", b"bytecode"),
            ("python/lib/runtime.pth", b"../../ltx-2-mlx/src"),
            ("python/lib/mlx.metallib", b"metal"),
            ("runtime.json", b"{}"),
        ] {
            fs::write(root.join(path), bytes).unwrap();
        }
        symlink("python3.12", root.join("python/bin/python3")).unwrap();
        let inventory = ai_inventory::collect(
            &root,
            || Ok(()),
            |bytes| crate::youtube::macho_content::content_sha256(bytes).map_err(invalid),
        )
        .unwrap();
        let bytes = ai_inventory::canonical_bytes(&inventory).unwrap();
        let manifest = contents.join(AI_RUNTIME_INVENTORY);
        fs::write(&manifest, &bytes).unwrap();
        Self {
            current: contents.join("MacOS/deadpan-cli"),
            python: root.join("python/bin/python3.12"),
            worker: root.join("worker/worker.py"),
            source: root.join("ltx-2-mlx"),
            ffmpeg: root.join("bin/ffmpeg"),
            ffprobe: root.join("bin/ffprobe"),
            root,
            manifest,
            pin: ai_inventory::digest(&bytes),
            _temp: temp,
        }
    }

    fn paths(&self) -> AiRuntimePaths<'_> {
        AiRuntimePaths {
            python: &self.python,
            worker_script: &self.worker,
            runtime_source: &self.source,
            ffmpeg: &self.ffmpeg,
            ffprobe: &self.ffprobe,
        }
    }

    fn run(&self) -> io::Result<()> {
        let cancelled = AtomicBool::new(false);
        qualify_with(
            &self.current,
            self.paths(),
            Some(&self.pin),
            true,
            Control::new(&cancelled, Instant::now() + Duration::from_secs(5)),
            |_, images, _| {
                assert_eq!(images.len(), 4);
                assert!(images.iter().all(
                    |(path, hash)| path.starts_with("Resources/ai-runtime/") && valid_pin(hash)
                ));
                Ok(())
            },
        )
    }
}

#[test]
fn closed_runtime_admission_covers_import_data_and_every_native_image() {
    Fixture::new().run().unwrap();
    for path in [
        "worker/worker.py",
        "ltx-2-mlx/src/module.py",
        "python/lib/cache.pyc",
        "python/lib/runtime.pth",
        "python/lib/mlx.metallib",
        "runtime.json",
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join(path), b"substitution").unwrap();
        assert!(fixture.run().is_err(), "{path}");
    }
    for path in [
        "python/bin/python3.12",
        "bin/ffmpeg",
        "bin/ffprobe",
        "python/lib/extension.so",
    ] {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join(path),
            crate::youtube::macho_content::tests::synthetic(b"foreign", 64),
        )
        .unwrap();
        assert!(fixture.run().is_err(), "{path}");
    }
}

#[test]
fn edited_manifest_extra_file_missing_file_and_changed_alias_cannot_supply_authority() {
    let fixture = Fixture::new();
    fs::write(&fixture.manifest, b"{}").unwrap();
    assert!(fixture.run().is_err());
    let fixture = Fixture::new();
    fs::write(fixture.root.join("python/lib/injected.pth"), b"outside").unwrap();
    assert!(fixture.run().is_err());
    let fixture = Fixture::new();
    fs::remove_file(&fixture.worker).unwrap();
    assert!(fixture.run().is_err());
    let fixture = Fixture::new();
    let alias = fixture.root.join("python/bin/python3");
    fs::remove_file(&alias).unwrap();
    symlink("../../bin/ffmpeg", alias).unwrap();
    assert!(fixture.run().is_err());
}

#[test]
fn fixed_paths_reject_external_substitution_sibling_swap_and_alias_spelling() {
    for substitute in ["bin/ffprobe", "python/bin/python3", "../foreign"] {
        let fixture = Fixture::new();
        let path = fixture.root.join(substitute);
        let mut paths = fixture.paths();
        paths.python = &path;
        let cancelled = AtomicBool::new(false);
        let called = Cell::new(false);
        assert!(
            qualify_with(
                &fixture.current,
                paths,
                Some(&fixture.pin),
                true,
                Control::new(&cancelled, Instant::now() + Duration::from_secs(5)),
                |_, _, _| {
                    called.set(true);
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!called.get());
    }
}

#[test]
fn compiled_pin_and_package_marker_are_required_before_any_verifier_runs() {
    let fixture = Fixture::new();
    let cancelled = AtomicBool::new(false);
    for pin in [None, Some(""), Some("not-a-hash")] {
        assert!(
            qualify_with(
                &fixture.current,
                fixture.paths(),
                pin,
                true,
                Control::new(&cancelled, Instant::now() + Duration::from_secs(5)),
                |_, _, _| panic!("untrusted inventory reached signer")
            )
            .is_err()
        );
    }
    fs::remove_file(
        fixture
            .current
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("Info.plist"),
    )
    .unwrap();
    assert!(fixture.run().is_err());
    assert!(
        qualify_with(
            Path::new("/tmp/deadpan-cli"),
            fixture.paths(),
            Some(&fixture.pin),
            false,
            Control::new(&cancelled, Instant::now() + Duration::from_secs(5)),
            |_, _, _| panic!("escaped host reached signer")
        )
        .is_err()
    );
    qualify_with(
        Path::new("/tmp/deadpan-cli"),
        fixture.paths(),
        None,
        false,
        Control::new(&cancelled, Instant::now() + Duration::from_secs(5)),
        |_, _, _| panic!("development reached signer"),
    )
    .unwrap();
}

#[test]
fn expected_signer_failure_is_propagated_after_inventory_qualification() {
    let fixture = Fixture::new();
    let cancelled = AtomicBool::new(false);
    let error = qualify_with(
        &fixture.current,
        fixture.paths(),
        Some(&fixture.pin),
        true,
        Control::new(&cancelled, Instant::now() + Duration::from_secs(5)),
        |_, images, _| {
            assert!(
                images
                    .iter()
                    .any(|(path, _)| path.ends_with("extension.so"))
            );
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "foreign signer",
            ))
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn cancellation_and_deadline_precede_any_development_or_packaged_access() {
    let fixture = Fixture::new();
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
            qualify_ai_runtime(fixture.paths(), &cancelled, deadline)
                .unwrap_err()
                .kind(),
            expected
        );
        assert_eq!(
            qualify_ai_host(&cancelled, deadline).unwrap_err().kind(),
            expected
        );
    }
}
