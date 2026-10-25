#!/usr/bin/env python3
"""Pure scratch-only tests for collector publication guards; no collection."""
import runpy
import tempfile
from pathlib import Path

root = Path(__file__).resolve().parent
collector = runpy.run_path(str(root / "collect-regions.py"))
totals = collector["package_totals"]
verify = collector["verify_archive_witnesses"]
verify_plan = collector["verify_archive_plan"]
identity = collector["identity"]

# Exact boundary includes the standalone files.json bytes and each metadata
# document in decoded/raw totals. One byte over fails before publication.
inventory = {"payload.bin": {"bytes": 20, "sha256": "0" * 64}}
metadata = [b"m" * 70]
result = totals(0, inventory, b"i" * 10, [], metadata, 80)
assert result == (30, 80, 80, 80), result
try:
    totals(0, inventory, b"i" * 10, [], [b"m" * 71], 80)
except RuntimeError as exc:
    assert "decoded package including identity metadata" in str(exc), exc
else:
    raise AssertionError("one-byte-over metadata passed the decoded bound")

# A source tree that changes after archive snapshot refuses publication.
with tempfile.TemporaryDirectory(prefix="regions-collector-guard-") as td:
    archive_root = Path(td)
    source = archive_root / "input.bin"
    source.write_bytes(b"before")
    witness = {"input.bin": identity(source, 100)}
    verify({str(archive_root): witness}, 100)
    source.write_bytes(b"after!")
    try:
        verify_plan(archive_root, [{"path": "input.bin", **witness["input.bin"]}], 100)
    except RuntimeError as exc:
        assert "archive source differs from reviewed plan" in str(exc), exc
    else:
        raise AssertionError("pre-capture archive change passed the reviewed-plan guard")
    try:
        verify_plan(archive_root, None, 100)
    except RuntimeError as exc:
        assert "archive lacks reviewed expected_members" in str(exc), exc
    else:
        raise AssertionError("missing reviewed archive inventory was accepted")
    try:
        verify({str(archive_root): witness}, 100)
    except RuntimeError as exc:
        assert "archive changed during collection" in str(exc), exc
    else:
        raise AssertionError("mutated archive tree passed the publication guard")

print("collector guard tests: exact cap accepted; metadata over-cap, pre-capture mismatch, missing inventory, and post-capture mutation refused")
