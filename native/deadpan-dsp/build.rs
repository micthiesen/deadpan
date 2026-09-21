fn main() {
    println!("cargo:rerun-if-changed=src/adapter.cpp");
    println!("cargo:rerun-if-changed=src/adapter.h");
    println!("cargo:rerun-if-changed=src/canonical.hpp");
    println!("cargo:rerun-if-changed=vendor");

    // Match the qualified portable FFT build in debug and release profiles.
    // All sources are retained in this crate; a build never fetches code.
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .opt_level(2)
        .file("src/adapter.cpp")
        .include("vendor/signalsmith-stretch/include")
        .include("vendor/signalsmith-linear/include")
        .flag("-Wall")
        .flag("-Wextra")
        .warnings_into_errors(true)
        .compile("deadpan_dsp");
}
