"""Bind imported LTX Python modules to a checked, pinned source checkout."""

import hashlib
import importlib.util
from pathlib import Path
import sys


PACKAGE_PATHS = {
    "ltx_core_mlx": "packages/ltx-core-mlx/src/ltx_core_mlx",
    "ltx_pipelines_mlx": "packages/ltx-pipelines-mlx/src/ltx_pipelines_mlx",
}


def matches(path, expected):
    with path.open("rb") as stream:
        data = stream.read(expected["bytes"] + 1)
    return len(data) == expected["bytes"] and hashlib.sha256(data).hexdigest() == expected["sha256"]


def verify_tree(root, manifest, check_cancel):
    root = Path(root).resolve()
    for relative, expected in manifest["files"].items():
        check_cancel()
        path = root / relative
        if not path.resolve().is_relative_to(root):
            raise ValueError("runtime source escaped its checkout")
        if not matches(path, expected):
            raise ValueError(f"pinned runtime source mismatch: {relative}")


def verify_roots(root, lookup=importlib.util.find_spec):
    root = Path(root).resolve()
    for package, relative in PACKAGE_PATHS.items():
        spec = lookup(package)
        if spec is None or spec.origin is None or Path(spec.origin).resolve() != root / relative / "__init__.py":
            raise ValueError(f"unexpected installed runtime package: {package}")


def loaded_sources(root, manifest, modules=None):
    root = Path(root).resolve()
    sources = {}
    for name, module in list((sys.modules if modules is None else modules).items()):
        if not name.startswith("ltx_"):
            continue
        package = name.split(".")[0]
        if package not in PACKAGE_PATHS:
            raise ValueError(f"unexpected LTX module family: {name}")
        location = getattr(module, "__file__", None)
        if location is None:
            raise ValueError(f"runtime module has no attributable source: {name}")
        path = Path(location).resolve()
        expected_root = root / PACKAGE_PATHS[package]
        if not path.is_relative_to(expected_root):
            raise ValueError(f"runtime module was imported from another installation: {name}")
        relative = str(path.relative_to(root))
        expected = manifest["files"].get(relative)
        if expected is None or not matches(path, expected):
            raise ValueError(f"imported runtime module differs from its pinned source: {name}")
        # Include package search paths as well as their __init__.py; a package
        # cannot redirect a later lazy submodule into an alternate installation.
        if any(not Path(p).resolve().is_relative_to(expected_root)
               for p in getattr(module, "__path__", [])):
            raise ValueError(f"runtime package search path escaped: {name}")
        sources[relative] = expected["sha256"]
    return sources
