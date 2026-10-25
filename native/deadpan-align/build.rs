use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn verify(path: &Path, expected: &str) {
    println!("cargo:rerun-if-changed={}", path.display());
    let bytes = fs::read(path).expect("read explicitly selected ONNX Runtime build input");
    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    assert_eq!(
        digest,
        expected,
        "ONNX Runtime build input differs from the qualified pin: {}",
        path.display()
    );
}

fn main() {
    println!("cargo:rerun-if-env-changed=DEADPAN_ORT_PREFIX");
    println!("cargo:rerun-if-changed=src/adapter.cpp");
    println!("cargo:rerun-if-changed=src/adapter.h");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let prefix = env::var_os("DEADPAN_ORT_PREFIX")
        .map(PathBuf::from)
        .expect("DEADPAN_ORT_PREFIX must name the qualified ONNX Runtime 1.31.0 CPU prefix");
    assert!(prefix.is_absolute(), "DEADPAN_ORT_PREFIX must be absolute");
    let include = prefix.join("include");
    let library = prefix.join("lib");
    assert!(
        include.join("onnxruntime_c_api.h").is_file(),
        "missing ONNX Runtime headers"
    );
    assert!(
        library.join("libonnxruntime.dylib").is_file(),
        "missing ONNX Runtime CPU library"
    );
    for (name, digest) in [
        (
            "onnxruntime_c_api.h",
            "43d584f596ef66820421317df32a79a95fc1d39ee46cb3971a870c08d8ba4baa",
        ),
        (
            "onnxruntime_cxx_api.h",
            "1e80d494c47091b07d84de60ad570326f82ed841c6de66e5b3eb7dce17bfc467",
        ),
        (
            "onnxruntime_cxx_inline.h",
            "908a50414c3f67f32fd556cd16f7779fa78380e12e9d08e44f5e6f66b965a7be",
        ),
        (
            "onnxruntime_float16.h",
            "88b242845d25981633a0bbd1c148e273cf8bfb016ea3f57c4af41a06530f72b0",
        ),
        (
            "onnxruntime_ep_c_api.h",
            "b64036772115dbc6f06a900487ce22d34e862100db03e2ad62210758e27a19b2",
        ),
        (
            "onnxruntime_error_code.h",
            "5ce3b054e798eced8d14f5b86e98692fd33470463f96194ce0700a2d53dd8721",
        ),
    ] {
        verify(&include.join(name), digest);
    }
    verify(
        &library.join("libonnxruntime.dylib"),
        "9d2945b4de2ca594608f33b99498c8a81e6696e75da7ae9c67bc9ad3cc36c0d6",
    );
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .opt_level(2)
        .file("src/adapter.cpp")
        .include(include)
        .flag("-Wall")
        .flag("-Wextra")
        .warnings_into_errors(true)
        .compile("deadpan_align_adapter");
    println!("cargo:rustc-link-search=native={}", library.display());
    println!("cargo:rustc-link-lib=dylib=onnxruntime");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", library.display());
}
