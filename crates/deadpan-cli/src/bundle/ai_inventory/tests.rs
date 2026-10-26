use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "deadpan-ai-inventory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(root: &Path) -> io::Result<Inventory> {
    collect(root, || Ok(()), |bytes| Ok(digest(bytes)))
}

#[test]
fn generic_snapshot_pins_data_modes_tree_and_confined_leaf_aliases() {
    let scratch = Scratch::new();
    let root = &scratch.0;
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(root.join("lib/source.py"), b"print('pinned')").unwrap();
    fs::write(root.join("lib/cache.pyc"), b"bytecode").unwrap();
    fs::write(root.join("image"), b"\xcf\xfa\xed\xfecode").unwrap();
    symlink("lib/source.py", root.join("alias")).unwrap();
    symlink("../image", root.join("lib/image")).unwrap();
    let original = snapshot(root).unwrap();
    let bytes = canonical_bytes(&original).unwrap();
    assert_eq!(parse(&bytes).unwrap(), original);
    assert!(matches!(original.entries["image"], Entry::Macho { .. }));
    assert!(matches!(
        original.entries["lib/cache.pyc"],
        Entry::File { .. }
    ));
    fs::write(root.join("lib/cache.pyc"), b"substitute").unwrap();
    assert_ne!(snapshot(root).unwrap(), original);
    fs::write(root.join("lib/cache.pyc"), b"bytecode").unwrap();
    fs::write(root.join("extra.pth"), b"outside").unwrap();
    assert_ne!(snapshot(root).unwrap(), original);
    fs::remove_file(root.join("extra.pth")).unwrap();
    fs::set_permissions(root.join("image"), fs::Permissions::from_mode(0o700)).unwrap();
    assert_ne!(snapshot(root).unwrap(), original);
    fs::remove_file(root.join("lib/source.py")).unwrap();
    assert!(snapshot(root).is_err());
}

#[test]
fn parser_rejects_ambiguous_unknown_noncanonical_and_unbounded_records() {
    for bytes in [
        br#"{"schema":1,"root_mode":448,"entries":{},"extra":1}"#.as_slice(),
        br#"{"schema":1,"schema":1,"root_mode":448,"entries":{}}"#,
        br#"{"schema":1,"root_mode":448,"entries":{"x":{"kind":"directory","mode":448},"x":{"kind":"directory","mode":448}}}"#,
        br#"{"schema":1,"root_mode":448,"entries":{"x":{"kind":"directory","mode":448,"extra":1}}}"#,
        br#"{"schema":1,"root_mode":448,"entries":{"x":{"kind":"directory","mode":448,"mode":448}}}"#,
        br#"{ "schema":1,"root_mode":448,"entries":{}}"#,
    ] { assert!(parse(bytes).is_err(), "{}", String::from_utf8_lossy(bytes)); }
    let base = Inventory {
        schema: 1,
        root_mode: 0o700,
        entries: BTreeMap::new(),
    };
    for path in [
        "/outside",
        "../outside",
        "a/../b",
        "a//b",
        "a/./b",
        "a/b",
        "",
    ] {
        let mut bad = base.clone();
        bad.entries
            .insert(path.into(), Entry::Directory { mode: 0o700 });
        assert!(canonical_bytes(&bad).is_err(), "{path}");
    }
    for mode in [0o777, 0o775, 0o1700, 0o4700] {
        let mut bad = base.clone();
        bad.root_mode = mode;
        assert!(canonical_bytes(&bad).is_err());
    }
    let mut bad = base;
    bad.entries.insert(
        "large".into(),
        Entry::File {
            mode: 0o600,
            bytes: MAX_FILE_BYTES + 1,
            sha256: digest(b""),
        },
    );
    assert!(canonical_bytes(&bad).is_err());
    assert!(parse(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
}

#[test]
fn aliases_cannot_escape_cycle_dangle_or_replace_directory_parents() {
    for target in [
        "/outside",
        "../outside",
        "missing",
        "directory",
        "alias",
        "./file",
        "file/",
    ] {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("file"), b"safe").unwrap();
        fs::create_dir(scratch.0.join("directory")).unwrap();
        symlink(target, scratch.0.join("alias")).unwrap();
        assert!(snapshot(&scratch.0).is_err(), "{target}");
    }
    let scratch = Scratch::new();
    symlink("second", scratch.0.join("first")).unwrap();
    symlink("first", scratch.0.join("second")).unwrap();
    assert!(snapshot(&scratch.0).is_err());
    let mut entries = BTreeMap::new();
    entries.insert(
        "alias".into(),
        Entry::Symlink {
            target: "file".into(),
        },
    );
    entries.insert(
        "file".into(),
        Entry::File {
            mode: 0o600,
            bytes: 0,
            sha256: digest(b""),
        },
    );
    entries.insert("alias/child".into(), Entry::Directory { mode: 0o700 });
    assert!(
        canonical_bytes(&Inventory {
            schema: 1,
            root_mode: 0o700,
            entries
        })
        .is_err()
    );
}

#[test]
fn shared_writes_special_files_and_root_aliases_fail_without_following() {
    let scratch = Scratch::new();
    let file = scratch.0.join("file");
    fs::write(&file, b"safe").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(snapshot(&scratch.0).is_err());
    fs::remove_file(&file).unwrap();
    let socket = std::os::unix::net::UnixListener::bind(&file).unwrap();
    assert!(snapshot(&scratch.0).is_err());
    drop(socket);
    fs::remove_file(&file).unwrap();
    symlink(&scratch.0, &file).unwrap();
    assert!(snapshot(&file).is_err());
}

#[test]
fn real_chunk_reads_and_manifest_validation_observe_interruption() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("large"), vec![42; 2 * 1024 * 1024]).unwrap();
    let mut checks = 0;
    let error = collect(
        &scratch.0,
        || {
            checks += 1;
            if checks >= 12 {
                Err(io::Error::new(io::ErrorKind::Interrupted, "cancel"))
            } else {
                Ok(())
            }
        },
        |bytes| Ok(digest(bytes)),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(checks, 12);
    let inventory = snapshot(&scratch.0).unwrap();
    let error = validate_with(&inventory, || {
        Err(io::Error::new(io::ErrorKind::TimedOut, "deadline"))
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn unsupported_macho_layout_cannot_become_an_ordinary_file() {
    for magic in [
        [0xce, 0xfa, 0xed, 0xfe],
        [0xfe, 0xed, 0xfa, 0xce],
        [0xca, 0xfe, 0xba, 0xbf],
        [0xbf, 0xba, 0xfe, 0xca],
    ] {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("image"), magic).unwrap();
        let error = collect(
            &scratch.0,
            || Ok(()),
            |_| Err(invalid("unsupported Mach-O")),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unsupported Mach-O"));
    }
}
