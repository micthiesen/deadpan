#!/usr/bin/env python3
"""Wrap an already-built Deadpan executable in a local developer .app bundle.

This does not run Cargo, copy external dylibs, sign, notarize, or qualify a
standalone distribution. Output must be a new path ending in .app.
"""

import argparse
import json
import os
import plistlib
import shutil
import stat
import tempfile
import tomllib
from pathlib import Path

from brand.native_icons import compile_icons, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path, help="Existing deadpan-app executable")
    parser.add_argument("--output", required=True, type=Path, help="New .app path; never overwritten")
    parser.add_argument("--bundle-id", default="dev.deadpan.Deadpan")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve(strict=True)
    output = args.output.absolute()
    if output.suffix != ".app":
        parser.error("--output must end in .app")
    if output.exists() or output.is_symlink():
        parser.error(f"--output already exists: {output}")
    if not stat.S_ISREG(binary.stat().st_mode) or not os.access(binary, os.X_OK):
        parser.error("--binary must be a regular executable file")
    with binary.open("rb") as source:
        magic = source.read(4)
    if magic not in (b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"):
        parser.error("--binary must be a Mach-O executable")
    if not args.bundle_id or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-" for c in args.bundle_id):
        parser.error("--bundle-id must contain only ASCII letters, digits, dots, and hyphens")
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".deadpan-bundle-", dir=output.parent) as temporary:
        stage = Path(temporary)
        contents = stage / "Contents"
        macos = contents / "MacOS"
        resources = contents / "Resources"
        macos.mkdir(parents=True)
        resources.mkdir()
        compiled = stage / "compiled-icons"
        compiled.mkdir()
        compile_icons(root / "assets/brand/macos/Deadpan.icon", compiled)
        for name in ("Assets.car", "Deadpan.icns"):
            shutil.copy2(compiled / name, resources / name)
        shutil.copy2(binary, macos / "deadpan-app")
        # The transcription worker runs beside the app executable.
        worker = binary.parent / "deadpan-transcribe"
        if worker.is_file() and os.access(worker, os.X_OK):
            shutil.copy2(worker, macos / "deadpan-transcribe")
        else:
            print(f"warning: {worker} is missing; transcription will be unavailable in this bundle")
        info = {
            "CFBundleDevelopmentRegion": "en",
            "CFBundleDisplayName": "Deadpan",
            "CFBundleExecutable": "deadpan-app",
            "CFBundleIdentifier": args.bundle_id,
            "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundleName": "Deadpan",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version,
            "CFBundleVersion": version,
            "CFBundleIconName": "Deadpan",
            "CFBundleIconFile": "Deadpan.icns",
            "LSApplicationCategoryType": "public.app-category.video",
            "LSMinimumSystemVersion": "15.0",
            "NSHighResolutionCapable": True,
        }
        (contents / "Info.plist").write_bytes(plistlib.dumps(info))
        (contents / "PkgInfo").write_bytes(b"APPL????")
        provenance = {
            "kind": "local-developer-wrapper",
            "source_binary": str(binary),
            "xcode": run(["xcodebuild", "-version"]).strip(),
            "external_libraries": run(["otool", "-L", str(binary)]).splitlines()[1:],
            "limitation": "External build-host libraries remain required. Not a signed or standalone distribution.",
        }
        (resources / "developer-build.json").write_text(json.dumps(provenance, indent=2) + "\n")
        # Reserve the final path atomically. Even a race after the initial check
        # cannot replace an existing bundle or a symlink supplied by another task.
        output.mkdir()
        contents.rename(output / "Contents")
    print(output)
    print("Local developer bundle; external build-host libraries remain required.")


if __name__ == "__main__":
    main()
