//! Privacy audit of every network-capable path in the shipped workspace
//! (specification Section 27.4, docs/PRIVACY.md).
//!
//! Network access is limited to user-requested imports, model/helper updates
//! and explicit links. This test reads `Cargo.lock` and the Rust sources of
//! `crates/` and `native/` and fails when a network client, socket API,
//! subprocess launch site or telemetry dependency appears outside the
//! reviewed allowlist below. Adding one is a deliberate act: review the new
//! path against Section 27.4, then update this allowlist and docs/PRIVACY.md
//! together.
//!
//! It is a static check of the source tree, not a proof about runtime
//! behavior; `privacy_sandbox.rs` exercises the local workflows with all
//! network access denied.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Packages in the user-facing download stack. Only these and the model-pack
/// crate may depend on any of them.
const NETWORK_STACK: [&str; 10] = [
    "ureq",
    "ureq-proto",
    "rustls",
    "rustls-platform-verifier",
    "rustls-native-certs",
    "rustls-webpki",
    "webpki-roots",
    "webpki-root-certs",
    "http",
    "httparse",
];

/// The one workspace crate that owns the HTTPS client.
const NETWORK_OWNER: &str = "deadpan-models";

/// Network clients, servers, runtimes, link openers, analytics and crash
/// reporters that must not enter the lockfile at all.
const FORBIDDEN_PACKAGES: [&str; 27] = [
    "reqwest",
    "hyper",
    "hyper-util",
    "h2",
    "h3",
    "quinn",
    "curl",
    "curl-sys",
    "isahc",
    "attohttpc",
    "surf",
    "tokio",
    "async-std",
    "smol",
    "native-tls",
    "openssl",
    "openssl-sys",
    "tungstenite",
    "websocket",
    "tiny_http",
    "trust-dns-resolver",
    "hickory-resolver",
    "open",
    "opener",
    "minidump-writer",
    "minidumper",
    "crash-handler",
];

/// Substrings of package names that indicate telemetry, analytics or crash
/// reporting.
const TELEMETRY_MARKERS: [&str; 13] = [
    "sentry",
    "opentelemetry",
    "telemetry",
    "analytics",
    "posthog",
    "mixpanel",
    "amplitude",
    "segment-",
    "bugsnag",
    "rollbar",
    "datadog",
    "crashpad",
    "breakpad",
];

/// Shipped worker crates that must not reach the HTTPS client even
/// transitively: media, transcription, tracking and rendering run local only.
const OFFLINE_CRATES: [&str; 13] = [
    "deadpan-core",
    "deadpan-store",
    "deadpan-plan",
    "deadpan-media",
    "deadpan-render",
    "deadpan-audio",
    "deadpan-playback",
    "deadpan-analysis",
    "deadpan-jobs",
    "deadpan-media-worker",
    "deadpan-transcribe",
    "deadpan-track",
    "deadpan-source",
];

struct Lock {
    dependencies: BTreeMap<String, BTreeSet<String>>,
    workspace: BTreeSet<String>,
}

fn lock() -> Lock {
    let text = fs::read_to_string(workspace().join("Cargo.lock")).expect("Cargo.lock");
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut workspace = BTreeSet::new();
    for block in text.split("[[package]]").skip(1) {
        let mut name = None;
        let mut local = true;
        let mut inside = false;
        let mut names = BTreeSet::new();
        for line in block.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value.trim_matches('"').to_owned());
            } else if line.starts_with("source = ") {
                local = false;
            } else if line.starts_with("dependencies = [") {
                inside = !line.ends_with(']');
            } else if inside && line == "]" {
                inside = false;
            } else if inside {
                let entry = line.trim_end_matches(',').trim_matches('"');
                let dependency = entry.split_whitespace().next().expect("dependency name");
                names.insert(dependency.to_owned());
            }
        }
        let name = name.expect("package name");
        if local {
            workspace.insert(name.clone());
        }
        dependencies.entry(name).or_default().extend(names);
    }
    Lock {
        dependencies,
        workspace,
    }
}

impl Lock {
    fn closure(&self, root: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut pending = vec![root.to_owned()];
        while let Some(name) = pending.pop() {
            if seen.insert(name.clone()) {
                pending.extend(self.dependencies.get(&name).into_iter().flatten().cloned());
            }
        }
        seen
    }
}

#[test]
fn lockfile_has_one_reviewed_https_stack_and_no_telemetry() {
    let lock = lock();
    assert!(
        lock.workspace.contains(NETWORK_OWNER) && lock.dependencies.contains_key("ureq"),
        "the audit no longer matches the lockfile; review docs/PRIVACY.md"
    );
    for (package, dependencies) in &lock.dependencies {
        if dependencies.contains("webbrowser") {
            assert_eq!(
                package, "egui-winit",
                "browser dispatch is limited to explicit egui links"
            );
        }
        for network in NETWORK_STACK {
            if dependencies.contains(network) {
                assert!(
                    package == NETWORK_OWNER || NETWORK_STACK.contains(&package.as_str()),
                    "{package} depends on the network crate {network}; only {NETWORK_OWNER} \
                     may (see docs/PRIVACY.md)"
                );
            }
        }
        assert!(
            !FORBIDDEN_PACKAGES.contains(&package.as_str()),
            "{package} is a network/link/crash-reporting crate outside the reviewed stack"
        );
        for marker in TELEMETRY_MARKERS {
            assert!(
                !package.contains(marker),
                "{package} looks like a telemetry or crash-reporting dependency"
            );
        }
    }
}

/// Runtime (normal) dependencies by package name, from `cargo metadata`.
/// The lockfile alone also lists dev-dependencies.
fn runtime_dependencies() -> BTreeMap<String, BTreeSet<String>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked", "--offline"])
        .current_dir(workspace())
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata JSON");
    let names: BTreeMap<&str, &str> = metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|package| {
            (
                package["id"].as_str().expect("id"),
                package["name"].as_str().expect("name"),
            )
        })
        .collect();
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in metadata["resolve"]["nodes"].as_array().expect("resolve") {
        let name = names[node["id"].as_str().expect("node id")];
        let entry = dependencies.entry(name.to_owned()).or_default();
        for dependency in node["deps"].as_array().expect("deps") {
            let normal = dependency["dep_kinds"]
                .as_array()
                .expect("dep_kinds")
                .iter()
                .any(|kind| kind["kind"].is_null());
            if normal {
                entry.insert(names[dependency["pkg"].as_str().expect("pkg")].to_owned());
            }
        }
    }
    dependencies
}

#[test]
fn local_workers_cannot_reach_the_https_client() {
    let lock = Lock {
        dependencies: runtime_dependencies(),
        workspace: BTreeSet::new(),
    };
    for offline in OFFLINE_CRATES {
        let closure = lock.closure(offline);
        assert!(
            !closure.contains("webbrowser"),
            "{offline} reaches browser dispatch"
        );
        assert!(closure.contains(offline), "{offline} is not a package");
        assert!(closure.len() > 1 || offline == "deadpan-core", "{offline}");
        for network in NETWORK_STACK {
            assert!(
                !closure.contains(network),
                "{offline} reaches the network crate {network} at runtime"
            );
        }
    }
    // The two user-facing binaries do reach it, through deadpan-models only.
    for binary in ["deadpan-cli", "deadpan-app"] {
        assert!(lock.closure(binary).contains("ureq"), "{binary}");
    }
    assert!(!lock.closure("deadpan-cli").contains("webbrowser"));
    assert!(lock.closure("deadpan-app").contains("webbrowser"));
}

/// Every Rust source under `crates/` and `native/`, except the developer-only
/// `xtask` (build, packaging and qualification tooling that never ships).
fn sources() -> Vec<(String, String)> {
    fn walk(directory: &Path, root: &Path, found: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .expect("read source directory")
            .map(|entry| entry.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    walk(&path, root, found);
                }
            } else if name.ends_with(".rs") {
                let relative = path
                    .strip_prefix(root)
                    .expect("inside workspace")
                    .to_string_lossy()
                    .into_owned();
                if !relative.starts_with("crates/xtask/") {
                    let text = fs::read_to_string(&path).expect("read source");
                    found.push((relative, text));
                }
            }
        }
    }
    let root = workspace();
    let mut found = Vec::new();
    for top in ["crates", "native"] {
        walk(&root.join(top), &root, &mut found);
    }
    assert!(found.len() > 100, "found only {} sources", found.len());
    found
}

/// Test, example and benchmark code that never ships in a binary.
fn is_test(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/')
        .any(|part| matches!(part, "tests" | "examples" | "benches"))
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
}

/// Shipped files containing any of `needles`.
fn shipped_with(sources: &[(String, String)], needles: &[&str]) -> BTreeSet<String> {
    sources
        .iter()
        .filter(|(path, _)| !is_test(path))
        .filter(|(_, text)| needles.iter().any(|needle| contains_word(text, needle)))
        .map(|(path, _)| path.clone())
        .collect()
}

/// `needle` occurs with no identifier character immediately before it.
fn contains_word(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn assert_allowlisted(found: BTreeSet<String>, allowed: &[&str], what: &str) {
    let allowed: BTreeSet<String> = allowed.iter().map(|path| (*path).to_owned()).collect();
    let unexpected: Vec<_> = found.difference(&allowed).collect();
    assert!(
        unexpected.is_empty(),
        "{what} appears outside the reviewed privacy allowlist: {unexpected:?}. Review the \
         new path against specification Section 27.4, then update this test and \
         docs/PRIVACY.md"
    );
    let stale: Vec<_> = allowed.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "{what} allowlist names files that no longer use it: {stale:?}"
    );
}

#[test]
fn network_clients_live_only_in_reviewed_download_paths() {
    let sources = sources();

    // eframe dispatches an explicitly activated hyperlink. No application
    // code may bypass that user action by calling a browser opener directly.
    assert_allowlisted(
        shipped_with(&sources, &["webbrowser::", "open::", "opener::"]),
        &[],
        "direct browser dispatch",
    );

    // The one HTTP client: HTTPS only, bounded redirects, system trust.
    assert_allowlisted(
        shipped_with(&sources, &["ureq"]),
        &["crates/deadpan-models/src/packs.rs"],
        "the ureq HTTP client",
    );
    let packs = &sources
        .iter()
        .find(|(path, _)| path == "crates/deadpan-models/src/packs.rs")
        .expect("packs.rs")
        .1;
    assert!(packs.contains(".https_only(true)") && packs.contains(".max_redirects(5)"));

    // Every construction of that transport is an explicit user download:
    // model packs (`models install/update`, the Models panel), helper
    // install/update (`downloader install/update`, New from URL's install
    // confirmation, the Models panel's signed update) and signed manifests
    // fetched from an explicit https URL.
    assert_allowlisted(
        shipped_with(&sources, &["HttpsTransport"]),
        &[
            "crates/deadpan-app/src/model_packs.rs",
            "crates/deadpan-app/src/youtube.rs",
            "crates/deadpan-cli/src/models.rs",
            "crates/deadpan-cli/src/update_signing.rs",
            "crates/deadpan-cli/src/youtube/helpers.rs",
            "crates/deadpan-models/src/packs.rs",
        ],
        "the HTTPS download transport",
    );

    // No IP sockets anywhere in shipped code.
    assert_allowlisted(
        shipped_with(
            &sources,
            &[
                "std::net",
                "TcpStream",
                "TcpListener",
                "UdpSocket",
                "ToSocketAddrs",
                "SocketAddrV4",
                "SocketAddrV6",
                "AddressFamily::INET",
                "AddressFamily::INET6",
                "AF_INET",
                "getaddrinfo",
            ],
        ),
        &[],
        "an IP socket API",
    );
    // No Apple networking APIs.
    assert_allowlisted(
        shipped_with(
            &sources,
            &[
                "NSURLSession",
                "URLSession",
                "NSURLConnection",
                "CFNetwork",
                "CFSocket",
                "NWConnection",
                "nw_connection",
                "openURL",
            ],
        ),
        &[],
        "an Apple networking or URL-opening API",
    );
    // Unix-domain sockets: the authenticated live-project endpoint and
    // worker control socket pairs. All are local to this user and machine.
    assert_allowlisted(
        shipped_with(
            &sources,
            &[
                "rustix::net",
                "unix::net",
                "UnixStream",
                "UnixListener",
                "UnixDatagram",
            ],
        ),
        &[
            "crates/deadpan-cli/src/encoded_render/admission/worker.rs",
            "crates/deadpan-cli/src/host/client.rs",
            "crates/deadpan-cli/src/host/server.rs",
            "crates/deadpan-cli/src/host/wire.rs",
            "crates/deadpan-cli/src/render_worker/worker/control.rs",
        ],
        "a Unix-domain socket",
    );
    let wire = &sources
        .iter()
        .find(|(path, _)| path == "crates/deadpan-cli/src/host/wire.rs")
        .expect("wire.rs")
        .1;
    assert!(wire.contains("AddressFamily::UNIX"));
}

#[test]
fn subprocess_launches_are_reviewed_and_name_no_network_tool() {
    let sources = sources();
    // Every shipped process launch site. yt-dlp and Deno run only through
    // `youtube/runner.rs` (user-requested import and the downloader probe);
    // the AI runtime check reuses that runner. The others start local
    // workers, codesign verification, pmset, test-only shells or the
    // optional UI harness.
    assert_allowlisted(
        shipped_with(&sources, &["Command::new("]),
        &[
            "crates/deadpan-app/src/recovery.rs",
            "crates/deadpan-app/src/ui_harness.rs",
            "crates/deadpan-chaos/src/process.rs",
            "crates/deadpan-cli/src/generation/attempt/synthetic.rs",
            "crates/deadpan-cli/src/proxy.rs",
            "crates/deadpan-cli/src/youtube/helpers.rs",
            "crates/deadpan-cli/src/youtube/runner.rs",
            "crates/deadpan-jobs/src/process.rs",
            "crates/deadpan-media/src/conversion.rs",
            "crates/deadpan-media/src/conversion/proxy.rs",
            "crates/deadpan-media/src/conversion/remux.rs",
            "native/deadpan-encode/build.rs",
            "native/deadpan-media-worker/build.rs",
            "native/deadpan-process/src/lib.rs",
            "native/deadpan-source/build.rs",
        ],
        "a subprocess launch",
    );
    // The yt-dlp/Deno runner is reached only from the YouTube import,
    // helper management and the AI runtime smoke check.
    assert_allowlisted(
        shipped_with(&sources, &["run_helper"]),
        &[
            "crates/deadpan-cli/src/generation/runtime.rs",
            "crates/deadpan-cli/src/youtube/acquire.rs",
            "crates/deadpan-cli/src/youtube/runner.rs",
        ],
        "the downloader helper runner",
    );
    // No shipped code names a network tool or URL opener as a program.
    for tool in [
        "\"curl\"",
        "\"/usr/bin/curl\"",
        "\"wget\"",
        "\"nscurl\"",
        "\"/usr/bin/nscurl\"",
        "\"/usr/bin/open\"",
        "\"osascript\"",
        "\"/usr/bin/osascript\"",
        "\"/usr/bin/nc\"",
        "\"/usr/bin/ssh\"",
        "\"/usr/bin/scp\"",
        "\"/usr/bin/git\"",
        "\"/usr/bin/sftp\"",
    ] {
        assert_allowlisted(shipped_with(&sources, &[tool]), &[], tool);
    }
}

#[test]
fn local_engines_stay_offline_by_construction() {
    let root = workspace();
    // The linked FFmpeg is built without network protocols; each build
    // script refuses a prefix configured otherwise.
    for crate_dir in [
        "native/deadpan-source",
        "native/deadpan-encode",
        "native/deadpan-media-worker",
    ] {
        let build = fs::read_to_string(root.join(crate_dir).join("build.rs")).expect("build.rs");
        let required = build
            .split("REQUIRED_CONFIGURATION")
            .nth(1)
            .and_then(|rest| rest.split("];").next())
            .expect("required configuration");
        let forbidden = build
            .split("FORBIDDEN_CONFIGURATION")
            .nth(1)
            .and_then(|rest| rest.split("];").next())
            .expect("forbidden configuration");
        assert!(required.contains("\"--disable-network\""), "{crate_dir}");
        assert!(forbidden.contains("\"--enable-network\""), "{crate_dir}");
    }
    // Both real AI entrypoints use the fixed macOS network-denied launcher.
    // The runtime tests prove the actual policy, inherited restrictions,
    // local file access, framed stdio and owned leader/group identity.
    let attempt = fs::read_to_string(root.join("crates/deadpan-cli/src/generation/attempt.rs"))
        .expect("attempt.rs");
    let runtime = fs::read_to_string(root.join("crates/deadpan-cli/src/generation/runtime.rs"))
        .expect("runtime.rs");
    let launch =
        fs::read_to_string(root.join("crates/deadpan-cli/src/generation/runtime/launch.rs"))
            .expect("launch.rs");
    assert!(attempt.contains("runtime.worker_launch(&runtime_config, WorkerMode::Inference)"));
    assert!(runtime.contains(".worker_launch(&configuration, WorkerMode::Check)"));
    assert!(launch.contains("\"/usr/bin/sandbox-exec\""));
    assert!(launch.contains("\"(version 1)(allow default)(deny network*)\""));
    assert!(!attempt.contains("executable: runtime.python"));
    assert!(!runtime.contains("executable: &self.python"));
    // Hugging Face/Transformers offline and telemetry-disabled settings are
    // retained as defense in depth; the worker refuses to generate otherwise.
    for flag in [
        "(\"HF_HUB_OFFLINE\", \"1\")",
        "(\"TRANSFORMERS_OFFLINE\", \"1\")",
        "(\"HF_HUB_DISABLE_TELEMETRY\", \"1\")",
        "(\"DO_NOT_TRACK\", \"1\")",
    ] {
        assert!(attempt.contains(flag), "{flag}");
    }
    // Helpers run with a cleared environment (no inherited proxy or
    // credentials) and Deno's update check disabled.
    let runner = fs::read_to_string(root.join("crates/deadpan-cli/src/youtube/runner.rs"))
        .expect("runner.rs");
    assert!(runner.contains(".env_clear()"));
    assert!(runner.contains("(\"DENO_NO_UPDATE_CHECK\".into(), \"1\".into())"));
    let acquire = fs::read_to_string(root.join("crates/deadpan-cli/src/youtube/acquire.rs"))
        .expect("acquire.rs");
    for argument in [
        "\"--no-cookies-from-browser\"",
        "\"--no-remote-components\"",
        "\"--ignore-config\"",
    ] {
        assert!(acquire.contains(argument), "{argument}");
    }
}

/// Section 27.4: the live endpoint secret must not reach logs. Structs that
/// hold a secret do not derive `Debug`, so no `{:?}` can print one.
#[test]
fn secret_bearing_types_cannot_be_debug_formatted() {
    let sources = sources();
    let mut checked = 0;
    for (path, text) in sources.iter().filter(|(path, _)| !is_test(path)) {
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let field = line.trim_start();
            let field = field
                .strip_prefix("pub(super) ")
                .or_else(|| field.strip_prefix("pub(crate) "))
                .or_else(|| field.strip_prefix("pub "))
                .unwrap_or(field);
            if !(field.starts_with("secret: ") || field.starts_with("cookie: ")) {
                continue;
            }
            let Some(start) = lines[..index]
                .iter()
                .rposition(|line| line.contains("struct ") || line.contains("enum "))
            else {
                continue;
            };
            let attributes = lines[..start]
                .iter()
                .rev()
                .take_while(|line| {
                    let line = line.trim_start();
                    line.starts_with("#[") || line.starts_with("///") || line.is_empty()
                })
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !attributes.contains("Debug"),
                "{path}:{} holds a secret in a Debug type",
                index + 1
            );
            checked += 1;
        }
    }
    // host.rs Request and namespace.rs Discovery.
    assert!(checked >= 2, "found only {checked} secret fields");
}
