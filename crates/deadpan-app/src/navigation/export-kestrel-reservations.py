#!/usr/bin/env python3
"""Export a trusted local Kestrel registry for Deadpan's development audit.

Runs the actual Swift definitions with minimal data-only dependency shims. It
does not launch Kestrel, install shortcuts, or modify its source. Output is TSV;
redirect to a scratch file and review before replacing the checked fixture.
"""

import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="Kestrel Sources/Kestrel/Shortcuts.swift")
    args = parser.parse_args()
    source = args.source.read_bytes()
    shims = """
struct ShortcutCommand { let action: String; let arguments: [String] }
struct MachineProfile {
    static let current = MachineProfile()
    let communicationsSummary = "communications"
}
"""
    export = """
for definition in ShortcutRegistry.definitions {
    var mask = 0
    if definition.flags.contains(.maskControl) { mask |= 1 }
    if definition.flags.contains(.maskAlternate) { mask |= 2 }
    if definition.flags.contains(.maskShift) { mask |= 4 }
    if definition.flags.contains(.maskCommand) { mask |= 8 }
    print([String(definition.key), String(mask), definition.bundle ?? "*",
           definition.chord, definition.command.action].joined(separator: "\\t"))
}
"""
    with tempfile.TemporaryDirectory(prefix="deadpan-kestrel-export-") as directory:
        directory = Path(directory)
        script = directory / "main.swift"
        script.write_text(shims + source.decode("utf-8") + export)
        result = subprocess.run(
            ["swift", "-module-cache-path", str(directory / "cache"), str(script)],
            check=True,
            capture_output=True,
            text=True,
        )
    print("# Kestrel ShortcutRegistry export. See docs/KEYBINDING_COMPATIBILITY.md.")
    print("# source-sha256=" + hashlib.sha256(source).hexdigest())
    print("# keycode\tmodifier-mask (ctrl=1,option=2,shift=4,command=8)\tbundle\tchord\taction")
    print(result.stdout, end="")


if __name__ == "__main__":
    main()
