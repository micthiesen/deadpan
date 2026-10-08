#!/usr/bin/env python3
"""Assemble Deadpan's private, relocatable AI runtime from pinned inputs.

`cargo xtask bundle` runs `build` and copies the result into
`Deadpan.app/Contents/Resources/ai-runtime`; nothing here runs on a user's Mac.
Every input is pinned in `pins.json` and verified before use:

* python-build-standalone CPython (archive SHA-256);
* the exact wheel set the qualified ltx-2-mlx checkout's `uv.lock` resolved
  (per-wheel SHA-256 from that lock), installed offline with `--no-index`;
* the ltx-2-mlx source at the qualified commit, copied file by file against
  `tools/model-qualification/ltx-source-manifest.json`;
* x264 at a pinned commit (Git object identity) and FFmpeg 8.0.3 (archive
  SHA-256), built as static `ffmpeg`/`ffprobe` programs for the worker's
  libx264 conditioning and lossless intermediates. That build is GPL-2.0-or-later
  and separate from the LGPL libraries the application links.

`pins` regenerates the wheel pins from a checkout's `uv.lock` and the recorded
runtime inventory. Python is a build-time tool only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent
WORKSPACE = ROOT.parent.parent
QUALIFICATION = WORKSPACE / "tools/model-qualification"
USER_AGENT = "OpenAI File Downloader, XaiImageApiFetch/1.0"
WORKER_FILES = ["worker.py", "worker_protocol.py", "worker_media.py", "mlx_backend.py",
                "worker_extension_context.py", "runtime_source.py", "ltx-source-manifest.json"]
RECEIPT = "evidence/2026-09-20-smoke/download-manifest.json"
# Standard-library parts no worker uses; removing them drops unsigned Tcl/Tk
# libraries and the package installer from the shipped runtime.
REMOVED_STDLIB = ["idlelib", "tkinter", "turtledemo", "test", "ensurepip", "turtle.py",
                  "site-packages/pip"]
# Bump when the assembly changes in a way the pins do not capture.
ASSEMBLY_VERSION = 4
# Bump when the codec build recipe below changes.
CODEC_RECIPE_VERSION = 1


def tree_digest(root: Path) -> str:
    """SHA-256 over every path, link target and file hash below `root`."""
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            digest.update(f"L {relative} {os.readlink(path)}\n".encode())
        elif path.is_file():
            digest.update(f"F {relative} {digest_file(path)}\n".encode())
        elif path.is_dir():
            digest.update(f"D {relative}\n".encode())
    return digest.hexdigest()


def digest_file(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def digest(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def fetch(url: str, sha256: str, destination: Path) -> Path:
    """Download once into the cache; every use re-verifies the hash."""
    if destination.exists() and digest(destination) == sha256:
        return destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".part")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=120) as response, partial.open("wb") as output:
        shutil.copyfileobj(response, output, 1 << 20)
    actual = digest(partial)
    if actual != sha256:
        partial.unlink()
        raise SystemExit(f"{url}: SHA-256 {actual} differs from the pin {sha256}")
    partial.rename(destination)
    return destination


def run(argv, *, cwd=None, env=None, log=None):
    started = time.monotonic()
    result = subprocess.run([str(v) for v in argv], cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if log is not None:
        with log.open("a") as stream:
            stream.write(f"$ {' '.join(str(v) for v in argv)}\n{result.stdout}\n")
    if result.returncode:
        raise SystemExit(f"{' '.join(str(v) for v in argv)} failed ({result.returncode}):\n"
                         f"{result.stdout[-4000:]}")
    return result.stdout, time.monotonic() - started


def macos_floor(tag: str) -> tuple[int, int] | None:
    match = re.search(r"macosx_(\d+)_(\d+)_(arm64|universal2)", tag)
    return (int(match.group(1)), int(match.group(2))) if match else None


def choose_wheel(package: dict, floor: tuple[int, int]) -> dict:
    """The newest arm64 macOS (or pure) cp312 wheel no newer than the floor."""
    best, best_key = None, None
    for wheel in package.get("wheels", []):
        name = wheel["url"].rsplit("/", 1)[1]
        parts = name[:-4].split("-")
        python_tag, abi_tag, platform_tag = parts[-3], parts[-2], parts[-1]
        if platform_tag == "any":
            if not any(tag in ("py3", "py2.py3", "py2") for tag in python_tag.split(".")) and python_tag != "cp312":
                continue
            key = (0, (0, 0))
        else:
            platform_version = None
            for tag in platform_tag.split("."):
                version = macos_floor(tag)
                if version is not None and version <= floor:
                    platform_version = max(platform_version or version, version)
            if platform_version is None:
                continue
            if python_tag == "cp312" and abi_tag == "cp312":
                key = (2, platform_version)
            elif abi_tag == "abi3" and python_tag.startswith("cp3") and int(python_tag[3:]) <= 12:
                key = (1, platform_version)
            elif abi_tag == "none" and python_tag == "py3":
                key = (1, platform_version)
            else:
                continue
        if best_key is None or key > best_key:
            best, best_key = wheel, key
    if best is None:
        raise SystemExit(f"no compatible wheel for {package['name']} {package['version']}")
    return best


def normalize(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def command_pins(args) -> None:
    import tomllib
    lock_bytes = args.lock.read_bytes()
    lock = tomllib.loads(lock_bytes.decode())
    inventory = json.loads(args.inventory.read_text())
    pins = json.loads((ROOT / "pins.json").read_text())
    floor = tuple(int(v) for v in pins["platform_floor"].split("."))
    licenses = {wheel["name"]: wheel.get("license") for wheel in pins.get("wheels", [])}
    packages = {normalize(package["name"]): package for package in lock["package"]}
    wheels = []
    for installed in inventory["installed_packages"]:
        package = packages[normalize(installed["name"])]
        if package["version"] != installed["version"]:
            raise SystemExit(f"{installed['name']}: lock {package['version']} != inventory {installed['version']}")
        if "editable" in package["source"] or "virtual" in package["source"]:
            continue  # ltx-core-mlx / ltx-pipelines-mlx come from the pinned source tree
        wheel = choose_wheel(package, floor)
        algorithm, value = wheel["hash"].split(":", 1)
        if algorithm != "sha256":
            raise SystemExit(f"{package['name']}: unexpected hash algorithm {algorithm}")
        name = normalize(package["name"])
        wheels.append({"name": name, "version": package["version"],
                       "filename": wheel["url"].rsplit("/", 1)[1], "url": wheel["url"],
                       "sha256": value, "bytes": wheel["size"], "license": licenses.get(name)})
    pins["ltx_source"]["uv_lock_sha256"] = hashlib.sha256(lock_bytes).hexdigest()
    pins["wheels"] = sorted(wheels, key=lambda wheel: wheel["name"])
    (ROOT / "pins.json").write_text(json.dumps(pins, indent=2) + "\n")
    print(f"pinned {len(wheels)} wheels")


def cache_key(pins: dict) -> str:
    material = hashlib.sha256()
    material.update(json.dumps(pins, sort_keys=True).encode())
    material.update(Path(__file__).read_bytes())
    for name in WORKER_FILES:
        material.update((QUALIFICATION / name).read_bytes())
    material.update((QUALIFICATION / RECEIPT).read_bytes())
    material.update(str(ASSEMBLY_VERSION).encode())
    return material.hexdigest()[:20]


def build_codec(pins: dict, cache: Path, work: Path, log: Path) -> tuple[Path, Path, dict]:
    """Static GPL ffmpeg/ffprobe with libx264, cached by its pins."""
    x264, ffmpeg = pins["x264"], pins["ffmpeg"]
    sdk = run(["xcrun", "--sdk", "macosx", "--show-sdk-path"])[0].strip()
    toolchain = [run(["xcrun", "--sdk", "macosx", "--show-sdk-version"])[0].strip(),
                 run(["xcrun", "clang", "--version"])[0].splitlines()[0]]
    key = hashlib.sha256(json.dumps([x264, ffmpeg, pins["platform_floor"], CODEC_RECIPE_VERSION,
                                     toolchain], sort_keys=True).encode()).hexdigest()[:16]
    built = cache / f"ai-codec-{key}"
    if (built / "build.json").is_file():
        report = json.loads((built / "build.json").read_text())
        if report.get("tree_sha256") == tree_digest(built / "bin"):
            return built / "bin/ffmpeg", built / "bin/ffprobe", report
        print(f"warning: cached codec {built} changed; rebuilding", file=sys.stderr)
        shutil.rmtree(built)
    env = dict(os.environ, MACOSX_DEPLOYMENT_TARGET=pins["platform_floor"],
               CFLAGS=f"-mmacosx-version-min={pins['platform_floor']}",
               LDFLAGS=f"-mmacosx-version-min={pins['platform_floor']}")
    env.pop("DEADPAN_FFMPEG_PREFIX", None)
    jobs = str(min(os.cpu_count() or 4, 12))
    prefix = work / "codec-prefix"

    source = work / "x264"
    run(["git", "init", "-q", source], log=log)
    run(["git", "-C", source, "fetch", "-q", "--depth", "1", x264["repository"], x264["commit"]], log=log)
    run(["git", "-C", source, "checkout", "-q", "FETCH_HEAD"], log=log)
    head = run(["git", "-C", source, "rev-parse", "HEAD"])[0].strip()
    tree = run(["git", "-C", source, "rev-parse", "HEAD^{tree}"])[0].strip()
    if head != x264["commit"] or tree != x264["tree"]:
        raise SystemExit(f"x264 checkout {head}/{tree} differs from its pin")
    run(["./configure", f"--prefix={prefix}", "--enable-static", "--disable-cli", "--enable-pic",
         "--disable-opencl", "--disable-avs", "--disable-swscale", "--disable-lavf",
         "--disable-ffms", "--disable-gpac", "--disable-lsmash", f"--extra-cflags=-isysroot {sdk}"],
        cwd=source, env=env, log=log)
    run(["make", f"-j{jobs}"], cwd=source, env=env, log=log)
    run(["make", "install"], cwd=source, env=env, log=log)

    archive = fetch(ffmpeg["archive_url"], ffmpeg["archive_sha256"], cache / Path(ffmpeg["archive_url"]).name)
    with tarfile.open(archive) as stream:
        stream.extractall(work, filter="data")
    ffmpeg_source = work / f"ffmpeg-{ffmpeg['version']}"
    configure = ["./configure", f"--prefix={prefix}", "--enable-static", "--disable-shared",
                 "--enable-gpl", "--enable-libx264", "--disable-nonfree", "--disable-version3",
                 "--disable-autodetect", "--enable-zlib", "--disable-network", "--disable-doc",
                 "--disable-debug", "--disable-ffplay", "--arch=aarch64", "--target-os=darwin",
                 f"--sysroot={sdk}", "--pkg-config-flags=--static",
                 f"--extra-cflags=-mmacosx-version-min={pins['platform_floor']}",
                 f"--extra-ldflags=-mmacosx-version-min={pins['platform_floor']}"]
    codec_env = dict(env, PKG_CONFIG_PATH=str(prefix / "lib/pkgconfig"), PKG_CONFIG_LIBDIR=str(prefix / "lib/pkgconfig"))
    run(configure, cwd=ffmpeg_source, env=codec_env, log=log)
    run(["make", f"-j{jobs}", "ffmpeg", "ffprobe"], cwd=ffmpeg_source, env=codec_env, log=log)
    staged = cache / f".ai-codec-{key}.{os.getpid()}"
    (staged / "bin").mkdir(parents=True)
    for program in ["ffmpeg", "ffprobe"]:
        shutil.copy2(ffmpeg_source / program, staged / "bin" / program)
        run(["strip", "-x", staged / "bin" / program])
    (staged / "licenses").mkdir()
    shutil.copy2(source / "COPYING", staged / "licenses/x264-COPYING")
    for name in ["COPYING.GPLv2", "LICENSE.md"]:
        shutil.copy2(ffmpeg_source / name, staged / "licenses" / f"ffmpeg-{name}")
    version = run([staged / "bin/ffmpeg", "-hide_banner", "-version"])[0]
    configuration = next(line.removeprefix("configuration: ") for line in version.splitlines()
                         if line.startswith("configuration: "))
    for host_path in [str(prefix), sdk]:
        configuration = configuration.replace(host_path, "<build>")
    report = {"configuration": configuration, "toolchain": toolchain,
              "x264_build": run([sys.executable, "-c", "print(open('x264.h').read().split('X264_BUILD')[1].split()[0])"], cwd=prefix / "include")[0].strip(),
              "tree_sha256": tree_digest(staged / "bin")}
    (staged / "build.json").write_text(json.dumps(report, indent=2))
    # Published by rename within the cache, so a crash never leaves a
    # half-copied entry that looks complete.
    os.rename(staged, built)
    return built / "bin/ffmpeg", built / "bin/ffprobe", report


def license_files(dist_info: Path) -> list[Path]:
    found = []
    for path in sorted(dist_info.rglob("*")):
        if path.is_file() and re.match(r"(?i)(licen[cs]e|copying|notice|authors)", path.name):
            found.append(path)
    return found


def command_build(args) -> None:
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise SystemExit("the AI runtime targets Apple Silicon macOS")
    if sys.version_info < (3, 11):
        raise SystemExit("tools/ai-runtime/build.py needs Python 3.11 or later on the build Mac")
    pins = json.loads((ROOT / "pins.json").read_text())
    cache = args.cache.resolve()
    cache.mkdir(parents=True, exist_ok=True)
    key = cache_key(pins)
    result = cache / f"ai-runtime-{key}"
    if (result / "report.json").is_file() and not args.rebuild:
        recorded = json.loads((result / "report.json").read_text()).get("tree_sha256")
        if recorded == tree_digest(result / "runtime"):
            print(json.dumps({"runtime": str(result / "runtime"), "notices": str(result / "notices"),
                              "report": str(result / "report.json"), "cached": True}))
            return
        print(f"warning: cached runtime {result} changed; rebuilding", file=sys.stderr)
    # Assembled inside the cache so publication is one rename on one volume.
    work = Path(tempfile.mkdtemp(prefix=".ai-runtime-work-", dir=cache))
    try:
        assemble(args, pins, cache, key, result, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def assemble(args, pins, cache, key, result, work) -> None:
    log = work / "build.log"
    started = time.monotonic()
    out = work / "result"
    runtime = out / "runtime"
    notices = out / "notices"
    notices.mkdir(parents=True)
    timings = {}

    # 1. CPython.
    python_pin = pins["python"]
    archive = fetch(python_pin["url"], python_pin["sha256"], cache / Path(python_pin["url"]).name.replace("%2B", "+"))
    with tarfile.open(archive) as stream:
        stream.extractall(runtime, filter="tar")
    python_root = runtime / "python"
    python = python_root / "bin/python3.12"
    version = run([python, "-I", "-c", "import sys; print(sys.version.split()[0])"])[0].strip()
    if version != python_pin["version"]:
        raise SystemExit(f"CPython {version} differs from {python_pin['version']}")
    stdlib = python_root / "lib/python3.12"
    shutil.copy2(stdlib / "LICENSE.txt", notices / "cpython-LICENSE.txt")
    timings["python"] = time.monotonic() - started

    # 2. The locked wheel set, offline and without dependency resolution.
    wheels = []
    for wheel in pins["wheels"]:
        wheels.append(fetch(wheel["url"], wheel["sha256"], cache / "wheels" / wheel["filename"]))
    before = {path.name for path in (python_root / "bin").iterdir()}
    run([python, "-I", "-m", "pip", "install", "--no-index", "--no-deps", "--no-compile",
         "--no-cache-dir", "--disable-pip-version-check", "--only-binary=:all:",
         "--no-warn-script-location", *wheels], log=log)
    # Console scripts carry absolute build shebangs and are never used.
    for path in (python_root / "bin").iterdir():
        if path.name not in before:
            path.unlink()
    site = stdlib / "site-packages"
    installed = {}
    for dist_info in site.glob("*.dist-info"):
        name, _, installed_version = dist_info.name.removesuffix(".dist-info").rpartition("-")
        installed[normalize(name)] = (installed_version, dist_info)
    for wheel in pins["wheels"]:
        installed_version, dist_info = installed.get(wheel["name"], (None, None))
        if installed_version != wheel["version"]:
            raise SystemExit(f"{wheel['name']} {wheel['version']} is not installed")
        # pip records the build machine's wheel path; drop it.
        direct_url = dist_info / "direct_url.json"
        if direct_url.exists():
            direct_url.unlink()
            record = dist_info / "RECORD"
            record.write_text("".join(line for line in record.read_text().splitlines(keepends=True)
                                      if not line.startswith(f"{dist_info.name}/direct_url.json,")))
        destination = notices / "wheels" / f"{wheel['name']}-{wheel['version']}"
        destination.mkdir(parents=True)
        for file in license_files(dist_info):
            shutil.copy2(file, destination / file.relative_to(dist_info).as_posix().replace("/", "_"))
    timings["wheels"] = time.monotonic() - started

    # 3. Remove parts no worker uses, including the installer.
    for relative in REMOVED_STDLIB:
        path = stdlib / relative
        if path.is_dir():
            shutil.rmtree(path)
        elif path.exists():
            path.unlink()
    for path in list(site.glob("pip-*.dist-info")):
        shutil.rmtree(path)
    for path in list((stdlib / "lib-dynload").glob("_tkinter*")):
        path.unlink()
    for pattern in ["libtcl*", "libtk*", "tcl*", "tk*", "itcl*", "thread*", "pkgconfig"]:
        for path in (python_root / "lib").glob(pattern):
            shutil.rmtree(path) if path.is_dir() else path.unlink()
    # Headers and manual pages; nothing compiles against the shipped runtime.
    for relative in ["include", "share"]:
        if (python_root / relative).is_dir():
            shutil.rmtree(python_root / relative)

    # Absolute search paths left by upstream wheel builds (for example
    # Pillow's /Users/runner/... CI directory) could load a library from
    # outside the bundle. Remove them, then re-sign the edited file ad hoc;
    # the bundle build signs everything again.
    stripped_rpaths = []
    for path in sorted(python_root.rglob("*")):
        if path.is_symlink() or not path.is_file() or path.suffix not in (".so", ".dylib"):
            continue
        load_commands = run(["otool", "-l", path])[0].splitlines()
        absolute = []
        for index, line in enumerate(load_commands):
            match = re.match(r"\s*path (.*) \(offset \d+\)$", line)
            if (match and "LC_RPATH" in "".join(load_commands[max(0, index - 2):index])
                    and not match.group(1).startswith("@")):
                absolute.append(match.group(1))
        for rpath in absolute:
            run(["install_name_tool", "-delete_rpath", rpath, path], log=log)
            stripped_rpaths.append({"file": str(path.relative_to(runtime)), "rpath": rpath})
        if absolute:
            run(["codesign", "--force", "-s", "-", path], log=log)

    # 4. The qualified LTX source, file by file against its pinned manifest.
    source_pin = pins["ltx_source"]
    manifest = json.loads((QUALIFICATION / "ltx-source-manifest.json").read_text())
    if manifest["commit"] != source_pin["commit"]:
        raise SystemExit("ltx-source-manifest.json names another commit")
    checkout = args.ltx_checkout
    if checkout is None:
        checkout = cache / f"ltx-2-mlx-{source_pin['commit']}"
        if not (checkout / ".git").is_dir():
            run(["git", "init", "-q", checkout], log=log)
            run(["git", "-C", checkout, "fetch", "-q", "--depth", "1", source_pin["repository"], source_pin["commit"]], log=log)
            run(["git", "-C", checkout, "checkout", "-q", "FETCH_HEAD"], log=log)
    ltx = runtime / "ltx-2-mlx"
    for relative, expected in sorted(manifest["files"].items()):
        source_file = checkout / relative
        data = source_file.read_bytes()
        if len(data) != expected["bytes"] or hashlib.sha256(data).hexdigest() != expected["sha256"]:
            raise SystemExit(f"LTX source {relative} differs from its manifest")
        target = ltx / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    license_data = (checkout / "LICENSE").read_bytes()
    if hashlib.sha256(license_data).hexdigest() != source_pin["license_sha256"]:
        raise SystemExit("ltx-2-mlx LICENSE differs from its pin")
    (notices / "ltx-2-mlx-LICENSE").write_bytes(license_data)
    # Relative .pth lines resolve against site-packages, so the tree relocates.
    (site / "deadpan-ltx.pth").write_text(
        "".join(f"../../../../ltx-2-mlx/packages/{package}/src\n"
                for package in ["ltx-core-mlx", "ltx-pipelines-mlx"]))

    # 5. The worker adapter and the model receipt it verifies.
    worker = runtime / "worker"
    for name in WORKER_FILES:
        target = worker / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(QUALIFICATION / name, target)
    (worker / RECEIPT).parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(QUALIFICATION / RECEIPT, worker / RECEIPT)

    # 6. The worker's GPL FFmpeg programs.
    ffmpeg, ffprobe, codec = build_codec(pins, cache, work, log)
    (runtime / "bin").mkdir()
    for program in [ffmpeg, ffprobe]:
        shutil.copy2(program, runtime / "bin" / program.name)
    for license_file in (ffmpeg.parent.parent / "licenses").iterdir():
        shutil.copy2(license_file, notices / license_file.name)
    timings["codec"] = time.monotonic() - started

    # 7. Bytecode, compiled once so the signed bundle is never written at run
    # time. Checked hashes tie each .pyc to its source bytes.
    for path in list(runtime.rglob("__pycache__")):
        shutil.rmtree(path)
    run([python, "-I", "-m", "compileall", "-q", "-j", "0", "--invalidation-mode", "checked-hash",
         "-s", str(runtime), "-p", "/ai-runtime", "-x", r"(bad_|lib2to3/tests)",
         stdlib, ltx, worker], log=log)

    # 8. A smoke check of the assembled tree, as the host launches it.
    check = ("import sys, mlx.core as mx, numpy, PIL, transformers, mlx_lm, safetensors, ltx_core_mlx, "
             "ltx_pipelines_mlx.keyframe_interpolation, ltx_pipelines_mlx.retake; a = mx.arange(10); "
             "assert int((a*a).sum().item()) == 285; print(mx.default_device())")
    device = run([python, "-I", "-B", "-c", check],
                 env={"PATH": "/usr/bin:/bin", "HOME": str(work)})[0].strip().splitlines()[-1]
    timings["total"] = time.monotonic() - started

    size = sum(path.stat().st_size for path in runtime.rglob("*") if path.is_file() and not path.is_symlink())
    report = {
        "schema": 1,
        "runtime_id": pins["runtime_id"],
        "runtime_version": pins["runtime_version"],
        "provider_runtime_versions": pins["provider_runtime_versions"],
        "cache_key": key,
        "platform_floor": pins["platform_floor"],
        "python": {"version": version, **{k: python_pin[k] for k in ["release", "url", "sha256", "license"]}},
        "wheels": pins["wheels"],
        "ltx_source": source_pin,
        "x264": pins["x264"],
        "ffmpeg": {**pins["ffmpeg"], "configuration": codec["configuration"],
                   "x264_build": codec["x264_build"]},
        "device_check": device,
        "stripped_rpaths": stripped_rpaths,
        "bytes": size,
        "tree_sha256": None,
        "timings_seconds": {k: round(v, 1) for k, v in timings.items()},
    }
    (runtime / "runtime.json").write_text(json.dumps({
        "schema": 1, "runtime_id": pins["runtime_id"], "runtime_version": pins["runtime_version"],
        "provider_runtime_versions": pins["provider_runtime_versions"],
        "python": version, "ltx_commit": source_pin["commit"],
        "minimum_macos": pins["platform_floor"],
        "worker": "worker/worker.py", "python_executable": "python/bin/python3.12",
        "ffmpeg": "bin/ffmpeg", "ffprobe": "bin/ffprobe", "runtime_source": "ltx-2-mlx",
    }, indent=2) + "\n")
    shutil.copy2(log, out / "build.log")
    report["tree_sha256"] = tree_digest(runtime)
    # The report is the completion marker and is written last.
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if result.exists():
        shutil.rmtree(result)
    os.rename(out, result)
    print(json.dumps({"runtime": str(result / "runtime"), "notices": str(result / "notices"),
                      "report": str(result / "report.json"), "cached": False}))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    pins = commands.add_parser("pins", help="regenerate wheel pins from uv.lock and the runtime inventory")
    pins.add_argument("--lock", type=Path, required=True)
    pins.add_argument("--inventory", type=Path,
                      default=QUALIFICATION / "evidence/2026-09-20-smoke/runtime-inventory.json")
    build = commands.add_parser("build", help="assemble the runtime into the cache and print its location")
    build.add_argument("--cache", type=Path, required=True)
    build.add_argument("--ltx-checkout", type=Path, help="an existing checkout to copy verified sources from")
    build.add_argument("--rebuild", action="store_true")
    args = parser.parse_args()
    {"pins": command_pins, "build": command_build}[args.command](args)


if __name__ == "__main__":
    main()
