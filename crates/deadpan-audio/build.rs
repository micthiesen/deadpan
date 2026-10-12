use std::path::{Path, PathBuf};

fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("audio engine sources") {
        let entry = entry.expect("audio engine source");
        let path = entry.path();
        if entry
            .file_type()
            .expect("audio engine source type")
            .is_dir()
        {
            if !matches!(
                entry.file_name().to_str(),
                Some("tests" | "target" | "examples" | "benches")
            ) {
                collect(&path, files);
            }
        } else {
            files.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest"));
    let root = manifest.join("../..").canonicalize().expect("workspace");
    let mut files = vec![root.join("Cargo.lock"), root.join("rust-toolchain.toml")];
    for path in [
        "crates/deadpan-audio",
        "crates/deadpan-plan",
        "crates/deadpan-core",
        "crates/deadpan-media",
        "native/deadpan-dsp",
        "native/deadpan-source",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
        collect(&root.join(path), &mut files);
    }
    files.sort();
    let mut hash = blake3::Hasher::new();
    hash.update(b"deadpan-persistent-pcm-engine-1");
    for name in [
        "TARGET",
        "PROFILE",
        "CARGO_CFG_TARGET_FEATURE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CXXFLAGS",
        "CFLAGS",
        "CC",
        "CXX",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        let value = std::env::var(name).unwrap_or_default();
        hash.update(&(value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .strip_prefix(&root)
            .expect("source within workspace")
            .to_string_lossy();
        let bytes = std::fs::read(&path).expect("read audio engine source");
        hash.update(&(name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    println!(
        "cargo:rustc-env=DEADPAN_PCM_ENGINE_PIN={}",
        hash.finalize().to_hex()
    );
}
