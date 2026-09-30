"""Compile and inspect Deadpan's editable Icon Composer document on macOS."""

import json
import plistlib
import struct
import subprocess
from pathlib import Path


ICON_NAME = "Deadpan"
ICON_SLOTS = {
    f"icon_{points}x{points}{suffix}.png": points * scale
    for points in (16, 32, 128, 256, 512)
    for suffix, scale in (("", 1), ("@2x", 2))
}


def run(arguments):
    result = subprocess.run(arguments, capture_output=True, text=True, check=False)
    if result.returncode:
        raise RuntimeError(
            f"{' '.join(map(str, arguments))} failed ({result.returncode}):\n"
            f"{result.stdout}{result.stderr}"
        )
    return result.stdout


def validate_iconset(iconset):
    actual = {path.name for path in iconset.glob("*.png")}
    if actual != set(ICON_SLOTS):
        raise RuntimeError(f"Incomplete iconset: {sorted(actual)}")
    for name, size in ICON_SLOTS.items():
        data = (iconset / name).read_bytes()
        if data[:8] != b"\x89PNG\r\n\x1a\n" or len(data) < 33:
            raise RuntimeError(f"Invalid PNG: {name}")
        width, height, depth, color = struct.unpack(">IIBB", data[16:26])
        if (width, height, depth, color) != (size, size, 8, 6):
            raise RuntimeError(f"Wrong size or RGBA format: {name}")


def compile_icons(source: Path, destination: Path):
    """Destination must exist and be empty. No Cargo or source edits occur."""
    if any(destination.iterdir()):
        raise RuntimeError(f"Icon compilation destination is not empty: {destination}")
    partial = destination / "icon-info.plist"
    run([
        "xcrun", "actool", str(source), "--compile", str(destination),
        "--platform", "macosx", "--minimum-deployment-target", "15.0",
        "--target-device", "mac", "--app-icon", ICON_NAME,
        "--output-partial-info-plist", str(partial),
        "--output-format", "human-readable-text",
    ])
    info = plistlib.loads(partial.read_bytes())
    if info.get("CFBundleIconName") != ICON_NAME:
        raise RuntimeError("actool did not emit the expected app icon identity")
    catalog = destination / "Assets.car"
    records = json.loads(run(["assetutil", "--info", str(catalog)]))
    stacks = [record for record in records if record.get("AssetType") == "IconImageStack"]
    appearances = {record.get("Appearance") for record in stacks
                   if record.get("Name") == ICON_NAME}
    expected = {"NSAppearanceNameAqua", "NSAppearanceNameDarkAqua", "ISAppearanceTintable"}
    if not expected.issubset(appearances):
        raise RuntimeError(f"Missing compiled icon appearances: {expected - appearances}")
    vectors = {record.get("Name") for record in records
               if record.get("AssetType") == "Vector"}
    if not {"Deadpan_Assets/01-frame", "Deadpan_Assets/02-hold"}.issubset(vectors):
        raise RuntimeError("Compiled icon does not retain both vector artwork layers")

    # Preserve Apple's exact legacy 128pt@2x render for bare executable launches.
    # The catalog's 256pt@1x uses a subtly different optical rendering.
    fallback = destination / "runtime.iconset"
    run(["iconutil", "-c", "iconset", "-o", str(fallback),
         str(destination / "Deadpan.icns")])

    # actool's compatibility .icns is intentionally sparse. Extract the complete
    # catalog by name, then package all ten standard 1x/2x slots explicitly.
    iconset = destination / "Deadpan.iconset"
    run(["iconutil", "-c", "iconset", "-o", str(iconset), str(catalog), ICON_NAME])
    validate_iconset(iconset)
    icon = destination / "Deadpan.icns"
    run(["iconutil", "-c", "icns", "-o", str(icon), str(iconset)])
    data = icon.read_bytes()
    if data[:4] != b"icns" or int.from_bytes(data[4:8], "big") != len(data):
        raise RuntimeError("Invalid compiled ICNS container")
    return records
