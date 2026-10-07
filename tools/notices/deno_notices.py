#!/usr/bin/env python3
"""Regenerate the vendored third-party notice set for the pinned Deno helper.

Deno publishes only its own MIT license. This tool aggregates, from upstream
sources at the exact pinned revisions, the license files of everything
statically linked into the pinned `deno` executable:

- Deno's own repository files (LICENSE.md and the license files beside
  embedded type declarations), plus retained source copyright/license
  comments from the runtime, extensions and embedded compiler declarations;
- every crates.io package in the first `deno` executable tree from the release build command
  (`-p deno -p denort -p test_server --features deno/panic-trace`, normal
  edges, aarch64-apple-darwin), with license files found by the same rules as
  `cargo xtask bundle` uses for Deadpan's crates;
- rusty_v8, V8 and the V8 third-party sources compiled into the prebuilt
  static library for this configuration (see SOURCE_COMPONENTS);
- the Rust standard library at the rustc commit embedded in the executable.

Output (deterministic): `<output>/manifest.json` and `<output>/texts/<sha256>.txt`
with each distinct license text once. `cargo xtask bundle` renders the
aggregated notice from these files and refuses any hash mismatch.

Usage (development only; needs git, cargo and network access):

    git clone --depth 1 --branch v2.9.7 https://github.com/denoland/deno.git DENO
    CARGO_HOME=SCRATCH/cargo-home python3 tools/notices/deno_notices.py \
        --deno-source DENO --deno-executable PATH/TO/pinned/deno \
        --cache SCRATCH/notice-sources --output packaging/notices/deno-2.9.7
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

SCHEMA = 1
DENO_VERSION = "2.9.7"
DENO_TAG = f"v{DENO_VERSION}"
DENO_COMMIT = "0c071246a412575e07423263404a5d13e7ed6aa2"
RUSTY_V8_COMMIT = "5c15a6995c9bb4bacd3e341b59fff32c909c80bf"
DENO_EXECUTABLE_SHA256 = "b73737579d5a84c160e3316487594783fa5c15f4e13252a6a07050b755317f1a"
DENO_RELEASE_URL = (
    f"https://github.com/denoland/deno/releases/download/{DENO_TAG}/deno-aarch64-apple-darwin.zip"
)
TOOLCHAIN = "1.97.1"
TARGET = "aarch64-apple-darwin"
TREE_COMMAND = [
    "cargo", "tree", "--locked", "-p", "deno", "-p", "denort", "-p", "test_server",
    "--features", "deno/panic-trace", "-e", "normal", "--target", TARGET,
    "--prefix", "none", "-f", "{p}",
]

SPDX_TEXT = "https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/text/{}.txt"
USER_AGENT = "OpenAI File Downloader, XaiImageApiFetch/1.0"

# Mirrors crates/xtask/src/bundle/notices.rs.
LICENSE_NAMES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE")
SKIPPED_DIRECTORIES = {
    "target", "tests", "test", "examples", "benches", ".git", ".github", "fuzz", "node_modules",
}
SOURCE_EXTENSIONS = {"RS", "C", "H", "CC", "CPP", "HPP", "M", "PY", "JS", "TOML", "JSON", "YML"}
LICENSE_FILE_LIMIT = 512 * 1024
SEARCH_DEPTH = 3

# Deno repository files compiled or embedded into the executable.
DENO_FILES = [
    ("LICENSE.md", "Deno", "MIT"),
    ("ext/webgpu/LICENSE.md", "Deno WebGPU extension", "MIT"),
    ("cli/tsc/dts/node/LICENSE", "Node.js type declarations (DefinitelyTyped), embedded", "MIT"),
    ("cli/tsc/dts/node/undici/LICENSE", "undici type declarations, embedded", "MIT"),
]
TYPESCRIPT_SOURCE = "cli/tsc/00_typescript.js"

# Pinned C/C++ sources linked into rusty_v8's static library on macOS arm64.
# Each submodule commit is read from the rusty_v8 tag's tree; licenses are the
# upstream files at those commits. `license` is the upstream README.chromium /
# repository declaration, used only as SBOM metadata.
SOURCE_COMPONENTS = [
    # (key, display name, submodule path in rusty_v8 or None, clone URL, files, license)
    ("rusty_v8", "rusty_v8", None, "https://github.com/denoland/rusty_v8.git", ["LICENSE"], "MIT"),
    ("v8", "V8", "v8", "https://github.com/denoland/v8.git", [
        "LICENSE", "LICENSE.fdlibm", "LICENSE.strongtalk", "LICENSE.v8",
        "third_party/glibc/LICENSE",
        "third_party/inspector_protocol/LICENSE",
        "third_party/rapidhash-v8/LICENSE",
        "third_party/siphash/LICENSE",
        "third_party/utf8-decoder/LICENSE",
        "third_party/v8/builtins/LICENSE",
        "third_party/v8/codegen/LICENSE",
    ], None),
    ("icu", "ICU (Chromium copy)", "third_party/icu",
     "https://chromium.googlesource.com/chromium/deps/icu.git", ["LICENSE"], "Unicode-3.0"),
    ("abseil-cpp", "Abseil C++", "third_party/abseil-cpp",
     "https://chromium.googlesource.com/chromium/src/third_party/abseil-cpp.git", ["LICENSE"],
     "Apache-2.0"),
    ("libc++", "LLVM libc++ (Chromium copy, statically linked)", "third_party/libc++/src",
     "https://chromium.googlesource.com/external/github.com/llvm/llvm-project/libcxx.git",
     ["LICENSE.TXT"], "Apache-2.0 WITH LLVM-exception"),
    ("libc++abi", "LLVM libc++abi", "third_party/libc++abi/src",
     "https://chromium.googlesource.com/external/github.com/llvm/llvm-project/libcxxabi.git",
     ["LICENSE.TXT"], "Apache-2.0 WITH LLVM-exception"),
    ("llvm-libc", "LLVM libc (shared with libc++)", "third_party/llvm-libc/src",
     "https://chromium.googlesource.com/external/github.com/llvm/llvm-project/libc.git",
     ["LICENSE.TXT"], "Apache-2.0 WITH LLVM-exception"),
    ("fp16", "FP16", "third_party/fp16/src", "https://github.com/Maratyszcza/FP16.git",
     ["LICENSE"], "MIT"),
    ("fast_float", "fast_float", "third_party/fast_float/src",
     "https://chromium.googlesource.com/external/github.com/fastfloat/fast_float.git",
     ["LICENSE-APACHE", "LICENSE-BOOST", "LICENSE-MIT"], "Apache-2.0 OR BSL-1.0 OR MIT"),
    ("dragonbox", "Dragonbox", "third_party/dragonbox/src",
     "https://chromium.googlesource.com/external/github.com/jk-jeon/dragonbox.git",
     ["LICENSE-Apache2-LLVM", "LICENSE-Boost"], "Apache-2.0 WITH LLVM-exception OR BSL-1.0"),
    ("highway", "Highway", "third_party/highway/src",
     "https://chromium.googlesource.com/external/github.com/google/highway.git", ["LICENSE"],
     None),
    ("simdutf", "simdutf", "third_party/simdutf",
     "https://chromium.googlesource.com/chromium/src/third_party/simdutf", ["LICENSE"], None),
]

# Present in the pinned sources but not linked into this configuration.
EXCLUDED = [
    "rusty_v8 third_party/partition_alloc: V8 enables it only with a shared pointer-compression cage, which Deno's v8 features leave off",
    "rusty_v8 third_party/libunwind: not built for macOS (the system unwinder is used)",
    "rusty_v8 third_party/rust: V8's Temporal support links temporal_capi and ICU4X through Cargo; those crates are listed with the Cargo packages",
    "rusty_v8 build, buildtools, tools/clang, tools/win, third_party/jinja2, third_party/markupsafe: build tools only",
    "V8 third_party/googletest, jsoncpp, colorama, re2: tests and tools only",
    "V8 third_party/valgrind: x64 only; third_party/vtune: v8_enable_vtunejit is off; third_party/wasm-api: separate C API target",
    "V8 Perfetto, zoslib and Fuchsia SDK references: not enabled for macOS",
]


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(command: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"{' '.join(command)} failed in {cwd}:\n{result.stderr}")
    return result.stdout


def is_license_name(name: str) -> bool:
    upper = name.upper()
    extension = upper.rsplit(".", 1)[1] if "." in upper else ""
    if extension in SOURCE_EXTENSIONS:
        return False
    return (
        upper.startswith(LICENSE_NAMES)
        or "LICENSE" in upper
        or "LICENCE" in upper
        or upper in ("OFL.TXT", "UFL.TXT")
    )


def is_font_license(path: Path) -> bool:
    return path.suffix == ".txt" and any(path.with_suffix(s).is_file() for s in (".ttf", ".otf"))


def license_files(directory: Path, depth: int = 0) -> list[Path]:
    found = []
    try:
        entries = sorted(directory.iterdir())
    except OSError:
        return found
    for entry in entries:
        if entry.is_symlink():
            continue
        if entry.is_dir():
            if depth < SEARCH_DEPTH and entry.name not in SKIPPED_DIRECTORIES:
                found.extend(license_files(entry, depth + 1))
        elif entry.is_file() and (is_license_name(entry.name) or is_font_license(entry)):
            if entry.stat().st_size <= LICENSE_FILE_LIMIT:
                found.append(entry)
    return found


class Texts:
    def __init__(self) -> None:
        self.bodies: dict[str, bytes] = {}

    def add(self, data: bytes, label: str) -> str:
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            sys.exit(f"{label} is not UTF-8")
        digest = sha256(data)
        self.bodies[digest] = data
        return digest


def git_partial(cache: Path, name: str, url: str, commit: str) -> Path:
    directory = cache / name
    if not (directory / ".git").is_dir():
        directory.mkdir(parents=True, exist_ok=True)
        run(["git", "init", "-q"], directory)
        run(["git", "remote", "add", "origin", url], directory)
    present = subprocess.run(
        ["git", "cat-file", "-e", f"{commit}^{{commit}}"], cwd=directory, capture_output=True
    )
    if present.returncode != 0 or commit.startswith("refs/"):
        run(["git", "fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", commit], directory)
    return directory


def git_blob(directory: Path, commit: str, path: str) -> bytes:
    result = subprocess.run(
        ["git", "show", f"{commit}:{path}"], cwd=directory, capture_output=True
    )
    if result.returncode != 0:
        sys.exit(f"{path} missing at {commit} in {directory}: {result.stderr.decode()}")
    return result.stdout


def source_attributions(deno: Path, texts: Texts) -> list[dict[str, str]]:
    """Retain third-party source notices, including Node-derived polyfills.

    This is a conservative inventory of runtime/extension/compiler source
    comments, not a claim that every source is linked on macOS. Taking all
    relevant comments also covers generated embedded .mjs copies. Preserve
    bytes; omit only the ubiquitous Deno-only header covered by LICENSE.md.
    """
    paths = run([
        "git", "ls-tree", "-r", "--name-only", DENO_COMMIT,
        "ext", "runtime", "cli/tsc",
    ], deno).splitlines()
    comment = re.compile(r"/\*[\s\S]*?\*/|(?:^[ \t]*//[^\n]*(?:\n|$))+", re.M)
    relevant = re.compile(r"copyright|permission is hereby granted|licensed under|SPDX-License-Identifier", re.I)
    deno_only = re.compile(r"// Copyright \d{4}(?:-\d{4})? the Deno authors\. MIT license\.")
    files = []
    for path in paths:
        if Path(path).suffix not in {".js", ".mjs", ".cjs", ".ts", ".rs"}:
            continue
        if any(part in {"tests", "test", "testdata", "benches", "benchmarks"} for part in Path(path).parts):
            continue
        body = git_blob(deno, DENO_COMMIT, path).decode("utf-8")
        comments = []
        for match in comment.finditer(body):
            value = match.group()
            if relevant.search(deno_only.sub("", value)):
                comments.append(value.rstrip("\n"))
        if not comments:
            continue
        retained = ("\n\n".join(comments) + "\n").encode()
        files.append({
            "path": f"{path} (copyright/license comments)",
            "component": "Deno embedded and runtime source attributions",
            "license": "see retained source comments and standard license texts",
            "source": file_url("https://github.com/denoland/deno.git", DENO_COMMIT, path),
            "derived": "verbatim copyright/license comment blocks in source order; conservative source inventory",
            "sha256": texts.add(retained, path),
        })
    return files


def gitlink(directory: Path, commit: str, path: str) -> str:
    line = run(["git", "ls-tree", commit, path], directory).strip()
    parts = line.split()
    if len(parts) < 3 or parts[1] != "commit":
        sys.exit(f"{path} is not a submodule at {commit}")
    return parts[2]


def file_url(clone_url: str, commit: str, path: str) -> str:
    if clone_url.startswith("https://github.com/"):
        repository = clone_url.removeprefix("https://github.com/").removesuffix(".git")
        return f"https://raw.githubusercontent.com/{repository}/{commit}/{path}"
    return f"{clone_url.removesuffix('.git')}/+/{commit}/{path}"


def lock_entries(lock: Path) -> dict[tuple[str, str], str]:
    result = {}
    for block in lock.read_text().split("[[package]]")[1:]:
        fields = dict(re.findall(r'^(name|version|checksum) = "([^"]*)"', block, re.M))
        if "checksum" in fields:
            result[(fields["name"], fields["version"])] = fields["checksum"]
    return result


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def spdx_ids(expression: str) -> set[str]:
    tokens = expression.replace("/", " OR ").replace("(", " ").replace(")", " ").split()
    return {
        token.removesuffix("+") for token in tokens if token not in ("AND", "OR", "WITH")
    }


def rustc_commit(executable: Path) -> str:
    data = executable.read_bytes()
    if sha256(data) != DENO_EXECUTABLE_SHA256:
        sys.exit(f"{executable} is not the pinned Deno {DENO_VERSION} executable")
    commits = set(re.findall(rb"/rustc/([0-9a-f]{40})/library/", data))
    if len(commits) != 1:
        sys.exit(f"expected one embedded rustc commit, found {sorted(commits)}")
    return commits.pop().decode()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--deno-source", type=Path, required=True)
    parser.add_argument("--deno-executable", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    deno = arguments.deno_source.resolve()
    head = run(["git", "rev-parse", "HEAD"], deno).strip()
    if head != DENO_COMMIT:
        sys.exit(f"{deno} is at {head}, not {DENO_TAG} ({DENO_COMMIT})")
    if run(["git", "status", "--porcelain", "--untracked-files=normal"], deno).strip():
        sys.exit("Deno source checkout must be clean so Cargo reads the pinned manifests and configuration")
    environment = dict(os.environ, RUSTUP_TOOLCHAIN=TOOLCHAIN)
    texts = Texts()

    # Deno's own files.
    deno_files = []
    for path, component, license in DENO_FILES:
        data = git_blob(deno, DENO_COMMIT, path)
        deno_files.append({
            "path": path,
            "component": component,
            "license": license,
            "source": file_url("https://github.com/denoland/deno.git", DENO_COMMIT, path),
            "sha256": texts.add(data, path),
        })
    typescript = git_blob(deno, DENO_COMMIT, TYPESCRIPT_SOURCE).decode("utf-8")
    end = typescript.index("*/") + 2
    if not typescript.startswith("/*!"):
        sys.exit("the TypeScript compiler no longer starts with its license header")
    header = (typescript[:end] + "\n").encode()
    deno_files.append({
        "path": f"{TYPESCRIPT_SOURCE} (leading license comment)",
        "component": "TypeScript compiler (Microsoft), embedded",
        "license": "Apache-2.0",
        "source": file_url("https://github.com/denoland/deno.git", DENO_COMMIT, TYPESCRIPT_SOURCE),
        "derived": "the file's leading /*! ... */ comment, verbatim",
        "sha256": texts.add(header, TYPESCRIPT_SOURCE),
    })
    deno_files.extend(source_attributions(deno, texts))

    # Cargo closure.
    tree = run(TREE_COMMAND, deno, environment)
    first_tree = tree.split("\n\n", 1)[0]
    if first_tree.splitlines()[0].split()[:2] != ["deno", f"v{DENO_VERSION}"]:
        sys.exit("Cargo's first dependency tree is not the pinned deno executable")
    roots = set()
    for line in first_tree.splitlines():
        line = line.removesuffix(" (*)").removesuffix(" (proc-macro)")
        name, version = line.split(" ", 2)[:2]
        roots.add((name, version.removeprefix("v")))
    metadata = json.loads(run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--all-features",
         "--filter-platform", TARGET],
        deno, environment,
    ))
    members = set(metadata["workspace_members"])
    checksums = lock_entries(deno / "Cargo.lock")
    crates = []
    workspace_crates = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        key = (package["name"], package["version"])
        if key not in roots:
            continue
        if package["id"] in members:
            workspace_crates.append(f"{package['name']} {package['version']}")
            continue
        directory = Path(package["manifest_path"]).parent
        files = license_files(directory)
        if package.get("license_file"):
            extra = directory / package["license_file"]
            if extra not in files:
                files.append(extra)
        entries = []
        for file in files:
            relative = file.relative_to(directory).as_posix()
            entries.append({
                "path": relative,
                "sha256": texts.add(file.read_bytes(), f"{package['name']} {relative}"),
            })
        crates.append({
            "name": package["name"],
            "version": package["version"],
            "license": package.get("license"),
            "authors": package.get("authors") or [],
            "repository": package.get("repository"),
            "checksum": checksums.get(key),
            "files": entries,
        })
    found = {(c["name"], c["version"]) for c in crates} | {
        tuple(item.split(" ")) for item in workspace_crates
    }
    if found != roots:
        sys.exit(f"cargo tree packages missing from metadata: {sorted(roots - found)}")

    # rusty_v8, V8 and third-party sources.
    v8_version = next(v for (n, v) in roots if n == "v8")
    rusty_url = SOURCE_COMPONENTS[0][3]
    rusty = git_partial(arguments.cache, "rusty_v8", rusty_url, f"refs/tags/v{v8_version}")
    rusty_commit = run(["git", "rev-parse", "FETCH_HEAD"], rusty).strip()
    if rusty_commit != RUSTY_V8_COMMIT:
        sys.exit(f"rusty_v8 tag resolved to {rusty_commit}, not reviewed commit {RUSTY_V8_COMMIT}")
    sources = []
    for key, name, submodule, url, paths, license in SOURCE_COMPONENTS:
        if submodule is None:
            directory, commit = rusty, rusty_commit
        else:
            commit = gitlink(rusty, rusty_commit, submodule)
            directory = git_partial(arguments.cache, key, url, commit)
        files = []
        for path in paths:
            files.append({
                "path": path,
                "source": file_url(url, commit, path),
                "sha256": texts.add(git_blob(directory, commit, path), f"{key} {path}"),
            })
        sources.append({
            "key": key,
            "name": name,
            "repository": url,
            "commit": commit,
            "location": "rusty_v8" if submodule is None else f"rusty_v8/{submodule}",
            "license": license,
            "files": files,
        })

    # The Rust standard library.
    rust_commit = rustc_commit(arguments.deno_executable)
    rust = git_partial(arguments.cache, "rust", "https://github.com/rust-lang/rust.git", rust_commit)
    rust_files = []
    for path in ["COPYRIGHT", "LICENSE-APACHE", "LICENSE-MIT"]:
        rust_files.append({
            "path": path,
            "source": file_url("https://github.com/rust-lang/rust.git", rust_commit, path),
            "sha256": texts.add(git_blob(rust, rust_commit, path), f"rust {path}"),
        })
    sources.append({
        "key": "rust-std",
        "name": "Rust standard library",
        "repository": "https://github.com/rust-lang/rust.git",
        "commit": rust_commit,
        "location": "rustc commit embedded in the executable",
        "license": "MIT OR Apache-2.0",
        "files": rust_files,
    })

    # Standard texts for crates that publish no license file.
    needed = set()
    for item in crates:
        if not item["files"]:
            if not item["license"]:
                sys.exit(f"{item['name']} {item['version']} has neither a license nor a file")
            needed |= spdx_ids(item["license"])
    spdx = []
    for identifier in sorted(needed):
        url = SPDX_TEXT.format(identifier)
        spdx.append({
            "id": identifier,
            "source": url,
            "sha256": texts.add(fetch(url), url),
        })

    manifest = {
        "schema": SCHEMA,
        "deno": {
            "version": DENO_VERSION,
            "tag": DENO_TAG,
            "commit": DENO_COMMIT,
            "release": DENO_RELEASE_URL,
            "executable_sha256": DENO_EXECUTABLE_SHA256,
        },
        "rusty_v8": {"version": v8_version, "tag": f"v{v8_version}", "commit": rusty_commit},
        "cargo": {
            "command": " ".join(TREE_COMMAND[:-4]),
            "lock_sha256": sha256((deno / "Cargo.lock").read_bytes()),
            "workspace_crates": sorted(workspace_crates),
        },
        "deno_files": deno_files,
        "sources": sources,
        "crates": crates,
        "spdx": spdx,
        "excluded": EXCLUDED,
        "texts": sorted(texts.bodies),
    }
    output = arguments.output
    if (output / "texts").exists():
        shutil.rmtree(output / "texts")
    (output / "texts").mkdir(parents=True)
    for digest, data in sorted(texts.bodies.items()):
        (output / "texts" / f"{digest}.txt").write_bytes(data)
    (output / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
    size = sum(len(data) for data in texts.bodies.values())
    print(f"{len(crates)} crates, {len(workspace_crates)} Deno workspace crates, "
          f"{len(sources)} source components, {len(texts.bodies)} texts ({size} bytes)")


if __name__ == "__main__":
    main()
