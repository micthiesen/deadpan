#!/usr/bin/env python3
# /// script
# dependencies = ["Pillow==11.3.0", "fonttools==4.59.0"]
# ///
"""Regenerate brand formats from the retained ImageGen-derived vector source.

Run `uv run tools/brand/export.py` on macOS with Xcode and rsvg-convert available.
The original generated images and the editable .icon are never overwritten.
"""

import hashlib
import json
import math
import os
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from PIL import Image, ImageDraw, ImageFont

from native_icons import compile_icons, run


ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets/brand"
ICON = ASSETS / "macos/Deadpan.icon"
SVG_NS = "http://www.w3.org/2000/svg"


def path_data(name):
    document = ET.parse(ICON / "Assets" / name)
    return document.find(f"{{{SVG_NS}}}path").attrib["d"]


def svg(viewbox, width, height, content):
    return (f'<svg xmlns="{SVG_NS}" width="{width}" height="{height}" '
            f'viewBox="{viewbox}"><title>Deadpan</title>{content}</svg>\n')


def wordmark_paths():
    font = instantiateVariableFont(TTFont(ASSETS / "source/Inter-Variable.ttf"),
                                   {"wght": 600, "opsz": 32}, inplace=False)
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    result = []
    x = 0
    for character in "Deadpan":
        name = cmap[ord(character)]
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        result.append(f'<path transform="translate({x} 0)" d="{pen.getCommands()}"/>')
        x += font["hmtx"][name][0] - 30
    return "".join(result), x + 30


def export_logos():
    frame, hold = path_data("01-frame.svg"), path_data("02-hold.svg")
    letters, advance = wordmark_paths()
    target = ASSETS / "logo"
    target.mkdir(exist_ok=True)
    for name, face, bar, text, mono in (
        ("color", "#C4B5FD", "#F6D365", "#17191D", False),
        ("on-light", "#17191D", "#F6D365", "#17191D", False),
        ("on-dark", "#C4B5FD", "#F6D365", "#E9EBF4", False),
        ("mono-black", "#000000", "#000000", "#000000", True),
        ("mono-white", "#FFFFFF", "#FFFFFF", "#FFFFFF", True),
    ):
        shape = frame + (" " + hold if mono else "")
        mark = f'<path fill="{face}" fill-rule="evenodd" d="{shape}"/>'
        if not mono:
            mark += f'<path fill="{bar}" d="{hold}"/>'
        width = math.ceil(478 + advance * 0.12)
        documents = {
            f"mark-{name}": svg("64 232 896 560", 1024, 640, mark),
            f"wordmark-{name}": svg(f"0 0 {math.ceil(advance * 0.12)} 256",
                                    math.ceil(advance * 0.12), 256,
                                    f'<g fill="{text}" transform="translate(0 191) scale(0.12 -0.12)">{letters}</g>'),
            f"lockup-{name}": svg(f"0 0 {width} 320", width, 320,
                                  f'<g transform="translate(-6.64 -54.88) scale(0.42)">{mark}</g>'
                                  f'<g fill="{text}" transform="translate(446 224) scale(0.12 -0.12)">{letters}</g>'),
        }
        for stem, document in documents.items():
            source = target / f"{stem}.svg"
            source.write_text(document)
            for format_name in ("png", "pdf"):
                run(["rsvg-convert", "--format", format_name, "--output",
                     str(target / f"{stem}.{format_name}"), str(source)])


def export_native():
    with tempfile.TemporaryDirectory(prefix="deadpan-brand-") as temporary:
        compiled = Path(temporary)
        records = compile_icons(ICON, compiled)
        for path in (compiled / "Deadpan.iconset").glob("*.png"):
            shutil.copy2(path, ASSETS / "macos/Deadpan.iconset" / path.name)
        shutil.copy2(compiled / "Deadpan.icns", ASSETS / "macos/Deadpan.icns")
        shutil.copy2(compiled / "runtime.iconset/icon_128x128@2x.png", ASSETS / "app-icon-256.png")
        shutil.copy2(compiled / "Deadpan.iconset/icon_512x512@2x.png", ASSETS / "app-icon-1024.png")
        evidence = ROOT / "docs/design/brand/compiled-assets.json"
        evidence.write_text(json.dumps(records, indent=2) + "\n")

    developer = Path(run(["xcode-select", "-p"]).strip())
    executable_directory = developer.parent / "Applications/Icon Composer.app/Contents/Executables"
    environment = dict(os.environ)
    environment["PATH"] = str(executable_directory) + os.pathsep + environment["PATH"]
    output = ASSETS / "macos/appearances"
    output.mkdir(exist_ok=True)
    for rendition in ("Default", "Dark", "ClearLight", "ClearDark", "TintedLight", "TintedDark"):
        subprocess.run([
            "ictool", str(ICON), "--export-image", "--output-file", str(output / f"{rendition}.png"),
            "--platform", "macOS", "--rendition", rendition,
            "--width", "1024", "--height", "1024", "--scale", "1",
        ], env=environment, check=True, capture_output=True, text=True)
        with Image.open(output / f"{rendition}.png") as image:
            if image.size != (1024, 1024) or image.mode != "RGBA":
                raise RuntimeError(f"Unexpected rendered appearance: {rendition}")


def export_web():
    target = ASSETS / "web"
    target.mkdir(exist_ok=True)
    image = Image.open(ASSETS / "macos/appearances/Default.png").convert("RGBA")
    image.save(target / "favicon.ico", sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)])
    for size in (16, 32, 48, 180, 192, 512):
        scaled = image.resize((size, size), Image.Resampling.LANCZOS)
        if size == 180:
            background = Image.new("RGBA", scaled.size, "#17191D")
            background.alpha_composite(scaled)
            background.convert("RGB").save(target / "apple-touch-icon.png")
        else:
            scaled.save(target / f"icon-{size}.png")
    frame, hold = path_data("01-frame.svg"), path_data("02-hold.svg")
    content = ('<rect width="1024" height="1024" rx="224" fill="#17191D"/>'
               f'<path fill="#C4B5FD" fill-rule="evenodd" d="{frame}"/>'
               f'<path fill="#F6D365" d="{hold}"/>')
    (target / "favicon.svg").write_text(svg("0 0 1024 1024", 1024, 1024, content))


def contact_sheet():
    sheet = Image.new("RGB", (1440, 820), "#ECEEF4")
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default(size=22)
    draw.text((32, 24), "Deadpan / native appearances and actual pixel sizes", font=font, fill="#17191D")
    modes = ("Default", "Dark", "ClearLight", "ClearDark", "TintedLight", "TintedDark")
    for index, name in enumerate(modes):
        x = 32 + index * 232
        draw.rectangle((x, 72, x + 208, 280), fill="#363B48" if "Dark" in name else "#FFFFFF")
        icon = Image.open(ASSETS / f"macos/appearances/{name}.png").convert("RGBA")
        icon = icon.resize((192, 192), Image.Resampling.LANCZOS)
        sheet.paste(icon, (x + 8, 80), icon)
        draw.text((x, 294), name, font=font, fill="#17191D")
    x = 32
    for size, filename in ((16, "icon_16x16.png"), (32, "icon_32x32.png"),
                           (64, "icon_32x32@2x.png"), (128, "icon_128x128.png"),
                           (256, "icon_256x256.png")):
        image = Image.open(ASSETS / "macos/Deadpan.iconset" / filename).convert("RGBA")
        sheet.paste(image, (x, 348), image)
        draw.text((x, 616), f"{size}px", font=font, fill="#17191D")
        x += size + 64
    mark = Image.open(ASSETS / "logo/lockup-on-light.png").convert("RGBA")
    mark.thumbnail((1320, 130), Image.Resampling.LANCZOS)
    sheet.paste(mark, (32, 676), mark)
    sheet.save(ROOT / "docs/design/brand/contact-sheet.png")


def manifest():
    paths = sorted(path for path in ASSETS.rglob("*") if path.is_file())
    paths += sorted((ROOT / "docs/design/brand/generated").glob("*.png"))
    paths += sorted((ROOT / "docs/design/brand/prompts").glob("*.txt"))
    records = []
    for path in paths:
        record = {"path": str(path.relative_to(ROOT)), "bytes": path.stat().st_size,
                  "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        if path.suffix == ".png":
            with Image.open(path) as image:
                record.update(width=image.width, height=image.height, mode=image.mode)
        records.append(record)
    data = {"date": "2026-09-30", "design_tool": "built-in image_gen.imagegen; model identifier not returned",
            "vector_source": "Optically regularized path reconstruction of generated concept A and selected master",
            "xcode": run(["xcodebuild", "-version"]).strip(),
            "rsvg": run(["rsvg-convert", "--version"]).strip(), "files": records}
    (ROOT / "docs/design/brand/manifest.json").write_text(json.dumps(data, indent=2) + "\n")


def main():
    export_logos()
    export_native()
    export_web()
    contact_sheet()
    manifest()
    print("Exported and validated Deadpan brand assets.")


if __name__ == "__main__":
    main()
