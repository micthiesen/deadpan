use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION_LINE: &str = "ffprobe version 8.0.3 ";
const REQUIRED_CONFIGURATION: [&str; 4] = [
    "--disable-gpl",
    "--disable-nonfree",
    "--disable-version3",
    "--disable-network",
];
const FORBIDDEN_CONFIGURATION: [&str; 4] = [
    "--enable-gpl",
    "--enable-nonfree",
    "--enable-version3",
    "--enable-network",
];

fn required_directory(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    assert!(
        path.is_dir(),
        "required FFmpeg directory is missing: {}",
        path.display()
    );
    path
}

fn main() {
    println!("cargo:rerun-if-env-changed=DEADPAN_FFMPEG_PREFIX");
    println!("cargo:rerun-if-changed=src/encoder.c");
    println!("cargo:rerun-if-changed=src/encoder.h");
    println!("cargo:rerun-if-changed=src/runtime.c");
    println!("cargo:rerun-if-changed=src/runtime.h");

    let prefix = env::var_os("DEADPAN_FFMPEG_PREFIX")
        .map(PathBuf::from)
        .expect("DEADPAN_FFMPEG_PREFIX must name the qualified FFmpeg 8.0.3 prefix");
    assert!(
        prefix.is_absolute(),
        "DEADPAN_FFMPEG_PREFIX must be absolute"
    );
    let include = required_directory(&prefix, "include");
    let library = required_directory(&prefix, "lib");
    let ffprobe = prefix.join("bin/ffprobe");
    assert!(ffprobe.is_file(), "qualified FFmpeg prefix has no ffprobe");

    let output = Command::new(&ffprobe)
        .arg("-version")
        .output()
        .expect("run qualified ffprobe");
    assert!(output.status.success(), "qualified ffprobe -version failed");
    let version = String::from_utf8(output.stdout).expect("ffprobe version output is UTF-8");
    assert!(
        version.starts_with(VERSION_LINE),
        "FFmpeg must be exactly 8.0.3"
    );
    for (option, forbidden) in REQUIRED_CONFIGURATION
        .into_iter()
        .zip(FORBIDDEN_CONFIGURATION)
    {
        assert!(
            version.contains(option) && !version.contains(forbidden),
            "FFmpeg configuration violates required option {option}"
        );
    }

    cc::Build::new()
        .file("src/encoder.c")
        .file("src/runtime.c")
        .include(&include)
        .flag("-std=c11")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-Werror")
        .compile("deadpan_native_encoder");

    println!("cargo:rustc-link-search=native={}", library.display());
    for library_name in ["avformat", "avcodec", "avutil"] {
        println!("cargo:rustc-link-lib=dylib={library_name}");
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", library.display());
}
